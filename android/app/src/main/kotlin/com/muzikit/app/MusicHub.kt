package com.muzikit.app

import android.content.Context
import android.content.SharedPreferences
import android.content.Intent
import android.net.Uri
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.Player
import com.muzikit.app.library.LibraryReport
import com.muzikit.app.library.LibraryScanner
import com.muzikit.app.library.buildReport
import com.muzikit.app.playback.SleepTimer
import com.muzikit.app.library.SortMode
import com.muzikit.app.library.Song
import com.muzikit.app.library.filterSongs
import com.muzikit.app.queue.LoopMode
import com.muzikit.app.queue.PlayEngine
import com.muzikit.app.queue.PrevAction
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class LibraryState(
    val folder: Uri? = null,
    val songs: List<Song> = emptyList(),
    val loading: Boolean = false,
    val error: String? = null,
    /** Klasöre yazma izni var mı (indirici için). */
    val writable: Boolean = false,
)

data class ViewState(
    val query: String = "",
    val sort: SortMode = SortMode.NUMBER,
    val onlyFavorites: Boolean = false,
)

data class PlayState(
    val playing: String? = null,
    val shuffle: Boolean = false,
    val loop: LoopMode = LoopMode.OFF,
    val queue: List<String> = emptyList(),
    val recent: List<String> = emptyList(),
)

data class Toast(val text: String, val id: Long)

/** Kullanıcı ayarları (kalıcı). */
data class Settings(
    /** Kulaklık yoksa hoparlörden asla çalma. */
    val headphoneOnly: Boolean = false,
    /** Bluetooth kulaklık 5 dk içinde geri gelirse kaldığı yerden devam et. */
    val btResume: Boolean = true,
    /** Medya sesi 0'a inince duraklat, geri gelince devam et. */
    val mutePause: Boolean = true,
    /** Alttaki panelde gerçek frekans analizörü. */
    val analyzer: Boolean = true,
    /** Arka planda yavaş akan veri sütunları ve dönen radar. Kapalıyken sadece sabit ızgara. */
    val motion: Boolean = true,
    /** Tek tuşlu kulaklıkta çift basış = sonraki, üçlü = önceki (Media3). Kapalıyken tek basış anında oynat/duraklat. */
    val headsetMultiTap: Boolean = false,
)

/** Arayüze tek seferlik istekler (klavye kısayolları). */
enum class UiEvent { FOCUS_SEARCH, REVEAL_PLAYING }

/**
 * Uygulamanın tek merkezi (süreç boyunca tek örnek, ana iş parçacığı).
 * Kütüphane, görünüm (arama/sıralama/favori), çalma motoru ve kalıcı ayarlar burada.
 * Hem arayüz hem PlaybackService (bildirim/kulaklık tuşları) aynı kurallardan geçer.
 *
 * Arka planda hiçbir döngü/sayaç çalıştırmaz: sadece olay gelince (şarkı bitti, tuşa basıldı) çalışır.
 */
object MusicHub {

    private lateinit var app: Context
    private lateinit var prefs: SharedPreferences
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val engine = PlayEngine()

    /** PlaybackService'in oynatıcısı (servis yaşarken). */
    private var player: Player? = null

    private val _library = MutableStateFlow(LibraryState())
    val library: StateFlow<LibraryState> = _library.asStateFlow()
    private val _view = MutableStateFlow(ViewState())
    val view: StateFlow<ViewState> = _view.asStateFlow()
    private val _favorites = MutableStateFlow<Set<String>>(emptySet())
    val favorites: StateFlow<Set<String>> = _favorites.asStateFlow()
    private val _visible = MutableStateFlow<List<Song>>(emptyList())
    val visible: StateFlow<List<Song>> = _visible.asStateFlow()
    private val _play = MutableStateFlow(PlayState())
    val play: StateFlow<PlayState> = _play.asStateFlow()
    private val _toast = MutableStateFlow<Toast?>(null)
    val toast: StateFlow<Toast?> = _toast.asStateFlow()
    private val _settings = MutableStateFlow(Settings())
    val settings: StateFlow<Settings> = _settings.asStateFlow()
    private val _sleep = MutableStateFlow<SleepTimer>(SleepTimer.Off)
    val sleep: StateFlow<SleepTimer> = _sleep.asStateFlow()
    private val _report = MutableStateFlow<LibraryReport?>(null)
    val report: StateFlow<LibraryReport?> = _report.asStateFlow()

    /** Arayüz görünür mü (uyarılar ekranda mı yoksa sistem bildirimi olarak mı gösterilsin). */
    @Volatile var uiVisible = false

    /** Servis kurar: "şu an kulaklık var mı / çalmaya izin var mı" sorusu. */
    var playGate: (() -> Boolean)? = null

    /** Servis kurar: sadece kulaklık kilidini hemen yeniden uygula (ayar değişince). */
    var lockHook: (() -> Unit)? = null

    private val main = Handler(Looper.getMainLooper())
    private val sleepRunnable = Runnable { onSleepTimeUp() }

    private var byFile: Map<String, Song> = emptyMap()
    private var initialized = false

    fun init(context: Context) {
        if (initialized) return
        initialized = true
        app = context.applicationContext
        prefs = app.getSharedPreferences("muzikit", Context.MODE_PRIVATE)
        _view.value = ViewState(
            sort = SortMode.fromKey(prefs.getString(K_SORT, null)),
            onlyFavorites = prefs.getBoolean(K_ONLY_FAV, false),
        )
        _favorites.value = prefs.getStringSet(K_FAVS, emptySet())!!.toSet()
        engine.setShuffle(prefs.getBoolean(K_SHUFFLE, false))
        engine.loop = LoopMode.fromKey(prefs.getString(K_LOOP, null))
        engine.restoreRecent(prefs.getString(K_RECENT, "")!!.split('\n').filter { it.isNotEmpty() })
        _settings.value = Settings(
            headphoneOnly = prefs.getBoolean(K_HP_ONLY, false),
            btResume = prefs.getBoolean(K_BT_RESUME, true),
            mutePause = prefs.getBoolean(K_MUTE_PAUSE, true),
            analyzer = prefs.getBoolean(K_ANALYZER, true),
            motion = prefs.getBoolean(K_MOTION, true),
            headsetMultiTap = prefs.getBoolean(K_HEADSET_MULTI, false),
        )
        publishPlay()

        val saved = prefs.getString(K_FOLDER, null)?.let(Uri::parse)
        if (saved != null && app.contentResolver.persistedUriPermissions.any { it.uri == saved && it.isReadPermission }) {
            load(saved)
        }
    }

    // ── Kütüphane ────────────────────────────────────────

    fun onFolderPicked(uri: Uri) {
        val cr = app.contentResolver
        // Okuma + yazma (indirici klasöre yeni şarkı ve downloaded.txt yazar). Yazma verilmediyse sadece okuma.
        val rw = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        runCatching { cr.takePersistableUriPermission(uri, rw) }
            .recoverCatching { cr.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
            .onFailure { Log.w(TAG, "kalıcı izin alınamadı", it) }
        cr.persistedUriPermissions.filter { it.uri != uri }.forEach { old ->
            runCatching { cr.releasePersistableUriPermission(old.uri, rw) }
                .recoverCatching { cr.releasePersistableUriPermission(old.uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
        }
        prefs.edit().putString(K_FOLDER, uri.toString()).apply()
        load(uri)
    }

    fun reload() { _library.value.folder?.let(::load) }

    private fun load(uri: Uri) {
        _library.value = _library.value.copy(folder = uri, loading = true, error = null)
        scope.launch {
            val result = withContext(Dispatchers.IO) { runCatching { LibraryScanner.scan(app, uri) } }
            result.onSuccess { scan ->
                val songs = scan.songs
                byFile = songs.associateBy { it.file }
                engine.prune { it in byFile }
                _library.value = LibraryState(
                    folder = uri, songs = songs,
                    writable = app.contentResolver.persistedUriPermissions.any { it.uri == uri && it.isWritePermission },
                )
                _report.value = buildReport(songs, scan.downloadedTxt)
                Log.i(TAG, "tarama: ${songs.size} şarkı")
            }.onFailure {
                Log.e(TAG, "tarama hatası", it)
                _library.value = LibraryState(folder = uri, error = "klasör okunamadı: ${it.message}")
            }
            rebuildVisible()
            publishPlay()
            restoreLastIfIdle()
            drainReady()
        }
    }

    fun song(file: String?): Song? = file?.let(byFile::get)

    /** Eşitleme kayması: favori, son çalınanlar, kuyruk, son şarkı yeni ada taşınır (kütüphane sonra yeniden taranır). */
    fun renameFile(old: String, new: String) {
        if (old in _favorites.value) {
            _favorites.value = _favorites.value - old + new
            prefs.edit().putStringSet(K_FAVS, _favorites.value).apply()
        }
        engine.renameFile(old, new)
        if (prefs.getString(K_LAST, null) == old) prefs.edit().putString(K_LAST, new).apply()
        prefs.edit().putString(K_RECENT, engine.recent.joinToString("\n")).apply()
        publishPlay()
    }

    // ── Widget gibi dış tetikleyiciler için ──────────────

    private val readyQueue = ArrayList<() -> Unit>()

    fun hasPlayer(): Boolean = player != null

    /** Kütüphane yüklenince çalıştır (klasör yoksa / hata varsa hemen). */
    fun whenLibraryReady(block: () -> Unit) {
        val l = _library.value
        if (!l.loading && (l.songs.isNotEmpty() || l.folder == null || l.error != null)) block() else readyQueue += block
    }

    private fun drainReady() {
        val q = readyQueue.toList()
        readyQueue.clear()
        q.forEach { it() }
    }

    // ── Görünüm: arama, sıralama, favoriler ─────────────

    fun setQuery(q: String) { _view.value = _view.value.copy(query = q); rebuildVisible() }

    fun cycleSort() {
        val s = _view.value.sort.next()
        _view.value = _view.value.copy(sort = s)
        prefs.edit().putString(K_SORT, s.name).apply()
        rebuildVisible()
    }

    fun toggleOnlyFavorites() {
        val on = !_view.value.onlyFavorites
        _view.value = _view.value.copy(onlyFavorites = on)
        prefs.edit().putBoolean(K_ONLY_FAV, on).apply()
        rebuildVisible()
        toast(if (on) "sadece favoriler" else "tüm şarkılar")
    }

    fun toggleFavorite(file: String) {
        val cur = _favorites.value
        val added = file !in cur
        _favorites.value = if (added) cur + file else cur - file
        prefs.edit().putStringSet(K_FAVS, _favorites.value).apply()
        toast(if (added) "★ favorilere eklendi" else "☆ favorilerden çıkarıldı")
        if (_view.value.onlyFavorites) rebuildVisible()
    }

    private fun rebuildVisible() {
        val v = _view.value
        _visible.value = filterSongs(_library.value.songs, v.query, v.onlyFavorites, _favorites.value, v.sort)
    }

    /** Masaüstü `play_context`: çalan şarkı görünür listedeyse o liste, değilse tüm kütüphane. */
    private fun context(): List<String> {
        val vis = _visible.value
        val playing = engine.playing
        val inView = playing != null && vis.any { it.file == playing }
        val src = if (inView || (playing == null && vis.isNotEmpty())) vis else _library.value.songs
        return src.map { it.file }
    }

    // ── Çalma ────────────────────────────────────────────

    fun attachPlayer(p: Player) {
        player = p
        p.repeatMode = if (engine.loop == LoopMode.ONE) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
        restoreLastIfIdle()
    }

    /**
     * Kaldığı yeri hatırlama: oynatıcı boşsa son şarkıyı son konumunda, DURAKLATILMIŞ olarak yükle
     * (ses çıkmaz; oynat'a basınca oradan devam eder).
     */
    private fun restoreLastIfIdle() {
        val p = player ?: return
        if (p.mediaItemCount > 0) return
        val last = prefs.getString(K_LAST, null) ?: return
        val s = byFile[last] ?: return
        val pos = prefs.getLong(K_LAST_POS, 0)
        engine.play(last, pushHistory = false)
        p.setMediaItem(toItem(s), pos)
        p.playWhenReady = false
        p.prepare()
        publishPlay()
        Log.i(TAG, "son şarkı geri yüklendi: $last @ ${pos / 1000}s")
    }

    /** Son şarkı + konum kaydı (duraklatınca, şarkı değişince, servis kapanırken, çalarken 30 sn'de bir). */
    fun savePosition() {
        val p = player ?: return
        val id = p.currentMediaItem?.mediaId ?: return
        // commit(): hemen diske yaz (süreç az sonra öldürülse de konum kaybolmasın; nadir ve küçük bir yazma).
        prefs.edit().putString(K_LAST, id).putLong(K_LAST_POS, p.currentPosition.coerceAtLeast(0)).commit()
    }

    /** Çalmaya izin var mı (sadece kulaklık modu). Yoksa uyarır. */
    fun allowPlay(): Boolean {
        if (playGate?.invoke() != false) return true
        toast("kulaklık bağlı değil")
        return false
    }

    // ── Ayarlar ──────────────────────────────────────────

    fun setHeadphoneOnly(on: Boolean) = updateSettings(_settings.value.copy(headphoneOnly = on)) {
        putBoolean(K_HP_ONLY, on)
    }.also {
        lockHook?.invoke() // ses kapısı, sabitleme ve hoparlör sesi hemen güncellensin
        if (on && player?.isPlaying == true && playGate?.invoke() == false) { player?.pause(); toast("kulaklık bağlı değil") }
    }

    fun setBtResume(on: Boolean) = updateSettings(_settings.value.copy(btResume = on)) { putBoolean(K_BT_RESUME, on) }
    fun setHeadsetMultiTap(on: Boolean) =
        updateSettings(_settings.value.copy(headsetMultiTap = on)) { putBoolean(K_HEADSET_MULTI, on) }

    // ── Klavye kısayolları ───────────────────────────────

    private val _events = kotlinx.coroutines.flow.MutableSharedFlow<UiEvent>(extraBufferCapacity = 4)
    val events: kotlinx.coroutines.flow.SharedFlow<UiEvent> = _events

    fun requestUi(e: UiEvent) { _events.tryEmit(e) }

    /** Herhangi bir yazı kutusu (arama, indirme linki) odakta mı: o sırada klavye kısayolları devre dışı. */
    @Volatile var textInputFocused = false

    fun seekBy(deltaMs: Long) {
        val p = player ?: return
        if (p.mediaItemCount == 0) return
        val d = p.duration.takeIf { it > 0 } ?: Long.MAX_VALUE
        p.seekTo((p.currentPosition + deltaMs).coerceIn(0, d))
    }

    /** Sistem medya sesi (ses tuşlarıyla aynı). [direction]: ADJUST_RAISE / ADJUST_LOWER / ADJUST_TOGGLE_MUTE. */
    fun adjustVolume(direction: Int) {
        app.getSystemService(android.media.AudioManager::class.java)
            .adjustStreamVolume(android.media.AudioManager.STREAM_MUSIC, direction, android.media.AudioManager.FLAG_SHOW_UI)
    }

    fun toggleFavoriteCurrent() { engine.playing?.let(::toggleFavorite) }

    fun setAnalyzer(on: Boolean) = updateSettings(_settings.value.copy(analyzer = on)) { putBoolean(K_ANALYZER, on) }
    fun setMotion(on: Boolean) = updateSettings(_settings.value.copy(motion = on)) { putBoolean(K_MOTION, on) }

    /** Analizör için: oynatıcının şarkıdaki tam konumu (µs) ve çalıyor mu. Sadece ana iş parçacığından. */
    fun playbackPositionUs(): Long = (player?.currentPosition ?: 0L) * 1000L
    fun isPlaying(): Boolean = player?.isPlaying == true

    fun setMutePause(on: Boolean) = updateSettings(_settings.value.copy(mutePause = on)) { putBoolean(K_MUTE_PAUSE, on) }

    private inline fun updateSettings(s: Settings, save: SharedPreferences.Editor.() -> Unit) {
        _settings.value = s
        prefs.edit().apply(save).apply()
    }

    // ── Uyku zamanlayıcısı ───────────────────────────────

    fun cycleSleep() {
        val now = SystemClock.elapsedRealtime()
        val t = SleepTimer.cycle(_sleep.value, now)
        applySleep(t)
        toast("uyku: " + SleepTimer.longLabel(t, now))
    }

    private fun applySleep(t: SleepTimer) {
        _sleep.value = t
        main.removeCallbacks(sleepRunnable)
        if (t is SleepTimer.At) main.postAtTime(sleepRunnable, SystemClock.uptimeMillis() + (t.atMs - SystemClock.elapsedRealtime()))
        // "Şarkı sonu": döngü "tek"te şarkı hiç bitmez (başa sarar); orada ExoPlayer'ın kendisi duraklatır.
        // Diğer durumlarda şarkı bitince onTrackEnded durdurur.
        (player as? androidx.media3.common.ForwardingPlayer)?.let { fp ->
            (fp.wrappedPlayer as? androidx.media3.exoplayer.ExoPlayer)?.pauseAtEndOfMediaItems = t == SleepTimer.EndOfTrack
        }
    }

    private fun onSleepTimeUp() {
        val t = _sleep.value as? SleepTimer.At ?: return
        val left = t.atMs - SystemClock.elapsedRealtime()
        // Handler uykuda geç kalabilir/erken gelebilir: gerçek saate göre kontrol et.
        if (left > 1000) { main.postAtTime(sleepRunnable, SystemClock.uptimeMillis() + left); return }
        applySleep(SleepTimer.Off)
        player?.pause()
        toast("uyku zamanlayıcısı: duraklatıldı")
        Log.i(TAG, "uyku: süre doldu")
    }

    /** ExoPlayer "şarkı sonunda duraklat" ile durdu (PlaybackService çağırır). */
    fun onPausedAtEndOfTrack() {
        if (_sleep.value != SleepTimer.EndOfTrack) return
        applySleep(SleepTimer.Off)
        toast("uyku zamanlayıcısı: şarkı sonunda durdu")
    }

    fun detachPlayer(p: Player) { if (player === p) player = null }

    private fun exists(file: String) = file in byFile

    /** Listeden dokunulan şarkıyı çal. */
    fun playFile(file: String, pushHistory: Boolean = true) {
        val p = player ?: return
        val s = byFile[file] ?: return
        engine.play(file, pushHistory)
        p.setMediaItem(toItem(s))
        p.prepare()
        p.play()
        prefs.edit()
            .putString(K_LAST, file)
            .putLong(K_LAST_POS, 0)
            .putString(K_RECENT, engine.recent.joinToString("\n"))
            .apply()
        publishPlay()
    }

    /** Sonraki (elle: liste sonunda başa döner). */
    fun next(manual: Boolean) {
        val f = engine.next(manual, context(), ::exists)
        if (f != null) playFile(f) else {
            toast("liste bitti")
            publishPlay()
        }
    }

    fun prev() {
        val pos = player?.currentPosition ?: 0
        when (val a = engine.prev(pos, context(), ::exists)) {
            PrevAction.SeekToStart -> player?.seekTo(0)
            is PrevAction.Play -> playFile(a.file, pushHistory = false)
            PrevAction.Nothing -> Unit
        }
    }

    /** Oynat/duraklat; hiçbir şey yüklü değilse son şarkıyı ya da listedeki ilki başlat. */
    fun togglePlay() {
        val p = player ?: return
        if (!p.isPlaying && !allowPlay()) return
        if (p.mediaItemCount == 0) { startSomething(); return }
        if (p.isPlaying) p.pause() else {
            if (p.playbackState == Player.STATE_IDLE) p.prepare()
            if (p.playbackState == Player.STATE_ENDED) p.seekToDefaultPosition()
            p.play()
        }
    }

    fun startSomething() {
        val last = prefs.getString(K_LAST, null)?.takeIf(::exists)
        if (last != null) playFile(last) else next(manual = true)
    }

    /** Şarkı kendiliğinden bitti (PlaybackService çağırır). */
    fun onTrackEnded() {
        // Uyku "şarkı sonu": sıradakine geçme, dur (masaüstüyle aynı).
        if (_sleep.value == SleepTimer.EndOfTrack) {
            applySleep(SleepTimer.Off)
            toast("uyku zamanlayıcısı: şarkı sonunda durdu")
            return
        }
        next(manual = false)
    }

    fun toggleShuffle() {
        engine.setShuffle(!engine.shuffle)
        prefs.edit().putBoolean(K_SHUFFLE, engine.shuffle).apply()
        toast(if (engine.shuffle) "karıştır: açık" else "karıştır: kapalı")
        publishPlay()
    }

    fun cycleLoop() {
        engine.loop = engine.loop.next()
        player?.repeatMode = if (engine.loop == LoopMode.ONE) Player.REPEAT_MODE_ONE else Player.REPEAT_MODE_OFF
        prefs.edit().putString(K_LOOP, engine.loop.name).apply()
        toast("döngü: ${engine.loop.label}")
        publishPlay()
    }

    fun enqueue(file: String, front: Boolean) {
        if (!exists(file)) return
        engine.enqueue(file, front)
        toast(if (front) "sıradaki olarak eklendi" else "sıraya eklendi (${engine.queue.size})")
        publishPlay()
    }

    fun queueMove(i: Int, up: Boolean) { engine.queueMove(i, up); publishPlay() }
    fun queueRemove(i: Int) { engine.queueRemove(i); publishPlay() }
    fun queueClear() { engine.queueClear(); publishPlay() }

    private fun publishPlay() {
        _play.value = PlayState(
            playing = engine.playing,
            shuffle = engine.shuffle,
            loop = engine.loop,
            queue = engine.queue.toList(),
            recent = engine.recent.toList(),
        )
    }

    private var toastSeq = 0L
    fun toast(text: String) {
        _toast.value = Toast(text, ++toastSeq)
        // Ekran kapalı / uygulama arka plandaysa kısa sistem uyarısı.
        if (!uiVisible && initialized) {
            runCatching { android.widget.Toast.makeText(app, "muzikit: $text", android.widget.Toast.LENGTH_SHORT).show() }
        }
    }
    fun clearToast(id: Long) { if (_toast.value?.id == id) _toast.value = null }

    private fun toItem(s: Song): MediaItem {
        val uri = Uri.parse(s.uri)
        return MediaItem.Builder()
            .setMediaId(s.file)
            .setUri(uri)
            .setRequestMetadata(MediaItem.RequestMetadata.Builder().setMediaUri(uri).build())
            .setMediaMetadata(
                MediaMetadata.Builder()
                    .setTitle(s.title)
                    .setArtist(s.number?.let { "#" + s.numberLabel } ?: "muzikit")
                    .build()
            )
            .build()
    }

    private const val K_FOLDER = "folder"
    private const val K_SORT = "sort"
    private const val K_ONLY_FAV = "only_fav"
    private const val K_FAVS = "favorites"
    private const val K_SHUFFLE = "shuffle"
    private const val K_LOOP = "loop"
    private const val K_RECENT = "recent"
    private const val K_LAST = "last_track"
    private const val K_LAST_POS = "last_pos"
    private const val K_HP_ONLY = "headphone_only"
    private const val K_BT_RESUME = "bt_resume"
    private const val K_MUTE_PAUSE = "mute_pause"
    private const val K_ANALYZER = "analyzer"
    private const val K_MOTION = "motion"
    private const val K_HEADSET_MULTI = "headset_multi_tap"
}

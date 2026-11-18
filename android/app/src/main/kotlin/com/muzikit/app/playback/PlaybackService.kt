package com.muzikit.app.playback

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import androidx.media3.common.Player
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.KeyEvent
import androidx.media3.common.PlaybackException
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import com.muzikit.app.spectrum.SpectrumTap
import com.muzikit.app.widget.PlayerWidget
import androidx.media3.session.CommandButton
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionResult
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.muzikit.app.MainActivity
import com.muzikit.app.MusicHub
import com.muzikit.app.TAG

/**
 * Arka planda çalmayı yürüten servis. Ekran kapalıyken / uygulama arka plandayken sadece bu çalışır:
 * ExoPlayer + medya oturumu. Arayüz (animasyon, ilerleme sayacı) arka planda hiçbir şey yapmaz.
 *
 * Media3 bunlardan kendiliğinden sorumlu: ön plan servisi (mediaPlayback), bildirim, kilit ekranı,
 * kulaklık/Bluetooth/araba tuşları, bildirimdeki ilerleme çubuğuyla saniyeye sarma.
 */
class PlaybackService : MediaSessionService() {

    private var session: MediaSession? = null
    private val mutePause = MutePause()
    private val guard = OutputGuard()
    private val main = Handler(Looper.getMainLooper())
    private lateinit var audio: AudioManager

    // ── Kulaklık / Bluetooth ─────────────────────────────

    private fun AudioDeviceInfo.toOutput(): Output {
        val kind = when (type) {
            AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
            AudioDeviceInfo.TYPE_WIRED_HEADSET,
            AudioDeviceInfo.TYPE_USB_HEADSET -> OutputKind.WIRED
            AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
            AudioDeviceInfo.TYPE_BLE_HEADSET,
            AudioDeviceInfo.TYPE_HEARING_AID -> OutputKind.BLUETOOTH
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER,
            AudioDeviceInfo.TYPE_BUILTIN_SPEAKER_SAFE -> OutputKind.SPEAKER
            // TYPE_BLE_SPEAKER, hoparlör, SCO (arama), HDMI… kulaklık sayılmaz
            else -> OutputKind.OTHER
        }
        return Output("$type:$productName", kind)
    }

    private fun currentOutputs(): List<Output> =
        audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).map { it.toOutput() }

    // ── Sadece kulaklık kilidi (şüphede sessiz) ──────────

    private var exo: ExoPlayer? = null
    private val lockPrefs by lazy { getSharedPreferences("headphone_lock", MODE_PRIVATE) }

    private val mediaAttrs by lazy {
        android.media.AudioAttributes.Builder()
            .setUsage(android.media.AudioAttributes.USAGE_MEDIA)
            .setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC)
            .build()
    }

    /** Medya sesinin ŞU AN gittiği çıkış(lar) — Android'in kendi yönlendirme kararı. */
    private fun mediaRouteDevices(): List<AudioDeviceInfo> =
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            runCatching { audio.getAudioDevicesForAttributes(mediaAttrs) }.getOrDefault(emptyList())
        } else {
            // Eski Android: kulaklık varsa medya ona gider, yoksa hoparlöre (sistem önceliği).
            val outs = audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).toList()
            outs.filter { it.toOutput().isHeadphone }.take(1).ifEmpty {
                outs.filter { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
            }
        }

    private fun routeOk(): Boolean {
        val r = mediaRouteDevices().map { it.toOutput() }
        return r.isNotEmpty() && r.all { it.isHeadphone }
    }

    /**
     * Kilidi uygula: uygulama sesi (0/1), gerekirse duraklat, sesi kulaklığa sabitle, hoparlör aktifse
     * hoparlörün medya sesini 0'a çek (mod kapanınca eski düzeye geri ver). Sıra: önce SUSTUR, sonra duraklat.
     */
    private fun enforceLock() {
        val x = exo ?: return
        val mode = MusicHub.settings.value.headphoneOnly
        val routeDevs = mediaRouteDevices()
        val saved = lockPrefs.getInt(K_SAVED_SPK, -1).takeIf { it >= 0 }
        val d = HeadphoneLock.decide(
            mode, routeDevs.map { it.toOutput() }, x.isPlaying,
            audio.getStreamVolume(AudioManager.STREAM_MUSIC), saved,
        )
        x.volume = d.appVolume
        if (d.pause) { Log.i(TAG, "kilit: ses kulaklığa gitmiyor → sessiz + duraklat"); x.pause() }
        // Sesi kulaklığa sabitle (birden fazla çıkış varken yanlış yere gitmesin)
        x.setPreferredAudioDevice(
            if (mode) audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS).firstOrNull { it.toOutput().isHeadphone } else null
        )
        d.setStreamVolume?.let {
            lockSelfVolumeChangeAt = SystemClock.elapsedRealtime()
            runCatching { audio.setStreamVolume(AudioManager.STREAM_MUSIC, it, 0) }
        }
        lockPrefs.edit().apply {
            if (d.savedSpeakerVolume != null) putInt(K_SAVED_SPK, d.savedSpeakerVolume) else remove(K_SAVED_SPK)
        }.apply()
    }

    /** Kilidin kendi yaptığı ses değişikliği ("ses 0 → duraklat" kuralı bunu kullanıcı kısması sanmasın). */
    private var lockSelfVolumeChangeAt = 0L

    private val deviceCallback = object : AudioDeviceCallback() {
        override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>) {
            enforceLock() // önce sustur (tampondaki ses de duyulmasın)
            main.postDelayed({ enforceLock() }, 800) // yönlendirme biraz gecikmeli değişebilir
            val p = session?.player ?: return
            val action = guard.onRemoved(
                removed.map { it.toOutput() }, currentOutputs(), p.isPlaying, SystemClock.elapsedRealtime(),
            )
            if (action == OutputGuard.Action.PAUSE) {
                Log.i(TAG, "kulaklık gitti → duraklat")
                p.pause()
            }
        }

        override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>) {
            enforceLock()
            main.postDelayed({ enforceLock() }, 800)
            val p = session?.player ?: return
            val action = guard.onAdded(
                added.map { it.toOutput() }, MusicHub.settings.value.btResume, p.isPlaying, SystemClock.elapsedRealtime(),
            )
            if (action == OutputGuard.Action.PLAY) {
                Log.i(TAG, "Bluetooth kulaklık geri geldi → devam")
                // Ses yolunun Bluetooth'a geçmesi için kısa bekleme (yoksa ilk an hoparlörden çıkabilir).
                main.postDelayed({ if (currentOutputs().any { it.isHeadphone }) p.play() }, 1500)
            }
        }
    }

    // ── Hata kurtarma ────────────────────────────────────

    private var retriedId: String? = null
    private var consecutiveFailures = 0

    private fun onError(player: Player, error: PlaybackException) {
        val id = player.currentMediaItem?.mediaId
        Log.e(TAG, "çalma hatası: $id", error)
        if (id != null && retriedId != id) {
            // Tek seferlik: aynı konumdan yeniden hazırla.
            retriedId = id
            player.prepare()
            player.play()
            return
        }
        consecutiveFailures++
        if (consecutiveFailures >= 3) {
            MusicHub.toast("art arda çalınamayan şarkılar: durduruldu")
            consecutiveFailures = 0
            return
        }
        MusicHub.toast("çalınamadı, atlandı")
        MusicHub.next(manual = false)
    }

    // ── Konum kaydı (sadece çalarken, 30 sn'de bir) ──────

    private val saver = object : Runnable {
        override fun run() {
            MusicHub.savePosition()
            main.postDelayed(this, 30_000)
        }
    }

    /** Medya sesi değişince (tuşlar, bildirim düğmeleri, sistem paneli) kuralı uygula. */
    private val volumeReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            enforceLock() // mod açıkken hoparlör sesi açılmaya çalışılırsa yine kapatılır
            if (SystemClock.elapsedRealtime() - lockSelfVolumeChangeAt > 1000) checkVolume()
        }
    }

    private fun checkVolume() {
        if (!MusicHub.settings.value.mutePause) return
        val player = session?.player ?: return
        val am = getSystemService(AudioManager::class.java)
        val action = mutePause.onVolume(
            volume = am.getStreamVolume(AudioManager.STREAM_MUSIC),
            muted = am.isStreamMute(AudioManager.STREAM_MUSIC),
            isPlaying = player.isPlaying,
        )
        when (action) {
            MutePause.Action.PAUSE -> { Log.i(TAG, "ses kısıldı → duraklat"); player.pause() }
            MutePause.Action.PLAY -> { Log.i(TAG, "ses geri geldi → devam"); player.play() }
            MutePause.Action.NONE -> Unit
        }
    }

    private val volDown = SessionCommand(CMD_VOLUME_DOWN, Bundle.EMPTY)
    private val volUp = SessionCommand(CMD_VOLUME_UP, Bundle.EMPTY)

    @UnstableApi
    private fun buttons(): ImmutableList<CommandButton> = ImmutableList.of(
        CommandButton.Builder(CommandButton.ICON_VOLUME_DOWN)
            .setDisplayName("Sesi azalt")
            .setSessionCommand(volDown)
            .build(),
        CommandButton.Builder(CommandButton.ICON_VOLUME_UP)
            .setDisplayName("Sesi artır")
            .setSessionCommand(volUp)
            .build(),
    )

    @UnstableApi
    override fun onCreate() {
        super.onCreate()
        // Ses hattına analizör musluğu takılır (sesi değiştirmez; analizör kapalıyken kopya da almaz).
        val renderers = object : DefaultRenderersFactory(this) {
            override fun buildAudioSink(
                context: Context, enableFloatOutput: Boolean, enableAudioTrackPlaybackParams: Boolean,
            ): AudioSink = DefaultAudioSink.Builder(context)
                .setAudioProcessors(arrayOf(SpectrumTap))
                .build()
        }
        val player = ExoPlayer.Builder(this, renderers)
            // Kablolu kulaklık çıkınca / Bluetooth kulaklık kopunca çalma duraklar.
            .setHandleAudioBecomingNoisy(true)
            // Telefon gelince / başka uygulama çalınca durur; geçici kayıptan sonra devam eder.
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .build(),
                /* handleAudioFocus = */ true,
            )
            // Ekran kapalıyken işlemci uyumasın (yerel dosya: ağ kilidi gerekmez).
            .setWakeMode(C.WAKE_MODE_LOCAL)
            .build()

        audio = getSystemService(AudioManager::class.java)
        player.addListener(object : Player.Listener {
            override fun onEvents(player: Player, events: Player.Events) {
                // Ana ekran widget'ı: sadece şarkı / çal-duraklat değişince (widget yoksa iş yapılmaz).
                if (events.containsAny(Player.EVENT_IS_PLAYING_CHANGED, Player.EVENT_MEDIA_METADATA_CHANGED,
                        Player.EVENT_MEDIA_ITEM_TRANSITION)
                ) {
                    val s = MusicHub.song(player.currentMediaItem?.mediaId)
                    PlayerWidget.push(this@PlaybackService, s?.title, s?.numberLabel, player.isPlaying)
                }
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                // Her çalma başlangıcında (arama sonrası kendiliğinden devam dahil) kilit yeniden doğrulanır.
                if (isPlaying) enforceLock()
                main.removeCallbacks(saver)
                if (isPlaying) {
                    // Sessizde elle oynatıldıysa / başka sebeple başladıysa "biz durdurduk" hatırası silinir.
                    mutePause.onStartedPlaying()
                    guard.onStartedPlaying()
                    consecutiveFailures = 0
                    main.postDelayed(saver, 30_000)
                } else {
                    MusicHub.savePosition()
                }
            }

            override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
                if (playWhenReady) return
                when (reason) {
                    Player.PLAY_WHEN_READY_CHANGE_REASON_AUDIO_BECOMING_NOISY ->
                        guard.onNoisyPause(SystemClock.elapsedRealtime())
                    Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM ->
                        MusicHub.onPausedAtEndOfTrack()
                }
            }

            override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
                retriedId = null
            }

            override fun onPlaybackStateChanged(state: Int) {
                // Şarkı bitti → sıradakini motor seçer (kuyruk / karıştır / döngü).
                if (state == Player.STATE_ENDED) MusicHub.onTrackEnded()
            }

            override fun onPlayerError(error: PlaybackException) = onError(player, error)
        })
        MusicHub.init(this)
        val enginePlayer = EnginePlayer(player)
        exo = player
        MusicHub.playGate = { !MusicHub.settings.value.headphoneOnly || routeOk() }
        MusicHub.lockHook = { enforceLock() }
        MusicHub.attachPlayer(enginePlayer)
        audio.registerAudioDeviceCallback(deviceCallback, main)
        enforceLock()
        ContextCompat.registerReceiver(
            this, volumeReceiver, IntentFilter(VOLUME_CHANGED_ACTION), ContextCompat.RECEIVER_NOT_EXPORTED,
        )

        val openApp = PendingIntent.getActivity(
            this, 0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        session = MediaSession.Builder(this, enginePlayer)
            .setSessionActivity(openApp)
            .setCallback(Callback())
            .setMediaButtonPreferences(buttons())
            .build()
        Log.i(TAG, "PlaybackService başladı")
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? = session

    override fun onTaskRemoved(rootIntent: Intent?) {
        MusicHub.savePosition()
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        MusicHub.savePosition()
        main.removeCallbacksAndMessages(null)
        MusicHub.playGate = null
        MusicHub.lockHook = null
        exo = null
        runCatching { audio.unregisterAudioDeviceCallback(deviceCallback) }
        runCatching { unregisterReceiver(volumeReceiver) }
        session?.run {
            MusicHub.detachPlayer(player)
            player.release()
            release()
        }
        session = null
        Log.i(TAG, "PlaybackService durdu")
        super.onDestroy()
    }

    @UnstableApi
    private inner class Callback : MediaSession.Callback {

        override fun onConnect(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
        ): MediaSession.ConnectionResult {
            val commands = MediaSession.ConnectionResult.DEFAULT_SESSION_COMMANDS.buildUpon()
                .add(volDown)
                .add(volUp)
                .build()
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session, controller)
                .setAvailableSessionCommands(commands)
                .setMediaButtonPreferences(buttons())
                .build()
        }

        /**
         * Tek tuşlu kulaklık (HEADSETHOOK / oynat-duraklat): ayar kapalıysa Media3'ün çift/üçlü basış
         * algılaması atlanır → tek basış BEKLEMEDEN oynat/duraklat. (Kulaklığın kendi çift basışı zaten
         * "sonraki" komutu gönderir; o etkilenmez.)
         */
        override fun onMediaButtonEvent(
            session: MediaSession, controllerInfo: MediaSession.ControllerInfo, intent: Intent,
        ): Boolean {
            if (MusicHub.settings.value.headsetMultiTap) return false
            @Suppress("DEPRECATION")
            val ev = intent.getParcelableExtra<KeyEvent>(Intent.EXTRA_KEY_EVENT) ?: return false
            if (ev.keyCode != KeyEvent.KEYCODE_HEADSETHOOK && ev.keyCode != KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE) return false
            if (ev.action == KeyEvent.ACTION_DOWN && ev.repeatCount == 0) MusicHub.togglePlay()
            return true
        }

        override fun onCustomCommand(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            customCommand: SessionCommand,
            args: Bundle,
        ): ListenableFuture<SessionResult> {
            val dir = when (customCommand.customAction) {
                CMD_VOLUME_DOWN -> AudioManager.ADJUST_LOWER
                CMD_VOLUME_UP -> AudioManager.ADJUST_RAISE
                else -> return Futures.immediateFuture(SessionResult(SessionResult.RESULT_ERROR_NOT_SUPPORTED))
            }
            val am = getSystemService(AudioManager::class.java)
            // Sistem medya sesi (donanım ses tuşlarıyla aynı); kısa ses çubuğu gösterilir.
            am.adjustStreamVolume(AudioManager.STREAM_MUSIC, dir, AudioManager.FLAG_SHOW_UI)
            checkVolume()
            return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
        }

        // Arayüzden gelen öğelerde adres (uri) süreçler arası aktarımda düşebilir: mediaId'den yeniden kur.
        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
        ): ListenableFuture<MutableList<MediaItem>> =
            Futures.immediateFuture(
                mediaItems.map { item ->
                    if (item.localConfiguration != null) item
                    else item.buildUpon().setUri(item.requestMetadata.mediaUri ?: android.net.Uri.parse(item.mediaId)).build()
                }.toMutableList()
            )
    }

    companion object {
        const val CMD_VOLUME_DOWN = "muzikit.VOLUME_DOWN"
        const val CMD_VOLUME_UP = "muzikit.VOLUME_UP"
        private const val K_SAVED_SPK = "saved_speaker_volume"
        /** Sistemin ses değişti yayını (gizli API sabiti, ama tüm sürümlerde gönderiliyor). */
        private const val VOLUME_CHANGED_ACTION = "android.media.VOLUME_CHANGED_ACTION"
    }
}

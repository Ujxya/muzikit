package com.muzikit.app.download

import android.content.Context
import android.content.Intent
import android.util.Log
import androidx.core.content.ContextCompat
import com.muzikit.app.MusicHub
import com.muzikit.app.TAG
import com.muzikit.app.library.FileNames
import com.yausername.ffmpeg.FFmpeg
import com.yausername.youtubedl_android.YoutubeDL
import com.yausername.youtubedl_android.YoutubeDLRequest
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.withContext
import java.io.File
import kotlin.random.Random

/** Kuyruktaki iş: tek video ya da onaylanmış playlist. */
sealed interface DownloadJob {
    data class Single(val videoId: String) : DownloadJob
    data class Playlist(val url: String) : DownloadJob
}

data class DownloadState(
    val busy: Boolean = false,
    val updating: Boolean = false,
    /** Kullanıcıya sorulacak playlist: (güvenli playlist URL'si, linkteki tekil video ID'si). */
    val pendingPlaylist: Pair<String, String?>? = null,
    val queued: Int = 0,
    /** "[3/12] 0807 - Başlık" gibi; boşsa iş yok. */
    val current: String = "",
    val progress: Float = -1f,
    val log: List<String> = emptyList(),
    val ytdlpVersion: String? = null,
)

/**
 * İndirme merkezi — masaüstü `downloader.rs` + `app.rs` indirme akışının karşılığı.
 * İşler [DownloadService] (ön plan, dataSync) içinde sırayla yürür; ekran kapansa da sürer.
 */
object DownloadCenter {

    private const val RETRY_MAX = 3
    private const val WAIT_MIN_MS = 3_000L
    private const val WAIT_MAX_MS = 7_000L
    private const val LOG_MAX = 300
    private const val PROCESS_ID = "muzikit-dl"

    private val _state = MutableStateFlow(DownloadState())
    val state: StateFlow<DownloadState> = _state.asStateFlow()

    private val jobs = ArrayDeque<DownloadJob>()
    private val links = ArrayDeque<String>()
    @Volatile private var cancelRequested = false
    @Volatile private var initialized = false

    // ── Arayüzden ────────────────────────────────────────

    /** Metin kutusundaki linkleri doğrular, kuyruğa alır, gerekirse servisi başlatır. */
    @Synchronized
    fun submit(context: Context, text: String) {
        val parts = UrlRules.splitLinks(text)
        if (parts.isEmpty()) return
        links.addAll(parts)
        pump(context)
    }

    @Synchronized
    private fun pump(context: Context) {
        if (_state.value.pendingPlaylist != null) return
        while (links.isNotEmpty()) {
            val raw = links.removeFirst()
            val n = UrlRules.normalizeUrl(raw)
            if (n == null) {
                log("güvenlik: reddedildi (${FileNames.stripControlChars(raw).take(80)}) — sadece youtube.com, youtu.be, music.youtube.com")
                continue
            }
            if (n.isPlaylist) {
                // Masaüstündeki gibi sor: playlist'in tamamı mı, sadece bu video mu?
                _state.value = _state.value.copy(pendingPlaylist = n.url to UrlRules.extractVideoId(raw))
                return
            }
            UrlRules.extractVideoId(n.url)?.let { log("link: video $it"); enqueue(context, DownloadJob.Single(it)) }
        }
    }

    @Synchronized
    fun answerPlaylist(context: Context, all: Boolean?) {
        val (url, single) = _state.value.pendingPlaylist ?: return
        _state.value = _state.value.copy(pendingPlaylist = null)
        when {
            all == null -> log("işlem iptal edildi.")
            all -> enqueue(context, DownloadJob.Playlist(url))
            single != null -> { log("playlist reddedildi, sadece tekli video indiriliyor: $single"); enqueue(context, DownloadJob.Single(single)) }
            else -> log("playlist reddedildi ve tekil video kimliği bulunamadı.")
        }
        pump(context)
    }

    @Synchronized
    private fun enqueue(context: Context, job: DownloadJob) {
        jobs.addLast(job)
        _state.value = _state.value.copy(queued = jobs.size)
        ContextCompat.startForegroundService(context, Intent(context, DownloadService::class.java))
    }

    fun cancel() {
        cancelRequested = true
        synchronized(this) { jobs.clear(); links.clear() }
        runCatching { YoutubeDL.getInstance().destroyProcessById(PROCESS_ID) }
        log("iptal istendi…")
    }

    fun requestUpdate(context: Context) {
        if (_state.value.busy || _state.value.updating) return
        _state.value = _state.value.copy(updating = true)
        ContextCompat.startForegroundService(
            context, Intent(context, DownloadService::class.java).setAction(DownloadService.ACTION_UPDATE),
        )
    }

    fun clearLog() { _state.value = _state.value.copy(log = emptyList()) }

    // ── Servis içinden (arka plan iş parçacığı) ───────────

    private fun ensureInit(context: Context) {
        if (initialized) return
        log("yt-dlp hazırlanıyor (ilk seferde birkaç saniye sürebilir)…")
        YoutubeDL.getInstance().init(context)
        FFmpeg.getInstance().init(context)
        initialized = true
        refreshVersion(context)
    }

    fun refreshVersion(context: Context) {
        val v = runCatching { YoutubeDL.getInstance().versionName(context) ?: YoutubeDL.getInstance().version(context) }.getOrNull()
        _state.value = _state.value.copy(ytdlpVersion = v)
    }

    /** yt-dlp'yi GitHub'dan günceller (masaüstü "u"). */
    suspend fun runUpdate(context: Context) = withContext(Dispatchers.IO) {
        try {
            ensureInit(context)
            log("yt-dlp güncelleniyor…")
            val st = YoutubeDL.getInstance().updateYoutubeDL(context, YoutubeDL.UpdateChannel.STABLE)
            log(if (st == YoutubeDL.UpdateStatus.ALREADY_UP_TO_DATE) "✓ yt-dlp zaten güncel." else "✓ yt-dlp güncellendi.")
            refreshVersion(context)
        } catch (e: Exception) {
            log("yt-dlp güncellenemedi: ${e.message?.lineSequence()?.firstOrNull()}")
        } finally {
            _state.value = _state.value.copy(updating = false)
        }
    }

    /** Kuyruktaki işleri bitene kadar sırayla yürütür. [onProgress]: bildirim için. */
    suspend fun runQueue(context: Context, onProgress: (String, Float) -> Unit) = withContext(Dispatchers.IO) {
        cancelRequested = false
        _state.value = _state.value.copy(busy = true)
        try {
            ensureInit(context)
            while (!cancelRequested) {
                val job = synchronized(this@DownloadCenter) { jobs.removeFirstOrNull() } ?: break
                _state.value = _state.value.copy(queued = synchronized(this@DownloadCenter) { jobs.size })
                runJob(context, job, onProgress)
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.e(TAG, "indirme hatası", e)
            log("hata: ${e.message?.lineSequence()?.firstOrNull()}")
        } finally {
            _state.value = _state.value.copy(busy = false, current = "", progress = -1f, queued = 0)
        }
    }

    private suspend fun runJob(context: Context, job: DownloadJob, onProgress: (String, Float) -> Unit) {
        val tree = MusicHub.library.value.folder ?: run { log("önce müzik klasörünü seç."); return }
        val folder = SafFolder(context, tree)
        if (!folder.canWrite()) {
            log("klasöre yazma izni yok: ayarlar → kütüphane → \"değiştir\" ile klasörü yeniden seç.")
            return
        }
        val ids: List<String> = when (job) {
            is DownloadJob.Single -> listOf(job.videoId)
            is DownloadJob.Playlist -> {
                log("playlist taranıyor…")
                playlistIds(job.url) ?: return
            }
        }
        val listing = folder.list()
        val downloaded = UrlRules.parseDownloaded(listing["downloaded.txt"]?.let(folder::readText))
        val fresh = UrlRules.newIds(ids, downloaded)
        if (fresh.isEmpty()) {
            log(when {
                ids.isEmpty() -> "ℹ indirilecek video bulunamadı."
                ids.size == 1 -> "ℹ bu şarkı zaten indirilmiş (downloaded.txt'de var): ${ids[0]}"
                else -> "ℹ şarkıların tümü zaten indirilmiş (${ids.size} video)."
            })
            return
        }
        log("📋 ${fresh.size} yeni şarkı sıraya alındı.")
        var next = UrlRules.nextNumber(listing.keys)
        log("sıradaki numara: ${next.toString().padStart(4, '0')}")
        var failed = 0
        for ((i, id) in fresh.withIndex()) {
            if (cancelRequested) break
            val label = "[${i + 1}/${fresh.size}]"
            val ok = downloadOne(context, folder, id, next, label, onProgress)
            if (ok) {
                next++
                withContext(Dispatchers.Main) { MusicHub.reload() }
                if (i + 1 < fresh.size && !cancelRequested) {
                    val w = Random.nextLong(WAIT_MIN_MS, WAIT_MAX_MS + 1)
                    log("⏳ kota koruması: %.1f sn bekleniyor…".format(w / 1000f))
                    delay(w)
                }
            } else failed++
        }
        log(if (failed == 0) "✨ tüm indirmeler tamamlandı." else "bitti; $failed şarkı atlandı.")
    }

    private fun playlistIds(url: String): List<String>? {
        val req = YoutubeDLRequest(emptyList())
        req.addOption("--ignore-config")
        req.addOption("--flat-playlist")
        req.addOption("-J")
        req.addOption("--no-warnings")
        req.addOption("--socket-timeout", "30")
        req.addCommands(listOf("--", url))
        return try {
            val r = YoutubeDL.getInstance().execute(req, PROCESS_ID)
            val entries = org.json.JSONObject(r.out).optJSONArray("entries")
            val ids = ArrayList<String>()
            val seen = HashSet<String>()
            for (k in 0 until (entries?.length() ?: 0)) {
                val id = entries!!.optJSONObject(k)?.optString("id") ?: continue
                if (UrlRules.isValidVideoId(id) && seen.add(id) && ids.size < UrlRules.MAX_PLAYLIST_ITEMS) ids += id
            }
            log("playlist: ${ids.size} video.")
            ids
        } catch (e: Exception) {
            log("playlist taranamadı: ${clean(e.message)}")
            null
        }
    }

    /** Tek video: indir → Opus (yeniden kodlamadan) → etiket/kapak yok → klasöre kopyala → downloaded.txt. */
    private suspend fun downloadOne(
        context: Context, folder: SafFolder, id: String, number: Int, label: String,
        onProgress: (String, Float) -> Unit,
    ): Boolean {
        val num = number.toString().padStart(4, '0')
        val work = File(context.cacheDir, "dl").apply { mkdirs(); listFiles()?.forEach { it.delete() } }
        val url = UrlRules.watchUrl(id) // kullanıcı girdisi değil: doğrulanmış ID'den sabit biçim
        var attempt = 0
        while (attempt < RETRY_MAX && !cancelRequested) {
            attempt++
            log("$label [$num] indiriliyor: $url")
            _state.value = _state.value.copy(current = "$label $num", progress = 0f)
            val req = YoutubeDLRequest(emptyList())
            req.addOption("--ignore-config")
            req.addOption("-f", "bestaudio/best")
            req.addOption("-x")
            req.addOption("--audio-format", "opus")
            // Kapak fotoğrafı gömülmez; açıklama/yorum ve tüm etiketler silinir.
            req.addOption("--postprocessor-args", "ExtractAudio:-map_metadata -1 -map_metadata:s -1")
            req.addOption("-o", File(work, "$num.%(ext)s").absolutePath)
            req.addOption("--no-playlist")
            req.addOption("--no-mtime")
            req.addOption("--no-simulate")
            req.addOption("--print", "title")
            req.addCommands(listOf("--", url))
            try {
                val r = YoutubeDL.getInstance().execute(req, PROCESS_ID) { p, _, _ ->
                    _state.value = _state.value.copy(progress = p / 100f)
                    onProgress("$label $num", p / 100f)
                }
                val rawTitle = r.out.lineSequence().firstOrNull { it.isNotBlank() }?.trim() ?: "Bilinmeyen"
                val title = UrlRules.sanitizeFilename(rawTitle, 200)
                val produced = File(work, "$num.opus")
                if (!produced.isFile) { log("⚠ beklenen .opus dosyası bulunamadı."); return false }
                // Üzerine yazmadan klasöre kopyala (masaüstü move_no_clobber)
                val names = folder.list().keys
                var finalName: String? = null
                repeat(16) {
                    if (finalName != null) return@repeat
                    val candidate = UrlRules.uniqueName({ it in names }, number, title, id)
                    finalName = folder.copyIn(produced, candidate)
                }
                produced.delete()
                val saved = finalName ?: run { log("⚠ benzersiz hedef dosya adı bulunamadı."); return false }
                folder.appendDownloaded(id)
                com.muzikit.app.sync.SyncVideoIds.note(context, saved, id) // eşitleme defteri için
                log("✓ başarılı: $saved")
                return true
            } catch (e: YoutubeDL.CanceledException) {
                log("iptal edildi.")
                return false
            } catch (e: Exception) {
                val msg = clean(e.message)
                if (UrlRules.isPermanentError(e.message ?: "")) {
                    log("kalıcı hata (yaş sınırı / gizli / kaldırılmış), atlanıyor: $id")
                    return false
                }
                log("geçici hata ($attempt/$RETRY_MAX): $msg")
                delay(2_000)
            } finally {
                work.listFiles()?.forEach { it.delete() }
            }
        }
        if (!cancelRequested) log("❌ atlandı: $id ($RETRY_MAX deneme başarısız)")
        return false
    }

    private fun clean(s: String?): String =
        FileNames.stripControlChars(s?.lineSequence()?.lastOrNull { it.isNotBlank() } ?: "bilinmeyen hata").take(160)

    fun log(line: String) {
        val l = FileNames.stripControlChars(line)
        Log.i(TAG, "indir: $l")
        val cur = _state.value.log
        _state.value = _state.value.copy(log = (cur + l).takeLast(LOG_MAX))
    }
}

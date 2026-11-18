package com.muzikit.app.download

import com.muzikit.app.library.FileNames

/**
 * İndiricinin güvenlik kuralları — masaüstü `src/utils.rs` / `src/downloader.rs` birebir.
 * Kullanıcının yazdığı metin yt-dlp'ye ASLA ham gitmez: önce doğrulanır, ID çıkarılır,
 * URL sabit `https://www.youtube.com/...` biçiminde yeniden kurulur.
 */
object UrlRules {
    /** Kabul edilen en uzun URL (bellek/DoS koruması). */
    const val MAX_URL_LEN = 2048
    const val MAX_PLAYLIST_ITEMS = 5000
    const val MAX_QUEUED_LINKS = 200

    private val ALLOWED_HOSTS = setOf(
        "youtube.com", "www.youtube.com", "m.youtube.com", "music.youtube.com", "youtu.be", "www.youtu.be",
    )

    /** Sadece ASCII 11 karakterli video kimliği (Unicode harf kabul edilmez). */
    private val VIDEO_ID = Regex("""(?:[?&]v=|youtu\.be/|/embed/|/shorts/|/live/)([A-Za-z0-9_-]{11})(?:[^A-Za-z0-9_-]|$)""")
    private val PLAYLIST_ID = Regex("""[?&]list=([A-Za-z0-9_-]{2,64})(?:[&#]|$)""")
    private val SANITIZE = Regex("""[<>:"/\\|?*]""")
    private val BITRATE = Regex("""^([0-9]{1,3})k$""")

    fun isValidVideoId(id: String): Boolean =
        id.length == 11 && id.all { it in 'A'..'Z' || it in 'a'..'z' || it in '0'..'9' || it == '_' || it == '-' }

    fun isAllowedYoutubeUrl(url: String): Boolean {
        if (url.isEmpty() || url.length > MAX_URL_LEN ||
            url.any { Character.isISOControl(it) || Character.isWhitespace(it) || Character.isSpaceChar(it) }
        ) return false
        val lower = url.lowercase(java.util.Locale.ROOT)
        val rest = when {
            lower.startsWith("https://") -> lower.removePrefix("https://")
            lower.startsWith("http://") -> lower.removePrefix("http://")
            else -> return false
        }
        val end = rest.indexOfAny(charArrayOf('/', '?', '#')).let { if (it < 0) rest.length else it }
        val authority = rest.substring(0, end)
        // Kullanıcı bilgisi (user@host) veya port ile alan adı yanıltmasını reddet
        if ('@' in authority || ':' in authority) return false
        return authority in ALLOWED_HOSTS
    }

    /** Şema yazılmamışsa (`youtube.com/...`) başına `https://` ekler. */
    private fun withScheme(url: String): String {
        val t = url.trim()
        return if ("://" in t) t else "https://$t"
    }

    fun extractVideoId(url: String): String? {
        val full = withScheme(url)
        if (!isAllowedYoutubeUrl(full)) return null
        return VIDEO_ID.find(full)?.groupValues?.get(1)?.takeIf(::isValidVideoId)
    }

    /** Güvenli, sabit biçimli URL ve playlist mi. Güvensiz / YouTube dışı → null. */
    data class Normalized(val url: String, val isPlaylist: Boolean)

    fun normalizeUrl(url: String): Normalized? {
        val full = withScheme(url)
        if (!isAllowedYoutubeUrl(full)) return null
        PLAYLIST_ID.find(full)?.groupValues?.get(1)?.let {
            return Normalized("https://www.youtube.com/playlist?list=$it", true)
        }
        return extractVideoId(full)?.let { Normalized("https://www.youtube.com/watch?v=$it", false) }
    }

    fun watchUrl(id: String): String {
        require(isValidVideoId(id)) { "geçersiz video kimliği" }
        return "https://www.youtube.com/watch?v=$id"
    }

    fun isValidBitrate(b: String): Boolean =
        BITRATE.find(b)?.groupValues?.get(1)?.toIntOrNull()?.let { it in 6..510 } == true

    /**
     * Dosya adını yasaklı karakterlerden, kontrol/yön karakterlerinden ve fazla boşluklardan temizler.
     * Sonuç asla boş, `.` veya `..` olmaz.
     */
    fun sanitizeFilename(name: String, maxLength: Int): String {
        var clean = SANITIZE.replace(FileNames.stripControlChars(name), "")
        clean = clean.split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")
        if (clean.codePointCount(0, clean.length) > maxLength) {
            val cut = clean.offsetByCodePoints(0, maxLength)
            val truncated = clean.substring(0, cut)
            val sp = truncated.lastIndexOf(' ')
            clean = if (sp >= 0) truncated.substring(0, sp) else truncated
        }
        val trimmed = clean.trim().trim('.').trim()
        return trimmed.ifEmpty { "Bilinmeyen" }
    }

    /** Metin kutusundaki (boşluk/satırla ayrılmış) linkler. Çok uzunlar atlanır, en fazla 200. */
    fun splitLinks(text: String): List<String> =
        text.split(Regex("\\s+")).filter { it.isNotEmpty() && it.length <= MAX_URL_LEN }.take(MAX_QUEUED_LINKS)

    /**
     * Var olan dosyanın üzerine yazmayan hedef ad (masaüstü `unique_destination`):
     * `NNNN - başlık.opus` → `NNNN - başlık [id].opus` → `NNNN - başlık [id] (2).opus` …
     */
    fun uniqueName(exists: (String) -> Boolean, number: Int, title: String, videoId: String): String {
        val n = number.toString().padStart(4, '0')
        val primary = "$n - $title.opus"
        if (!exists(primary)) return primary
        val withId = "$n - $title [$videoId].opus"
        if (!exists(withId)) return withId
        var k = 2
        while (true) {
            val c = "$n - $title [$videoId] ($k).opus"
            if (!exists(c)) return c
            k++
        }
    }

    /** Sıradaki numara: klasördeki en büyük `NNNN - ….opus` + 1 (masaüstü `get_next_number`). */
    fun nextNumber(fileNames: Iterable<String>): Int =
        (fileNames.filter { it.endsWith(".opus") }.mapNotNull(FileNames::parseTrackNumber).maxOrNull() ?: 0) + 1

    /** downloaded.txt'deki geçerli ID'ler. */
    fun parseDownloaded(text: String?): Set<String> =
        text?.lineSequence()?.map { it.trim() }?.filter(::isValidVideoId)?.toSet() ?: emptySet()

    /** Yeni indirilecekler: geçerli, daha önce indirilmemiş, tekrarsız, en fazla 5000. */
    fun newIds(ids: List<String>, downloaded: Set<String>): List<String> {
        val seen = HashSet<String>()
        return ids.filter { isValidVideoId(it) && it !in downloaded && seen.add(it) }.take(MAX_PLAYLIST_ITEMS)
    }

    /** yt-dlp hata metni kalıcı mı (yaş sınırı / gizli / kaldırılmış): yeniden denenmez. */
    fun isPermanentError(stderr: String): Boolean {
        val e = stderr.lowercase(java.util.Locale.ROOT)
        // Düz "age" araması "page", "message" gibi kelimeleri de yakalardı (masaüstündeki not).
        return listOf("sign in", "age-restricted", "age restricted", "confirm your age", "private video", "video unavailable")
            .any { it in e }
    }
}

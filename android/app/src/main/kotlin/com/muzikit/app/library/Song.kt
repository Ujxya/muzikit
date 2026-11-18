package com.muzikit.app.library

/**
 * Kütüphanedeki tek şarkı. Masaüstü `src/app.rs` `Song::new` mantığının birebir karşılığı.
 *
 * @param file disk üzerindeki dosya adı (örn. "0001 - Sanatci - Sarki.opus")
 * @param uri  dosyanın içerik adresi (SAF `content://...`); saf testlerde herhangi bir metin olabilir
 */
data class Song(
    val file: String,
    val uri: String,
    /** Ekranda gösterilen ad (numara ve uzantı olmadan); asla boş değil. */
    val title: String,
    val number: Int?,
    /** Arama için küçük harfli gövde (numara dahil, uzantısız). */
    val lower: String,
) {
    /** Listede gösterilen 4 haneli numara ("0023") ya da numarasızlar için nokta. */
    val numberLabel: String = number?.toString()?.padStart(4, '0') ?: "····"

    companion object {
        const val UNTITLED = "(isimsiz)"

        fun fromFile(file: String, uri: String): Song {
            val clean = FileNames.stripControlChars(file)
            val number = FileNames.parseTrackNumber(file)
            val stem = clean.removeSuffix(".opus")
            val raw = if (number != null) {
                // "0001 - Ad" → "Ad"
                val i = stem.indexOf(" - ")
                if (i >= 0) stem.substring(i + 3).trim() else ""
            } else {
                stem.trim()
            }
            return Song(
                file = file,
                uri = uri,
                title = raw.ifEmpty { UNTITLED },
                number = number,
                lower = stem.lowercase(),
            )
        }
    }
}

object FileNames {
    // İsim kısmı boş olabilir ("0373 - .opus"): yine de numaralı sayılır.
    private val FILENAME_PATTERN = Regex("""^(\d{4}) - .*\.opus$""", RegexOption.DOT_MATCHES_ALL)

    fun parseTrackNumber(filename: String): Int? =
        FILENAME_PATTERN.find(filename)?.groupValues?.get(1)?.toIntOrNull()

    private fun isBidiControl(c: Char): Boolean =
        c == '‎' || c == '‏' || c in '‪'..'‮' || c in '⁦'..'⁩'

    /** Satır sonu/sekme → boşluk; diğer kontrol ve yön (bidi) karakterleri silinir. */
    fun stripControlChars(s: String): String = buildString(s.length) {
        for (c in s) {
            val m = if (c == '\t' || c == '\n' || c == '\r') ' ' else c
            if (!Character.isISOControl(m) && !isBidiControl(m)) append(m)
        }
    }

    fun isOpus(name: String): Boolean = name.endsWith(".opus")
}

/** Dosya adlarından numara sıralı şarkı listesi kurar (masaüstü: `files.sort()`). */
fun buildLibrary(entries: List<Pair<String, String>>): List<Song> =
    entries
        .filter { FileNames.isOpus(it.first) }
        .sortedBy { it.first }
        .map { (name, uri) -> Song.fromFile(name, uri) }

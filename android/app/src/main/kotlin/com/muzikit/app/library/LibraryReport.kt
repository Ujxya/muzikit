package com.muzikit.app.library

/** Kütüphane sağlık raporu (masaüstü `inspect_library` + aynı numaralı dosyalar). */
data class LibraryReport(
    val total: Int,
    val maxNumber: Int,
    val missing: List<Int>,
    /** Aynı numarayı taşıyan birden fazla dosya: numara → dosya sayısı. */
    val duplicates: Map<Int, Int>,
    val unnumbered: Int,
    /** downloaded.txt'deki geçerli ID sayısı; dosya yoksa null. */
    val downloadedIds: Int?,
)

private val VIDEO_ID = Regex("^[A-Za-z0-9_-]{11}$")

fun countDownloadedIds(text: String): Int =
    text.lineSequence().map { it.trim() }.filter { VIDEO_ID.matches(it) }.toSet().size

fun buildReport(songs: List<Song>, downloadedTxt: String?): LibraryReport {
    val nums = songs.mapNotNull { it.number }
    val max = nums.maxOrNull() ?: 0
    val set = nums.toHashSet()
    return LibraryReport(
        total = songs.size,
        maxNumber = max,
        missing = (1..max).filter { it !in set },
        duplicates = nums.groupingBy { it }.eachCount().filterValues { it > 1 }.toSortedMap(),
        unnumbered = songs.count { it.number == null },
        downloadedIds = downloadedTxt?.let(::countDownloadedIds),
    )
}

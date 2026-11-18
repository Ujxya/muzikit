package com.muzikit.app.sync

import org.junit.Assert.assertEquals
import org.junit.Test

class IndexFormatTest {

    private val sha = "a".repeat(64)

    @Test
    fun roundTripKeepsTabsAndUnicodeInName() {
        val rows = listOf(
            IndexRow(23, sha, 100, 1_700_000_000, null, "2026-10-01T12:00:00Z", "pc", "0023 - さよならクロステック.opus"),
            IndexRow(1, "b".repeat(64), 5, 1, "dQw4w9WgXcQ", "2026-10-01T12:00:01Z", "phone", "0001 - Goth.opus"),
        )
        val text = IndexFormat.write(rows)
        assertEquals("# muzikit-index v1", text.lineSequence().first())
        assertEquals(rows.sortedBy { it.number }, IndexFormat.parse(text))
    }

    @Test
    fun badLinesSkipped() {
        val t = "# muzikit-index v1\nnumber\tsha256\tsize\tmtime\tvideoId\taddedAt\taddedBy\tname\n" +
            "x\ty\n0005\tkısa\t1\t1\t-\tt\tpc\t0005 - a.opus\n0006\t$sha\t1\t1\t-\t2026-01-01T00:00:00Z\tpc\t0006 - b.opus\n"
        assertEquals(listOf(6), IndexFormat.parse(t).map { it.number })
    }

    @Test
    fun utcFormatIsFixedWidthAndSortable() {
        assertEquals("1970-01-01T00:00:00Z", IndexFormat.utc(0))
        assertEquals("2026-11-16T12:00:00Z", IndexFormat.utc(1_794_830_400_000))
    }
}

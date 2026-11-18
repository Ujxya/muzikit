package com.muzikit.app.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LibraryReportTest {

    private fun lib(vararg names: String) = buildLibrary(names.map { it to it })

    @Test
    fun missingDuplicatesAndUnnumbered() {
        val r = buildReport(
            lib("0001 - a.opus", "0003 - c.opus", "0003 - c2.opus", "0006 - f.opus", "serbest.opus", "kapak.jpg"),
            null,
        )
        assertEquals(5, r.total)
        assertEquals(6, r.maxNumber)
        assertEquals(listOf(2, 4, 5), r.missing)
        assertEquals(mapOf(3 to 2), r.duplicates)
        assertEquals(1, r.unnumbered)
        assertNull(r.downloadedIds)
    }

    @Test
    fun emptyLibrary() {
        val r = buildReport(emptyList(), "")
        assertEquals(0, r.maxNumber)
        assertEquals(emptyList<Int>(), r.missing)
        assertEquals(0, r.downloadedIds)
    }

    @Test
    fun downloadedTxtCountsOnlyValidUniqueIds() {
        val txt = "dQw4w9WgXcQ\n  dQw4w9WgXcQ \nkısa\n--exec=abcd\nabcdefghijk\n\n"
        assertEquals(2, countDownloadedIds(txt))
    }
}

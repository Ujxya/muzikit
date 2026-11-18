package com.muzikit.app.library

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Masaüstü `song_titles_never_empty`, `test_parse_track_number`, `test_strip_control_chars` testlerinin karşılığı. */
class SongTest {

    private fun s(name: String) = Song.fromFile(name, "u")

    @Test
    fun songTitlesNeverEmpty() {
        s("0023 - さよならクロステック.opus").let {
            assertEquals(23, it.number); assertEquals("さよならクロステック", it.title)
        }
        s("0373 - .opus").let {
            assertEquals(373, it.number); assertEquals("(isimsiz)", it.title)
        }
        assertEquals("A - B", s("0001 - A - B.opus").title)
        s("serbest isim.opus").let {
            assertNull(it.number); assertEquals("serbest isim", it.title)
        }
        assertEquals("(isimsiz)", s(".opus").title)
    }

    @Test
    fun cyrillicTitleKept() {
        assertEquals("восемнадцать nightcore", s("0004 - восемнадцать nightcore.opus").title)
    }

    @Test
    fun parseTrackNumber() {
        assertEquals(759, FileNames.parseTrackNumber("0759 - test.opus"))
        assertNull(FileNames.parseTrackNumber("test.opus"))
        assertEquals(373, FileNames.parseTrackNumber("0373 - .opus"))
        assertNull(FileNames.parseTrackNumber("0373.opus"))
        assertNull(FileNames.parseTrackNumber("12345 - x.opus"))
        assertNull(FileNames.parseTrackNumber("0001 - x.mp3"))
    }

    @Test
    fun stripControlChars() {
        assertEquals("ok]0;pwn  x", FileNames.stripControlChars("ok\u001b]0;pwn\u0007\r\nx"))
        assertEquals("abcgpj.exe", FileNames.stripControlChars("abc‮gpj.exe"))
    }

    @Test
    fun lowerIsForSearch() {
        assertEquals("0005 - büyük harf", s("0005 - BÜYÜK Harf.opus").lower)
    }

    @Test
    fun buildLibrarySortsAndFilters() {
        val lib = buildLibrary(
            listOf(
                "0002 - b.opus" to "u2",
                "kapak.jpg" to "x",
                "0001 - a.opus" to "u1",
                "downloaded.txt" to "y",
                "zz.opus" to "u3",
            )
        )
        assertEquals(listOf("0001 - a.opus", "0002 - b.opus", "zz.opus"), lib.map { it.file })
        assertEquals("u1", lib[0].uri)
    }
}

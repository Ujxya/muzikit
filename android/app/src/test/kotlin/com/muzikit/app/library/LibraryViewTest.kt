package com.muzikit.app.library

import org.junit.Assert.assertEquals
import org.junit.Test

class LibraryViewTest {

    private val songs = buildLibrary(
        listOf(
            "0001 - Goth.opus",
            "0002 - akiaura LONOWN - pussypodium.opus",
            "0003 - akiaura Dj Pointless - If You Care.opus",
            "0023 - さよならクロステック.opus",
            "0004 - Bеtа Ünlü.opus",
        ).map { it to it }
    )

    private fun files(l: List<Song>) = l.map { it.number }

    @Test
    fun multiWordSearchNeedsAllTerms() {
        val r = filterSongs(songs, "  AKIAURA   care ", false, emptySet(), SortMode.NUMBER)
        assertEquals(listOf(3), files(r))
        assertEquals(listOf(2, 3), files(filterSongs(songs, "akiaura", false, emptySet(), SortMode.NUMBER)))
    }

    @Test
    fun searchMatchesNumberAndJapanese() {
        assertEquals(listOf(23), files(filterSongs(songs, "クロス", false, emptySet(), SortMode.NUMBER)))
        assertEquals(listOf(23), files(filterSongs(songs, "0023", false, emptySet(), SortMode.NUMBER)))
    }

    @Test
    fun emptyQueryKeepsAll() {
        assertEquals(5, filterSongs(songs, "   ", false, emptySet(), SortMode.NUMBER).size)
    }

    @Test
    fun sorting() {
        assertEquals(listOf(1, 2, 3, 4, 23), files(filterSongs(songs, "", false, emptySet(), SortMode.NUMBER)))
        assertEquals(listOf(23, 4, 3, 2, 1), files(filterSongs(songs, "", false, emptySet(), SortMode.NUMBER_DESC)))
        // isim a-z, küçük harfe göre: akiaura Dj… < akiaura LONOWN… < Bеtа… < Goth < さよなら…
        assertEquals(listOf(3, 2, 4, 1, 23), files(filterSongs(songs, "", false, emptySet(), SortMode.NAME)))
    }

    @Test
    fun onlyFavorites() {
        val fav = setOf("0023 - さよならクロステック.opus", "0001 - Goth.opus")
        assertEquals(listOf(1, 23), files(filterSongs(songs, "", true, fav, SortMode.NUMBER)))
        assertEquals(listOf(23), files(filterSongs(songs, "クロス", true, fav, SortMode.NUMBER)))
    }

    @Test
    fun sortCycle() {
        assertEquals(SortMode.NUMBER_DESC, SortMode.NUMBER.next())
        assertEquals(SortMode.NAME, SortMode.NUMBER_DESC.next())
        assertEquals(SortMode.NUMBER, SortMode.NAME.next())
    }
}

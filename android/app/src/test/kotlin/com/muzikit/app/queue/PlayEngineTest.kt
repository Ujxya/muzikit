package com.muzikit.app.queue

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.random.Random

class PlayEngineTest {

    private val ctx = listOf("a", "b", "c", "d")
    private val all: (String) -> Boolean = { true }

    private fun PlayEngine.step(manual: Boolean, c: List<String> = ctx): String? =
        next(manual, c, all)?.also { play(it) }

    @Test
    fun sequentialAndEndOfList() {
        val e = PlayEngine()
        e.play("c")
        assertEquals("d", e.step(manual = false))
        assertNull("döngü kapalı: liste bitince durur", e.next(false, ctx, all))
        assertEquals("elle sonraki başa döner", "a", e.next(true, ctx, all))
    }

    @Test
    fun loopAllWrapsAndLoopOneRepeats() {
        val e = PlayEngine()
        e.loop = LoopMode.ALL
        e.play("d")
        assertEquals("a", e.next(false, ctx, all))
        e.loop = LoopMode.ONE
        assertEquals("tek: kendiliğinden bitince aynı şarkı", "d", e.next(false, ctx, all))
        assertEquals("tek: elle sonraki yine ilerler", "a", e.next(true, ctx, all))
    }

    @Test
    fun loopCycle() {
        assertEquals(LoopMode.ALL, LoopMode.OFF.next())
        assertEquals(LoopMode.ONE, LoopMode.ALL.next())
        assertEquals(LoopMode.OFF, LoopMode.ONE.next())
    }

    @Test
    fun nothingPlayingStartsAtFirst() {
        assertEquals("a", PlayEngine().next(false, ctx, all))
    }

    @Test
    fun queueHasPriorityAndSkipsMissing() {
        val e = PlayEngine()
        e.play("a")
        e.enqueue("d", front = false)
        e.enqueue("silindi", front = false)
        e.enqueue("c", front = true) // sıradaki yap
        assertEquals(listOf("c", "d", "silindi"), e.queue)
        val exists: (String) -> Boolean = { it != "silindi" }
        assertEquals("c", e.next(true, ctx, exists)); e.play("c")
        assertEquals("d", e.next(true, ctx, exists)); e.play("d")
        // "silindi" atlanır, kuyruk biter, listeye dönülür (d'den sonra: liste sonu → elle başa)
        assertEquals("a", e.next(true, ctx, exists))
        assertTrue(e.queue.isEmpty())
    }

    @Test
    fun queueMoveAndRemove() {
        val e = PlayEngine()
        listOf("x", "y", "z").forEach { e.enqueue(it, false) }
        e.queueMove(2, up = true)
        assertEquals(listOf("x", "z", "y"), e.queue)
        e.queueMove(0, up = true) // sınırda: değişmez
        e.queueMove(2, up = false)
        assertEquals(listOf("x", "z", "y"), e.queue)
        e.queueRemove(0)
        assertEquals(listOf("z", "y"), e.queue)
    }

    @Test
    fun shuffleBagNoRepeatUntilExhausted() {
        val songs = (1..50).map { "s$it" }
        repeat(20) { seed ->
            val e = PlayEngine(Random(seed))
            e.setShuffle(true)
            val seen = HashSet<String>()
            repeat(songs.size) {
                val f = e.step(manual = false, c = songs)!!
                assertTrue("torba bitmeden tekrar: $f", seen.add(f))
            }
            assertEquals(songs.toSet(), seen)
        }
    }

    @Test
    fun newBagDoesNotStartWithJustPlayed() {
        val songs = listOf("a", "b", "c")
        repeat(200) { seed ->
            val e = PlayEngine(Random(seed))
            e.setShuffle(true)
            repeat(3) { e.step(false, songs) }
            val last = e.playing
            val firstOfNewBag = e.step(false, songs)
            assertNotEquals("seed=$seed", last, firstOfNewBag)
        }
    }

    @Test
    fun songPlayedByHandIsRemovedFromBag() {
        val songs = listOf("a", "b", "c", "d")
        repeat(30) { seed ->
            val e = PlayEngine(Random(seed))
            e.setShuffle(true)
            e.step(false, songs)           // torba oluşur
            e.play("d", true)              // elle d çalındı (torbadan düşer)
            val rest = (1..2).map { e.step(false, songs) }
            assertTrue("seed=$seed rest=$rest", "d" !in rest)
        }
    }

    @Test
    fun prevRules() {
        val e = PlayEngine()
        assertEquals(PrevAction.Play("d"), e.prev(0, ctx, all)) // hiçbir şey çalmıyor: listenin sonu
        e.play("a"); e.play("c")
        assertEquals("3 sn'den fazla: başa sar", PrevAction.SeekToStart, e.prev(3001, ctx, all))
        assertEquals("geçmişten önceki", PrevAction.Play("a"), e.prev(1000, ctx, all))
        e.play("a", pushHistory = false)
        // geçmiş boş: listede bir önceki (a'nın öncesi → sondaki)
        assertEquals(PrevAction.Play("d"), e.prev(0, ctx, all))
    }

    @Test
    fun recentIsUniqueNewestFirstAndCapped() {
        val e = PlayEngine()
        e.play("a"); e.play("b"); e.play("a")
        assertEquals(listOf("a", "b"), e.recent)
        repeat(40) { e.play("x$it") }
        assertEquals(PlayEngine.RECENT_MAX, e.recent.size)
        assertEquals("x39", e.recent.first())
    }

    @Test
    fun pruneRemovesDeleted() {
        val e = PlayEngine()
        e.play("a"); e.play("b")
        e.enqueue("gone", false); e.enqueue("c", false)
        e.prune { it != "gone" && it != "a" }
        assertEquals(listOf("c"), e.queue)
        assertEquals(listOf("b"), e.recent)
    }
}

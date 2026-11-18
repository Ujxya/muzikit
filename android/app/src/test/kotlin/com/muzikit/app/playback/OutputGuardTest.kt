package com.muzikit.app.playback

import com.muzikit.app.playback.OutputGuard.Action
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class OutputGuardTest {

    private val speaker = Output("2:hoparlör", OutputKind.OTHER)
    private val buds = Output("8:Buds", OutputKind.BLUETOOTH)
    private val otherBuds = Output("8:Başka", OutputKind.BLUETOOTH)
    private val wired = Output("3:kablolu", OutputKind.WIRED)
    private val btSpeaker = Output("27:BT hoparlör", OutputKind.OTHER)

    @Test
    fun headphoneOnlyRefusesWithoutHeadphone() {
        val g = OutputGuard()
        assertFalse("kulaklık yok → çalma reddedilir", g.canPlay(true, listOf(speaker)))
        assertFalse("BT hoparlör kulaklık sayılmaz", g.canPlay(true, listOf(speaker, btSpeaker)))
        assertTrue(g.canPlay(true, listOf(speaker, buds)))
        assertTrue(g.canPlay(true, listOf(speaker, wired)))
        assertTrue("mod kapalıyken her zaman çalar", g.canPlay(false, listOf(speaker)))
    }

    @Test
    fun btLostWhilePlayingPauses() {
        val g = OutputGuard()
        assertEquals(Action.PAUSE, g.onRemoved(listOf(buds), listOf(speaker), isPlaying = true, now = 0))
        assertEquals("8:Buds", g.pending?.first)
    }

    @Test
    fun otherHeadphoneStillConnectedDoesNotPause() {
        val g = OutputGuard()
        assertEquals(Action.NONE, g.onRemoved(listOf(buds), listOf(speaker, wired), isPlaying = true, now = 0))
        assertNull(g.pending)
    }

    @Test
    fun btBackWithin5MinResumesWhenEnabled() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), true, now = 0)
        assertEquals(Action.PLAY, g.onAdded(listOf(buds), resumeEnabled = true, isPlaying = false, now = 4 * 60_000))
        assertNull(g.pending)
    }

    @Test
    fun settingOffDoesNotResume() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), true, now = 0)
        assertEquals(Action.NONE, g.onAdded(listOf(buds), resumeEnabled = false, isPlaying = false, now = 1000))
    }

    @Test
    fun after5MinDoesNotResume() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), true, now = 0)
        assertEquals(Action.NONE, g.onAdded(listOf(buds), true, false, now = 5 * 60_000 + 1))
        assertNull(g.pending)
    }

    @Test
    fun differentDeviceDoesNotResume() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), true, now = 0)
        assertEquals(Action.NONE, g.onAdded(listOf(otherBuds), true, false, now = 1000))
        assertEquals("aynı aygıt hâlâ bekleniyor", Action.PLAY, g.onAdded(listOf(buds), true, false, now = 2000))
    }

    @Test
    fun wiredNeverAutoResumes() {
        val g = OutputGuard()
        assertEquals(Action.PAUSE, g.onRemoved(listOf(wired), listOf(speaker), true, now = 0))
        assertEquals(Action.NONE, g.onAdded(listOf(wired), true, false, now = 1000))
    }

    @Test
    fun noisyPauseBeforeRemovalIsCorrelated() {
        val g = OutputGuard()
        g.onNoisyPause(now = 1000) // ExoPlayer önce duraklattı
        assertEquals(Action.NONE, g.onRemoved(listOf(buds), listOf(speaker), isPlaying = false, now = 1500))
        assertEquals(Action.PLAY, g.onAdded(listOf(buds), true, false, now = 60_000))
    }

    @Test
    fun noisyPauseAfterRemovalIsCorrelated() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), isPlaying = false, now = 1000) // zaten duraklatılmış görünüyor
        assertNull(g.pending)
        g.onNoisyPause(now = 1200)
        assertEquals("8:Buds", g.pending?.first)
    }

    @Test
    fun userPausedLongBeforeIsNotResumed() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), isPlaying = false, now = 100_000)
        assertEquals(Action.NONE, g.onAdded(listOf(buds), true, false, now = 101_000))
    }

    @Test
    fun manualPlayCancelsPending() {
        val g = OutputGuard()
        g.onRemoved(listOf(buds), listOf(speaker), true, now = 0)
        g.onStartedPlaying()
        assertEquals(Action.NONE, g.onAdded(listOf(buds), true, false, now = 1000))
    }
}

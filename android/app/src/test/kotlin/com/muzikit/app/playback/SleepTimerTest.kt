package com.muzikit.app.playback

import org.junit.Assert.assertEquals
import org.junit.Test

class SleepTimerTest {

    private val min = 60_000L

    @Test
    fun fullCycle() {
        var t: SleepTimer = SleepTimer.Off
        val now = 1_000_000L
        val seen = mutableListOf<String>()
        repeat(7) {
            t = SleepTimer.cycle(t, now)
            seen += SleepTimer.shortLabel(t, now)
        }
        assertEquals(listOf("15dk", "30dk", "45dk", "60dk", "90dk", "son", "uyku"), seen)
    }

    @Test
    fun cycleUsesRemainingTime() {
        val now = 0L
        // 30 dk kurulmuş, 20 dk geçmiş: kalan ~10 → sonraki seçenek 15
        val t = SleepTimer.cycle(SleepTimer.At(30 * min), now + 20 * min)
        assertEquals(SleepTimer.At(20 * min + 15 * min), t)
    }

    @Test
    fun labelsRoundUp() {
        assertEquals("1dk", SleepTimer.shortLabel(SleepTimer.At(30_000), 0))
        assertEquals("0dk", SleepTimer.shortLabel(SleepTimer.At(0), 5))
        assertEquals("şarkı sonunda", SleepTimer.longLabel(SleepTimer.EndOfTrack, 0))
    }
}

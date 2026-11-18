package com.muzikit.app.playback

import com.muzikit.app.playback.MutePause.Action
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class MutePauseTest {

    @Test
    fun volumeZeroWhilePlayingPausesThenResumes() {
        val m = MutePause()
        assertEquals(Action.PAUSE, m.onVolume(0, muted = false, isPlaying = true))
        assertEquals(Action.NONE, m.onVolume(0, muted = false, isPlaying = false))
        assertEquals(Action.PLAY, m.onVolume(3, muted = false, isPlaying = false))
        assertFalse(m.pausedByMute)
    }

    @Test
    fun muteFlagCountsAsSilent() {
        val m = MutePause()
        assertEquals(Action.PAUSE, m.onVolume(8, muted = true, isPlaying = true))
        assertEquals(Action.PLAY, m.onVolume(8, muted = false, isPlaying = false))
    }

    @Test
    fun userPausedBeforeMuteIsNotResumed() {
        val m = MutePause()
        // Zaten duraklatılmışken ses kısıldı → biz durdurmadık → ses açılınca başlamaz.
        assertEquals(Action.NONE, m.onVolume(0, muted = false, isPlaying = false))
        assertEquals(Action.NONE, m.onVolume(5, muted = false, isPlaying = false))
    }

    @Test
    fun userStartedPlaybackClearsMemory() {
        val m = MutePause()
        m.onVolume(0, muted = false, isPlaying = true)
        m.onStartedPlaying() // kullanıcı sessizde elle oynattı
        assertEquals(Action.NONE, m.onVolume(4, muted = false, isPlaying = false))
    }

    @Test
    fun volumeChangesWhilePlayingDoNothing() {
        val m = MutePause()
        assertEquals(Action.NONE, m.onVolume(5, muted = false, isPlaying = true))
        assertEquals(Action.NONE, m.onVolume(9, muted = false, isPlaying = true))
    }
}

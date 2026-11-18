package com.muzikit.app.playback

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HeadphoneLockTest {

    private val speaker = Output("2:hoparlör", OutputKind.SPEAKER)
    private val buds = Output("8:Buds", OutputKind.BLUETOOTH)
    private val wired = Output("3:kablolu", OutputKind.WIRED)
    private val hdmi = Output("9:hdmi", OutputKind.OTHER)

    private fun d(route: List<Output>, playing: Boolean = true, vol: Int = 7, saved: Int? = null, mode: Boolean = true) =
        HeadphoneLock.decide(mode, route, playing, vol, saved)

    @Test
    fun headphoneRouteIsAudible() {
        val r = d(listOf(buds))
        assertEquals(1f, r.appVolume); assertFalse(r.pause); assertNull(r.setStreamVolume)
        assertEquals(1f, d(listOf(wired)).appVolume)
    }

    @Test
    fun speakerRouteSilencesPausesAndMutesSpeakerRememberingOldVolume() {
        val r = d(listOf(speaker), vol = 9)
        assertEquals(0f, r.appVolume); assertTrue(r.pause)
        assertEquals(0, r.setStreamVolume); assertEquals(9, r.savedSpeakerVolume)
    }

    @Test
    fun alreadyMutedSpeakerKeepsFirstSavedVolume() {
        val r = d(listOf(speaker), vol = 3, saved = 9) // kullanıcı hoparlörü 3'e açtı → yine 0, eski 9 korunur
        assertEquals(0, r.setStreamVolume); assertEquals(9, r.savedSpeakerVolume)
        val z = d(listOf(speaker), vol = 0, saved = 9)
        assertNull(z.setStreamVolume); assertEquals(9, z.savedSpeakerVolume)
    }

    @Test
    fun unknownOrMixedRouteStaysSilent() {
        assertEquals("yönlendirme bilinmiyor → sessiz", 0f, d(emptyList()).appVolume)
        assertEquals("kulaklık + hoparlör birlikte → sessiz", 0f, d(listOf(buds, speaker)).appVolume)
        assertEquals("HDMI vb. → sessiz", 0f, d(listOf(hdmi)).appVolume)
        assertNull("bilinmeyen yönlendirmede sistem sesine dokunma", d(emptyList()).setStreamVolume)
    }

    @Test
    fun headphoneVolumeIsNeverTouched() {
        assertNull(d(listOf(buds), vol = 12).setStreamVolume)
    }

    @Test
    fun notPlayingDoesNotRequestPause() {
        assertFalse(d(listOf(speaker), playing = false).pause)
    }

    @Test
    fun modeOffRestoresSpeakerWhenSpeakerActive() {
        val r = d(listOf(speaker), vol = 0, saved = 9, mode = false)
        assertEquals(1f, r.appVolume); assertFalse(r.pause)
        assertEquals(9, r.setStreamVolume); assertNull(r.savedSpeakerVolume)
    }

    @Test
    fun modeOffWithHeadphoneWaitsForSpeakerToRestore() {
        val r = d(listOf(buds), vol = 12, saved = 9, mode = false)
        assertNull("kulaklıktayken kulaklık sesine dokunma", r.setStreamVolume)
        assertEquals("geri verme sonraya kalır", 9, r.savedSpeakerVolume)
    }
}

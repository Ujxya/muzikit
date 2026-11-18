package com.muzikit.app.spectrum

import org.junit.Assert.assertEquals
import org.junit.Test

class SampleRingTest {

    /** Örnek değeri = şarkıdaki kare numarası; böylece hangi pencerenin döndüğü görülür. */
    private fun ringAt(baseUs: Long, frames: Int, rate: Int = 1000, cap: Int = 5000): SampleRing {
        val r = SampleRing(cap)
        r.reset(baseUs, rate)
        val baseFrame = baseUs * rate / 1_000_000
        for (i in 0 until frames) r.write((baseFrame + i).toFloat())
        return r
    }

    @Test
    fun returnsWindowEndingAtPlaybackPositionNotNewestData() {
        // 1000 Hz, 3 sn çözülmüş ama oynatıcı henüz 1. saniyede: analizör 1. saniyeyi göstermeli.
        val r = ringAt(baseUs = 0, frames = 3000)
        val out = FloatArray(4)
        assertEquals(4, r.read(1_000_000, out, 4))
        assertEquals(listOf(996f, 997f, 998f, 999f), out.toList())
    }

    @Test
    fun usesFlushPositionAfterSeek() {
        // 60. saniyeye sarıldı: örnekler 60000. kareden başlar.
        val r = ringAt(baseUs = 60_000_000, frames = 2000)
        val out = FloatArray(2)
        assertEquals(2, r.read(61_000_000, out, 2))
        assertEquals(listOf(60998f, 60999f), out.toList())
    }

    @Test
    fun positionAheadOfDecodedClampsToNewest() {
        val r = ringAt(0, 500)
        val out = FloatArray(3)
        assertEquals(3, r.read(9_000_000, out, 3))
        assertEquals(listOf(497f, 498f, 499f), out.toList())
    }

    @Test
    fun notEnoughOrTooOldDataReturnsZero() {
        val r = ringAt(0, 100)
        assertEquals("pencere için az veri", 0, r.read(50_000, FloatArray(80), 80))
        val big = ringAt(0, 20_000, cap = 5000)
        assertEquals("halka taşmış: çok eski", 0, big.read(2_000_000, FloatArray(10), 10))
        assertEquals(0, SampleRing().read(0, FloatArray(1), 1))
    }
}

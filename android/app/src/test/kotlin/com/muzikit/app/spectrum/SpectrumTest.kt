package com.muzikit.app.spectrum

import com.muzikit.app.spectrum.SpectrumMath.BANDS
import com.muzikit.app.spectrum.SpectrumMath.FFT_N
import com.muzikit.app.spectrum.SpectrumMath.F_HI
import com.muzikit.app.spectrum.SpectrumMath.F_LO
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.ln
import kotlin.math.sin

/** Masaüstü `spectrum.rs` testlerinin karşılığı. */
class SpectrumTest {

    private fun tone(freq: Float, rate: Int) =
        FloatArray(FFT_N) { i -> (sin(2 * PI * freq * i / rate) * 0.5).toFloat() }

    private fun bandOf(freq: Float) = (ln(freq / F_LO) / ln(F_HI / F_LO) * BANDS).toInt()

    @Test
    fun tonesLandInTheirBand() {
        val re = FloatArray(FFT_N); val im = FloatArray(FFT_N)
        for (f in listOf(60f, 440f, 1000f, 5000f, 12000f)) {
            val db = SpectrumMath.bandDb(tone(f, 48_000), FFT_N, 48_000, re, im)
            val got = db.indices.maxBy { db[it] }
            val want = bandOf(f)
            assertTrue("$f Hz: bant $got, beklenen $want", abs(got - want) <= 1)
        }
    }

    @Test
    fun silenceDropsBarsAndSoundRaisesThem() {
        val s = Spectrum()
        val t = tone(1000f, 48_000)
        repeat(10) { s.update(t, FFT_N, 48_000, 0.04f) }
        val b = bandOf(1000f)
        assertTrue("ton bandı yükselmeli: ${s.levels[b]}", s.levels[b] > 0.8f)
        assertTrue("uzak bant düşük kalmalı: ${s.levels[0]}", s.levels[0] < 0.3f)
        repeat(40) { s.update(null, 0, 0, 0.04f) }
        assertTrue(s.isSilent)
    }

    @Test
    fun tooFewSamplesGiveNothing() {
        val re = FloatArray(FFT_N); val im = FloatArray(FFT_N)
        val db = SpectrumMath.bandDb(FloatArray(100), 100, 48_000, re, im)
        assertTrue(db.all { it == -120f })
    }
}

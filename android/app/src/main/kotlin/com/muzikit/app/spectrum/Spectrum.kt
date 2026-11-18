package com.muzikit.app.spectrum

import kotlin.math.PI
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.pow
import kotlin.math.sin

/**
 * Gerçek frekans analizi — masaüstü `src/spectrum.rs`'in birebir karşılığı.
 *
 * O an duyulan sesten Hann pencereli FFT (2048) alınır; enerji 24 logaritmik banda (30 Hz–16 kHz)
 * toplanır ve dB'ye çevrilir. Çubuklar hızlı yükselir, yavaş düşer; ölçek son zamanlardaki en yüksek
 * seviyeye göre kendini ayarlar (otomatik kazanç).
 */
object SpectrumMath {
    const val BANDS = 24
    const val FFT_N = 2048
    const val F_LO = 30f
    const val F_HI = 16_000f
    /** Görünen dinamik aralık (dB): en yüksek bandın bu kadar altı sıfır çubuk. */
    const val RANGE_DB = 48f

    private val hann = FloatArray(FFT_N) { i -> (0.5 - 0.5 * cos(2 * PI * i / (FFT_N - 1))).toFloat() }

    /** Yerinde radix-2 FFT (uzunluk 2'nin kuvveti). */
    fun fft(re: FloatArray, im: FloatArray) {
        val n = re.size
        var j = 0
        for (i in 1 until n) {
            var bit = n shr 1
            while (j and bit != 0) { j = j xor bit; bit = bit shr 1 }
            j = j or bit
            if (i < j) {
                var t = re[i]; re[i] = re[j]; re[j] = t
                t = im[i]; im[i] = im[j]; im[j] = t
            }
        }
        var len = 2
        while (len <= n) {
            val ang = -2.0 * PI / len
            val wr = cos(ang).toFloat(); val wi = sin(ang).toFloat()
            var start = 0
            while (start < n) {
                var cr = 1f; var ci = 0f
                for (k in 0 until len / 2) {
                    val a = start + k
                    val b = a + len / 2
                    val vr = re[b] * cr - im[b] * ci
                    val vi = re[b] * ci + im[b] * cr
                    re[b] = re[a] - vr; im[b] = im[a] - vi
                    re[a] += vr; im[a] += vi
                    val ncr = cr * wr - ci * wi
                    ci = cr * wi + ci * wr
                    cr = ncr
                }
                start += len
            }
            len = len shl 1
        }
    }

    /** [samples]'ın son FFT_N örneğinden bant başına dB. Az örnek varsa hepsi -120. */
    fun bandDb(samples: FloatArray, count: Int, rate: Int, re: FloatArray, im: FloatArray): FloatArray {
        val out = FloatArray(BANDS) { -120f }
        if (count < FFT_N || rate <= 0) return out
        val off = count - FFT_N
        for (i in 0 until FFT_N) { re[i] = samples[off + i] * hann[i]; im[i] = 0f }
        fft(re, im)
        val binHz = rate.toFloat() / FFT_N
        val nyq = FFT_N / 2
        val ratio = F_HI / F_LO
        for (b in 0 until BANDS) {
            val lo = F_LO * ratio.pow(b.toFloat() / BANDS)
            val hi = F_LO * ratio.pow((b + 1).toFloat() / BANDS)
            val k0 = floor(lo / binHz).toInt().coerceIn(1, nyq - 1)
            val k1 = ceil(hi / binHz).toInt().coerceIn(k0 + 1, nyq)
            var e = 0f
            for (k in k0 until k1) e += re[k] * re[k] + im[k] * im[k]
            out[b] = 10f * log10(e / (k1 - k0) + 1e-12f)
        }
        return out
    }
}

/** Ekrandaki çubukların durumu (0..1). */
class Spectrum {
    val levels = FloatArray(SpectrumMath.BANDS)
    private var peakDb = -30f
    private val re = FloatArray(SpectrumMath.FFT_N)
    private val im = FloatArray(SpectrumMath.FFT_N)

    /** [samples]: o an duyulan ses (null → sessizlik, çubuklar iner). [dt]: saniye. */
    fun update(samples: FloatArray?, count: Int, rate: Int, dtIn: Float) {
        val dt = dtIn.coerceIn(0f, 0.25f)
        val target = FloatArray(SpectrumMath.BANDS)
        if (samples != null) {
            val db = SpectrumMath.bandDb(samples, count, rate, re, im)
            val framePeak = db.max()
            // Otomatik kazanç: tepe hemen yükselir, yavaşça (6 dB/sn) iner
            peakDb = max(framePeak, peakDb - 6f * dt).coerceAtLeast(-60f)
            for (i in db.indices) {
                target[i] = ((db[i] - (peakDb - SpectrumMath.RANGE_DB)) / SpectrumMath.RANGE_DB).coerceIn(0f, 1f)
            }
        }
        for (i in levels.indices) {
            val t = target[i]
            levels[i] = if (t > levels[i]) {
                levels[i] + (t - levels[i]) * (1f - exp(-dt * 30f)) // hızlı yükseliş
            } else {
                max(levels[i] - 1.6f * dt, t) // yavaş düşüş (~1.6 çubuk boyu/sn)
            }
        }
    }

    val isSilent: Boolean get() = levels.all { it == 0f }
}

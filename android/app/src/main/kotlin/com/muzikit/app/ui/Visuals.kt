package com.muzikit.app.ui

import android.graphics.Paint
import android.os.SystemClock
import androidx.annotation.OptIn
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.Stable
import kotlinx.coroutines.delay
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.runtime.derivedStateOf
import androidx.compose.foundation.layout.size
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.core.content.res.ResourcesCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import androidx.media3.common.util.UnstableApi
import com.muzikit.app.MusicHub
import com.muzikit.app.R
import com.muzikit.app.spectrum.Spectrum
import com.muzikit.app.spectrum.SpectrumMath
import com.muzikit.app.spectrum.SpectrumTap
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.pow
import kotlin.math.sin

/*
 * Masaüstü `gui.rs` arka planının telefona uyarlanmış, SAKİN hâli:
 *  - titreme, yırtılma, glitch, yanıp sönme YOK;
 *  - hareket yavaş (akan veri, dönen radar) ve en fazla ~30 kare/sn;
 *  - her şey sadece ekran görünürken çizilir; uygulama arka plandayken hiçbir döngü çalışmaz.
 */

private const val FRAME_MS = 33L // ~30 kare/sn yeter; pil için 60/120 Hz'de çizilmez

private fun gray(v: Int, a: Float = 1f) = Color(v, v, v).copy(alpha = a)

/** Masaüstü `star_hash`'e benzer, kararlı sözde rastgele. */
private fun hash(a: Int, b: Int): Int {
    var h = a * 374761393 + b * 668265263
    h = (h xor (h ushr 13)) * 1274126177
    return h xor (h ushr 16)
}

private fun hrand(a: Int, b: Int): Float = ((hash(a, b) ushr 8) and 0xFFFF) / 65535f

/**
 * Ekranın TEK görsel saati. Arka plan, radar ve analizör aynı anda (aynı ana iş parçacığı adımında)
 * güncellenir → saniyede en fazla ~30 çizim. Sadece ekran STARTED iken ve gerçekten hareket eden bir
 * şey varken döner; duraklatılmış + hareket kapalıyken hiç çizim istenmez.
 */
@Stable
class VisualTicker {
    /** Animasyon zamanı (saniye). */
    val time = mutableFloatStateOf(0f)
    /** Analizör karesi sayacı (çizimi tetikler). */
    val frame = mutableIntStateOf(0)
    val spectrum = Spectrum()
    internal val window = FloatArray(SpectrumMath.FFT_N)
}

@OptIn(UnstableApi::class)
@Composable
fun rememberVisualTicker(motion: Boolean, analyzer: Boolean, playing: Boolean): VisualTicker {
    val v = remember { VisualTicker() }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(Unit) { onDispose { SpectrumTap.enabled = false } }
    LaunchedEffect(motion, analyzer, playing, lifecycle) {
        if (!motion && !analyzer) { SpectrumTap.enabled = false; return@LaunchedEffect }
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            SpectrumTap.enabled = analyzer
            try {
                var lastNs = SystemClock.elapsedRealtimeNanos()
                while (motion || (analyzer && (playing || !v.spectrum.isSilent))) {
                    delay(FRAME_MS)
                    val now = SystemClock.elapsedRealtimeNanos()
                    val dt = (now - lastNs) / 1e9f
                    lastNs = now
                    if (motion) v.time.floatValue += dt
                    if (analyzer) {
                        val n = if (MusicHub.isPlaying()) {
                            SpectrumTap.ring.read(MusicHub.playbackPositionUs(), v.window, SpectrumMath.FFT_N)
                        } else 0
                        v.spectrum.update(if (n > 0) v.window else null, n, SpectrumTap.ring.rate, dt)
                        v.frame.intValue++
                    }
                }
            } finally {
                SpectrumTap.enabled = false
            }
        }
    }
    return v
}

/** Uygulamanın gömülü yazı tipiyle yerel Paint (Canvas metni için). */
@Composable
private fun rememberMonoPaint(sizeSp: Float, color: Int): Paint {
    val ctx = LocalContext.current
    val px = with(LocalDensity.current) { sizeSp * fontScale * density }
    return remember(px, color) {
        Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = ResourcesCompat.getFont(ctx, R.font.dejavu_sans_mono)
            textSize = px
            this.color = color
        }
    }
}

/**
 * Tüm ekranın arkası: ince ızgara + kesişimlerde seyrek artılar (sabit) ve [motion] açıksa yukarı
 * doğru yavaşça akan soluk hex sütunları.
 */
@Composable
fun CyberBackground(motion: Boolean, ticker: VisualTicker, modifier: Modifier = Modifier) {
    val clock = ticker.time
    val paint = rememberMonoPaint(10f, android.graphics.Color.WHITE)
    val density = LocalDensity.current.density
    // Sabit ızgara: kendi katmanında, hiçbir duruma bağlı değil → sadece boyut değişince çizilir.
    androidx.compose.foundation.layout.Spacer(
        modifier
            .fillMaxSize()
            .graphicsLayer()
            .drawWithCache {
                // Sabit kısım bir kez hesaplanır.
                val step = 44f * density * 0.75f
                val nx = (size.width / step).toInt() + 1
                val ny = (size.height / step).toInt() + 1
                val x0 = (size.width % step) / 2
                val y0 = (size.height % step) / 2
                val grid = Color(17, 18, 21)
                val cross = Color(62, 64, 70)
                val c = 3f * density
                onDrawBehind {
                    drawRect(Color.Black)
                    for (i in 0 until nx) {
                        val x = x0 + i * step
                        drawLine(grid, Offset(x, 0f), Offset(x, size.height), 1f)
                    }
                    for (j in 0 until ny) {
                        val y = y0 + j * step
                        drawLine(grid, Offset(0f, y), Offset(size.width, y), 1f)
                    }
                    for (i in 0 until nx) for (j in 0 until ny) {
                        if (Math.floorMod(hash(i, j), 9) != 0) continue
                        val p = Offset(x0 + i * step, y0 + j * step)
                        drawLine(cross, p - Offset(c, 0f), p + Offset(c, 0f), 1f)
                        drawLine(cross, p - Offset(0f, c), p + Offset(0f, c), 1f)
                    }
                }
            }
    )
    if (!motion) return
    // Akan hex sütunları — PİL İÇİN: her sütun kendi katmanında, yazılar sadece değişince (saniyede ~1)
    // çizilir; akış sadece katmanın Y kaydırmasıdır (yeniden çizim yok, RenderThread sadece birleştirir).
    val slowTick = remember { derivedStateOf { (clock.value * 0.8f).toInt() } }
    androidx.compose.foundation.layout.BoxWithConstraints(modifier.fillMaxSize()) {
        val hPx = constraints.maxHeight.toFloat()
        val wPx = constraints.maxWidth.toFloat()
        val lh = 13f * density
        val colW = 120f * density
        val cols = (wPx / colW).toInt().coerceAtLeast(1)
        for (col in 0 until cols) {
            val h = hash(col, 21)
            if (Math.floorMod(h, 3) == 0) continue
            val x = 14f * density + col * colW + Math.floorMod(h, 40) * density
            val speed = (8f + Math.floorMod(h ushr 8, 18)) * density // yavaş
            val len = 6 + Math.floorMod(h ushr 4, 12)
            val span = hPx + len * lh
            val phase = Math.floorMod(h ushr 12, 1000) * density
            val chars = remember(col) { CharArray(9) }
            Canvas(
                Modifier
                    .size(with(LocalDensity.current) { (70f * density).toDp() }, with(LocalDensity.current) { (len * lh).toDp() })
                    .graphicsLayer {
                        translationX = x
                        translationY = hPx - ((clock.value * speed + phase) % span) // sadece konum
                    }
            ) {
                val tickBase = slowTick.value // ~0,8 kez/sn değişir → içerik o kadar seyrek çizilir
                val nc = drawContext.canvas.nativeCanvas
                for (r in 0 until len) {
                    hexInto(chars, hash(col * 131 + r, tickBase / (2 + r % 3)))
                    val fade = 1f - r.toFloat() / len
                    // Çok soluk: şarkı adlarının okunurluğunu bozmasın
                    val g = if (r == 0) 34 else (12 + 14 * fade).toInt()
                    paint.color = android.graphics.Color.rgb(g, g, g + 2)
                    nc.drawText(chars, 0, 9, 0f, (r + 1) * lh, paint)
                }
            }
        }
    }
}

/** Dönen tarama çizgisi ve geçtikçe parlayan noktalarıyla küçük radar (masaüstü `draw_radar`). */
@Composable
fun Radar(ticker: VisualTicker, modifier: Modifier) {
    val clock = ticker.time
    Canvas(modifier.graphicsLayer()) {
        val t = clock.value
        val c = center
        val r = size.minDimension / 2 - 1f
        val ring = gray(48)
        for (k in 1..3) drawCircle(ring, r * k / 3f, c, style = Stroke(1f))
        drawLine(ring, c - Offset(r, 0f), c + Offset(r, 0f), 1f)
        drawLine(ring, c - Offset(0f, r), c + Offset(0f, r), 1f)
        val tau = (2 * PI).toFloat()
        val sweep = (t * 1.1f) % tau // masaüstünden biraz daha yavaş
        for (k in 0 until 14) {
            val a = sweep - k * 0.05f
            drawLine(gray((170 * (1f - k / 14f)).toInt()), c, c + Offset(cos(a), sin(a)) * r, 1f)
        }
        val cyc = (t / 8f).toInt()
        for (i in 0 until 5) {
            val ang = hrand(cyc * 17 + i, 70) * tau
            val dist = (0.2f + 0.75f * hrand(cyc * 17 + i, 71)) * r
            val since = ((sweep - ang) % tau + tau) % tau
            val glow = (1f - since / tau).pow(3)
            drawCircle(gray((40 + 215 * glow).toInt()), 1.2f + glow * 1.5f, c + Offset(cos(ang), sin(ang)) * dist)
        }
    }
}

/**
 * Gerçek frekans analizörü: o an DUYULAN sesin 24 bandı, LED tarzı bölmeli çubuklar.
 * Hesap [VisualTicker]'da (sadece ekran görünürken ve çalarken); burada sadece çizilir.
 */
@Composable
fun Analyzer(ticker: VisualTicker, modifier: Modifier) {
    val spectrum = ticker.spectrum
    val frame = ticker.frame
    Canvas(modifier.graphicsLayer()) {
        frame.intValue // her yeni karede yeniden çiz
        val lv = spectrum.levels
        val n = lv.size
        val gap = 2f * density
        val bw = (size.width - gap * (n - 1)) / n
        val seg = 4f * density
        val maxSegs = (size.height / seg).toInt().coerceAtLeast(1)
        for (i in 0 until n) {
            // Sessizlikte de bir bölme görünsün (analizör kaybolmasın)
            val segs = (maxSegs * lv[i]).toInt().coerceIn(1, maxSegs)
            val x = i * (bw + gap)
            for (k in 0 until segs) {
                val y = size.height - (k + 1) * seg
                val top = k + 1 == segs && segs > 1
                val col = if (top) gray(210) else gray((60 + 90f * k / maxSegs).toInt())
                drawRect(col, Offset(x, y), Size(bw, seg - density))
            }
        }
    }
}

private const val HEX = "0123456789ABCDEF"

/** "XXXX XXXX" — String.format'sız (her karede çağrılır). */
private fun hexInto(out: CharArray, v: Int) {
    var k = 0
    for (shift in intArrayOf(28, 24, 20, 16, -1, 12, 8, 4, 0)) {
        out[k++] = if (shift < 0) ' ' else HEX[(v ushr shift) and 0xF]
    }
}

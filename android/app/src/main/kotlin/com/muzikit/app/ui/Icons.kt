package com.muzikit.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class Glyph { Play, Pause, Prev, Next, Shuffle, Loop, LoopOne, Star, StarOutline, Up, Down, Close }

/** Durağan, ince çizgili simgeler (ek simge kütüphanesi gerekmesin; emoji olarak çizilmesin). */
@Composable
fun GlyphIcon(glyph: Glyph, color: Color, size: Dp = 22.dp) {
    Canvas(Modifier.size(size)) {
        val w = this.size.width
        val h = this.size.height
        val sw = w * 0.085f
        fun tri(x0: Float, x1: Float) = Path().apply {
            moveTo(x0, h * 0.15f); lineTo(x1, h * 0.5f); lineTo(x0, h * 0.85f); close()
        }
        fun line(x0: Float, y0: Float, x1: Float, y1: Float) =
            drawLine(color, Offset(w * x0, h * y0), Offset(w * x1, h * y1), sw, StrokeCap.Round)
        when (glyph) {
            Glyph.Play -> drawPath(tri(w * 0.22f, w * 0.86f), color)
            Glyph.Pause -> {
                drawRect(color, Offset(w * 0.22f, h * 0.15f), Size(w * 0.2f, h * 0.7f))
                drawRect(color, Offset(w * 0.58f, h * 0.15f), Size(w * 0.2f, h * 0.7f))
            }
            Glyph.Next -> {
                drawPath(tri(w * 0.15f, w * 0.72f), color)
                drawRect(color, Offset(w * 0.74f, h * 0.15f), Size(w * 0.11f, h * 0.7f))
            }
            Glyph.Prev -> {
                drawRect(color, Offset(w * 0.15f, h * 0.15f), Size(w * 0.11f, h * 0.7f))
                drawPath(Path().apply {
                    moveTo(w * 0.85f, h * 0.15f); lineTo(w * 0.28f, h * 0.5f); lineTo(w * 0.85f, h * 0.85f); close()
                }, color)
            }
            Glyph.Shuffle -> {
                line(0.1f, 0.28f, 0.32f, 0.28f); line(0.32f, 0.28f, 0.68f, 0.72f); line(0.68f, 0.72f, 0.88f, 0.72f)
                line(0.1f, 0.72f, 0.32f, 0.72f); line(0.32f, 0.72f, 0.68f, 0.28f); line(0.68f, 0.28f, 0.88f, 0.28f)
                line(0.78f, 0.18f, 0.88f, 0.28f); line(0.78f, 0.38f, 0.88f, 0.28f)
                line(0.78f, 0.62f, 0.88f, 0.72f); line(0.78f, 0.82f, 0.88f, 0.72f)
            }
            Glyph.Loop, Glyph.LoopOne -> {
                drawRoundRect(
                    color, Offset(w * 0.12f, h * 0.3f), Size(w * 0.76f, h * 0.42f),
                    androidx.compose.ui.geometry.CornerRadius(w * 0.12f), style = Stroke(sw),
                )
                // ok ucu üst kenarda
                line(0.5f, 0.3f, 0.6f, 0.2f); line(0.5f, 0.3f, 0.6f, 0.4f)
                if (glyph == Glyph.LoopOne) {
                    drawCircle(color, w * 0.13f, Offset(w * 0.82f, h * 0.78f))
                    drawCircle(Color.Black, w * 0.07f, Offset(w * 0.82f, h * 0.78f))
                }
            }
            Glyph.Star, Glyph.StarOutline -> drawStar(color, glyph == Glyph.Star, sw)
            Glyph.Up -> { line(0.2f, 0.65f, 0.5f, 0.35f); line(0.5f, 0.35f, 0.8f, 0.65f) }
            Glyph.Down -> { line(0.2f, 0.35f, 0.5f, 0.65f); line(0.5f, 0.65f, 0.8f, 0.35f) }
            Glyph.Close -> { line(0.25f, 0.25f, 0.75f, 0.75f); line(0.75f, 0.25f, 0.25f, 0.75f) }
        }
    }
}

private fun DrawScope.drawStar(color: Color, filled: Boolean, sw: Float) {
    val cx = size.width / 2; val cy = size.height * 0.53f
    val ro = size.width * 0.42f; val ri = ro * 0.42f
    val p = Path()
    for (i in 0 until 10) {
        val r = if (i % 2 == 0) ro else ri
        val a = Math.toRadians(-90.0 + i * 36.0)
        val x = cx + (r * Math.cos(a)).toFloat(); val y = cy + (r * Math.sin(a)).toFloat()
        if (i == 0) p.moveTo(x, y) else p.lineTo(x, y)
    }
    p.close()
    if (filled) drawPath(p, color) else drawPath(p, color, style = Stroke(sw * 0.8f))
}

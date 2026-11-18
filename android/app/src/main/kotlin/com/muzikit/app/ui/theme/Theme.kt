package com.muzikit.app.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import com.muzikit.app.R

/** Masaüstüyle aynı ruh: siyah zemin, beyaz yazı, gri ayrıntılar. Renkli vurgu yok. */
object MuzikitColors {
    val Black = Color(0xFF000000)
    val White = Color(0xFFFFFFFF)
    val Gray = Color(0xFF9E9E9E)
    val DarkGray = Color(0xFF424242)
    val Card = Color(0x14FFFFFF) // yarı saydam kart
    val Panel = Color(0xFF141414) // alttaki çalar paneli: opak (arka plan yazıları altında görünmesin)
}

private val Scheme = darkColorScheme(
    primary = MuzikitColors.White,
    onPrimary = MuzikitColors.Black,
    secondary = MuzikitColors.Gray,
    onSecondary = MuzikitColors.Black,
    background = MuzikitColors.Black,
    onBackground = MuzikitColors.White,
    surface = MuzikitColors.Black,
    onSurface = MuzikitColors.White,
    surfaceVariant = MuzikitColors.Card,
    onSurfaceVariant = MuzikitColors.Gray,
    outline = MuzikitColors.DarkGray,
)

// Masaüstüyle aynı yazı tipi: DejaVu Sans Mono (APK'ya gömülü; lisans: assets/licenses/dejavu-fonts.txt).
// Japonca/Çince vb. bu yazı tipinde olmayan harfler için Android otomatik olarak sistem yazı tiplerine düşer.
val Mono = FontFamily(
    Font(R.font.dejavu_sans_mono, FontWeight.Normal),
    Font(R.font.dejavu_sans_mono_bold, FontWeight.Bold),
)
private val Base = Typography()
private val MonoTypography = Typography(
    headlineMedium = Base.headlineMedium.copy(fontFamily = Mono),
    titleMedium = Base.titleMedium.copy(fontFamily = Mono),
    bodyLarge = Base.bodyLarge.copy(fontFamily = Mono),
    bodyMedium = Base.bodyMedium.copy(fontFamily = Mono),
    bodySmall = Base.bodySmall.copy(fontFamily = Mono),
    labelLarge = Base.labelLarge.copy(fontFamily = Mono),
    labelSmall = Base.labelSmall.copy(fontFamily = Mono),
)

@Composable
fun MuzikitTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = Scheme, typography = MonoTypography, content = content)
}


package co.huggingface.translatte.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

private val LatteBackground = Color(0xFFFBF6EE)
private val LatteSurface = Color(0xFFF3E8D9)
private val LatteSurfaceVariant = Color(0xFFE9D8C3)
private val LatteSheet = Color(0xFFFFFCF7)
private val LatteOutline = Color(0xFFCDB79F)
private val LattePrimary = Color(0xFFA4501A)
private val LatteOnPrimary = Color(0xFFFFF8F0)
private val LattePrimaryContainer = Color(0xFFF1D5B8)
private val LatteOnPrimaryContainer = Color(0xFF3E1F08)
private val LatteText = Color(0xFF2B1D14)
private val LatteTextMuted = Color(0xFF6B5444)

private val AmericanoBackground = Color(0xFF16110E)
private val AmericanoSurface = Color(0xFF201914)
private val AmericanoSurfaceVariant = Color(0xFF2C231C)
private val AmericanoSheet = Color(0xFF241C17)
private val AmericanoOutline = Color(0xFF4A3B30)
private val AmericanoPrimary = Color(0xFFE09A5F)
private val AmericanoOnPrimary = Color(0xFF2A1406)
private val AmericanoPrimaryContainer = Color(0xFF5A3216)
private val AmericanoOnPrimaryContainer = Color(0xFFF6DCC2)
private val AmericanoText = Color(0xFFF1E6D8)
private val AmericanoTextMuted = Color(0xFFBBA894)

val TranslatteColors = lightColorScheme(
    primary = LattePrimary,
    onPrimary = LatteOnPrimary,
    primaryContainer = LattePrimaryContainer,
    onPrimaryContainer = LatteOnPrimaryContainer,
    secondary = LattePrimary,
    onSecondary = LatteOnPrimary,
    secondaryContainer = LattePrimaryContainer,
    onSecondaryContainer = LatteOnPrimaryContainer,
    tertiary = LattePrimary,
    onTertiary = LatteOnPrimary,
    tertiaryContainer = LattePrimaryContainer,
    onTertiaryContainer = LatteOnPrimaryContainer,
    background = LatteBackground,
    onBackground = LatteText,
    surface = LatteSurface,
    onSurface = LatteText,
    surfaceVariant = LatteSurfaceVariant,
    onSurfaceVariant = LatteTextMuted,
    surfaceTint = LattePrimary,
    outline = LatteOutline,
    outlineVariant = LatteOutline,
    error = Color(0xFFB3261E),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFF9DEDC),
    onErrorContainer = Color(0xFF410E0B),
    surfaceBright = LatteBackground,
    surfaceDim = LatteSurfaceVariant,
    surfaceContainerLowest = LatteSheet,
    surfaceContainerLow = LatteSheet,
    surfaceContainer = LatteSurface,
    surfaceContainerHigh = LatteSurfaceVariant,
    surfaceContainerHighest = LatteSurfaceVariant,
)

val AmericanoColors = darkColorScheme(
    primary = AmericanoPrimary,
    onPrimary = AmericanoOnPrimary,
    primaryContainer = AmericanoPrimaryContainer,
    onPrimaryContainer = AmericanoOnPrimaryContainer,
    secondary = AmericanoPrimary,
    onSecondary = AmericanoOnPrimary,
    secondaryContainer = AmericanoPrimaryContainer,
    onSecondaryContainer = AmericanoOnPrimaryContainer,
    tertiary = AmericanoPrimary,
    onTertiary = AmericanoOnPrimary,
    tertiaryContainer = AmericanoPrimaryContainer,
    onTertiaryContainer = AmericanoOnPrimaryContainer,
    background = AmericanoBackground,
    onBackground = AmericanoText,
    surface = AmericanoSurface,
    onSurface = AmericanoText,
    surfaceVariant = AmericanoSurfaceVariant,
    onSurfaceVariant = AmericanoTextMuted,
    surfaceTint = AmericanoPrimary,
    outline = AmericanoOutline,
    outlineVariant = AmericanoOutline,
    error = Color(0xFFF2B8B5),
    onError = Color(0xFF601410),
    errorContainer = Color(0xFF8C1D18),
    onErrorContainer = Color(0xFFF9DEDC),
    surfaceBright = AmericanoSurfaceVariant,
    surfaceDim = AmericanoBackground,
    surfaceContainerLowest = AmericanoBackground,
    surfaceContainerLow = AmericanoSheet,
    surfaceContainer = AmericanoSurface,
    surfaceContainerHigh = AmericanoSurfaceVariant,
    surfaceContainerHighest = AmericanoSurfaceVariant,
)

private val TranslatteGlass = Color(0xDBFBF6EE)
private val AmericanoGlass = Color(0xC716110E)

/** Translucent fill for controls floating over a photo. */
val LocalGlass = staticCompositionLocalOf { TranslatteGlass }

/** The window background drawn before Compose's first frame. */
fun windowBackground(dark: Boolean): Color = if (dark) AmericanoBackground else LatteBackground

@Composable
fun TranslatteTheme(dark: Boolean, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalGlass provides if (dark) AmericanoGlass else TranslatteGlass) {
        MaterialTheme(
            colorScheme = if (dark) AmericanoColors else TranslatteColors,
            typography = AppTypography,
            content = content,
        )
    }
}


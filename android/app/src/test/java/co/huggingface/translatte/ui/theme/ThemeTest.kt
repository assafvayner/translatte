package co.huggingface.translatte.ui.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Every role in the spec's palette table, written out as the spec gives it. */
@RunWith(AndroidJUnit4::class)
class ThemeTest {
    @get:Rule
    val compose = createComposeRule()

    private fun hex(rgb: String) = Color(("FF" + rgb.removePrefix("#")).toLong(16))

    private fun rgba(r: Int, g: Int, b: Int, a: Float) = Color(r, g, b, (a * 255 + 0.5f).toInt())

    private class Spec(
        val background: String,
        val surface: String,
        val surfaceVariant: String,
        val sheet: String,
        val outline: String,
        val primary: String,
        val onPrimary: String,
        val primaryContainer: String,
        val onPrimaryContainer: String,
        val text: String,
        val textMuted: String,
        val error: String,
    )

    private val translatte = Spec(
        "#FBF6EE", "#F3E8D9", "#E9D8C3", "#FFFCF7", "#CDB79F", "#A4501A",
        "#FFF8F0", "#F1D5B8", "#3E1F08", "#2B1D14", "#6B5444", "#B3261E",
    )
    private val americano = Spec(
        "#16110E", "#201914", "#2C231C", "#241C17", "#4A3B30", "#E09A5F",
        "#2A1406", "#5A3216", "#F6DCC2", "#F1E6D8", "#BBA894", "#F2B8B5",
    )

    private fun capture(dark: Boolean): Pair<ColorScheme, Color> {
        lateinit var scheme: ColorScheme
        var glass = Color.Unspecified
        compose.setContent {
            TranslatteTheme(dark) {
                scheme = MaterialTheme.colorScheme
                glass = LocalGlass.current
            }
        }
        compose.waitForIdle()
        return scheme to glass
    }

    private fun assertMatches(spec: Spec, s: ColorScheme) {
        assertEquals("background", hex(spec.background), s.background)
        assertEquals("surface", hex(spec.surface), s.surface)
        assertEquals("surfaceVariant", hex(spec.surfaceVariant), s.surfaceVariant)
        assertEquals("sheet", hex(spec.sheet), s.surfaceContainerLow)
        assertEquals("outline", hex(spec.outline), s.outline)
        assertEquals("primary", hex(spec.primary), s.primary)
        assertEquals("onPrimary", hex(spec.onPrimary), s.onPrimary)
        assertEquals("primaryContainer", hex(spec.primaryContainer), s.primaryContainer)
        assertEquals("onPrimaryContainer", hex(spec.onPrimaryContainer), s.onPrimaryContainer)
        assertEquals("onBackground", hex(spec.text), s.onBackground)
        assertEquals("onSurface", hex(spec.text), s.onSurface)
        assertEquals("onSurfaceVariant", hex(spec.textMuted), s.onSurfaceVariant)
        assertEquals("error", hex(spec.error), s.error)
    }

    @Test
    fun translatteMatchesTheSpec() {
        val (s, glass) = capture(dark = false)
        assertMatches(translatte, s)
        assertEquals(rgba(251, 246, 238, 0.86f), glass)
        assertEquals(hex(translatte.background), windowBackground(dark = false))
    }

    @Test
    fun americanoMatchesTheSpec() {
        val (s, glass) = capture(dark = true)
        assertMatches(americano, s)
        assertEquals(rgba(22, 17, 14, 0.78f), glass)
        assertEquals(hex(americano.background), windowBackground(dark = true))
    }
}

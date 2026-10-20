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

@RunWith(AndroidJUnit4::class)
class ThemeTest {
    @get:Rule
    val compose = createComposeRule()

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

    @Test
    fun translatteMatchesTheSpec() {
        val (s, glass) = capture(dark = false)
        assertEquals(Color(0xFFFBF6EE), s.background)
        assertEquals(Color(0xFFF3E8D9), s.surface)
        assertEquals(Color(0xFFA4501A), s.primary)
        assertEquals(Color(0xFF2B1D14), s.onBackground)
        assertEquals(Color(0xFF6B5444), s.onSurfaceVariant)
        assertEquals(Color(0xDBFBF6EE), glass)
    }

    @Test
    fun americanoMatchesTheSpec() {
        val (s, glass) = capture(dark = true)
        assertEquals(Color(0xFF16110E), s.background)
        assertEquals(Color(0xFF201914), s.surface)
        assertEquals(Color(0xFFE09A5F), s.primary)
        assertEquals(Color(0xFFF1E6D8), s.onBackground)
        assertEquals(Color(0xFFBBA894), s.onSurfaceVariant)
        assertEquals(Color(0xC716110E), glass)
    }
}

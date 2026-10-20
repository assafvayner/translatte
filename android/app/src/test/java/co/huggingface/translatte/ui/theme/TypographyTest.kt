package co.huggingface.translatte.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TypographyTest {
    @get:Rule
    val compose = createComposeRule()

    private fun Typography.all() = mapOf(
        "displayLarge" to displayLarge, "displayMedium" to displayMedium, "displaySmall" to displaySmall,
        "headlineLarge" to headlineLarge, "headlineMedium" to headlineMedium, "headlineSmall" to headlineSmall,
        "titleLarge" to titleLarge, "titleMedium" to titleMedium, "titleSmall" to titleSmall,
        "bodyLarge" to bodyLarge, "bodyMedium" to bodyMedium, "bodySmall" to bodySmall,
        "labelLarge" to labelLarge, "labelMedium" to labelMedium, "labelSmall" to labelSmall,
    )

    @Test
    fun everyMaterialStyleIsFraunces() {
        lateinit var typography: Typography
        compose.setContent { TranslatteTheme(dark = false) { typography = MaterialTheme.typography } }
        compose.waitForIdle()
        val styles = typography.all()
        assertEquals(15, styles.size)
        styles.forEach { (name, style) ->
            assertTrue(name, style.fontFamily == FrauncesDisplay || style.fontFamily == FrauncesText)
        }
    }

    @Test
    fun materialStylesMapToTheSharedTokens() {
        assertEquals(AppType.title, AppTypography.headlineSmall)
        assertEquals(AppType.title, AppTypography.titleLarge)
        assertEquals(AppType.headline, AppTypography.titleMedium)
        assertEquals(AppType.body, AppTypography.bodyLarge)
        assertEquals(AppType.body, AppTypography.bodyMedium)
        assertEquals(AppType.caption, AppTypography.bodySmall)
        assertEquals(AppType.button, AppTypography.labelLarge)
        assertEquals(AppType.label, AppTypography.labelMedium)
        assertEquals(AppType.overline, AppTypography.labelSmall)
    }

    @Test
    fun tokenSizesMatchTheSpec() {
        val expected = listOf(
            AppType.wordmark to 26, AppType.display to 24, AppType.title to 20, AppType.headline to 17,
            AppType.body to 16, AppType.button to 16, AppType.label to 14, AppType.caption to 13, AppType.overline to 11,
        )
        expected.forEach { (style, size) -> assertEquals(size.sp, style.fontSize) }
        listOf(AppType.wordmark, AppType.display, AppType.title).forEach {
            assertEquals(FrauncesDisplay, it.fontFamily)
            assertEquals(FontWeight.SemiBold, it.fontWeight)
        }
        assertEquals(FontWeight.Normal, AppType.body.fontWeight)
        assertEquals(FontWeight.Normal, AppType.caption.fontWeight)
        assertEquals(0.8.sp, AppType.overline.letterSpacing)
        assertEquals((-0.3).sp, AppType.wordmark.letterSpacing)
    }
}

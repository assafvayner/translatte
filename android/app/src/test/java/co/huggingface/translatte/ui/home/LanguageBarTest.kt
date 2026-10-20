package co.huggingface.translatte.ui.home

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.ui.theme.TranslatteTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.translatte_ffi.LanguageInfo

@RunWith(AndroidJUnit4::class)
class LanguageBarTest {
    @get:Rule
    val compose = createComposeRule()

    private val fr = LanguageInfo("fr", "French", false)
    private val en = LanguageInfo("en", "English", false)
    private val ja = LanguageInfo("ja", "Japanese", false)

    @Test
    fun directionLabelUsesNames() {
        assertEquals("Japanese → English", directionLabel(listOf(ja), listOf(en, ja), "ja", "en"))
    }

    @Test
    fun swapIsDisabledWhenTheTargetIsUnreadable() {
        compose.setContent {
            TranslatteTheme(dark = false) {
                LanguageBar(listOf(fr), listOf(en, fr, ja), "fr", "ja", {}, {}, {})
            }
        }
        compose.onNodeWithText("French").assertExists()
        compose.onNodeWithText("Japanese").assertExists()
        compose.onNodeWithContentDescription("Swap languages").assertIsNotEnabled()
    }

    @Test
    fun swapCallsBack() {
        var swaps = 0
        compose.setContent {
            TranslatteTheme(dark = false) {
                LanguageBar(listOf(fr, en), listOf(en, fr), "fr", "en", {}, {}, { swaps++ })
            }
        }
        compose.onNodeWithContentDescription("Swap languages").assertIsEnabled().performClick()
        assertEquals(1, swaps)
    }
}

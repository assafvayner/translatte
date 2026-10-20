package co.huggingface.translatte.ui.home

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
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

    @Test
    fun fromListsOnlyInstalledSourcesAndClosesOnSelect() {
        var source = "fr"
        compose.setContent {
            TranslatteTheme(dark = false) {
                LanguageBar(listOf(en, fr), listOf(en, fr, ja), source, "ja", { source = it }, {}, {})
            }
        }
        compose.onNodeWithText("FROM").performClick()
        compose.onNodeWithText("TRANSLATE FROM").assertExists()
        compose.onNodeWithText("English").assertExists()
        compose.onAllNodesWithText("Japanese").assertCountEquals(1)
        compose.onNode(hasText("French") and isSelectable()).assertIsSelected()

        compose.onNodeWithText("English").performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
        assertEquals("en", source)
        compose.onNodeWithText("TRANSLATE FROM").assertDoesNotExist()
    }

    @Test
    fun toListsEveryLanguage() {
        var target = "en"
        compose.setContent {
            TranslatteTheme(dark = false) {
                LanguageBar(listOf(fr), listOf(en, fr, ja), "fr", target, {}, { target = it }, {})
            }
        }
        compose.onNodeWithText("TO").performClick()
        compose.onNodeWithText("TRANSLATE TO").assertExists()
        compose.onNode(hasText("Japanese") and isSelectable()).assertIsNotSelected()
            .performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
        assertEquals("ja", target)
        compose.onNodeWithText("TRANSLATE TO").assertDoesNotExist()
    }
}

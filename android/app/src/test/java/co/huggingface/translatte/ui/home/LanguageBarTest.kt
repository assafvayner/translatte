package co.huggingface.translatte.ui.home

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
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
        assertEquals("Japanese → English", directionLabel(listOf(ja), "ja", en))
        assertEquals("xx → English", directionLabel(emptyList(), "xx", null))
    }

    @Test
    fun toIsFixedToTheTarget() {
        compose.setContent {
            TranslatteTheme(dark = false) {
                LanguageBar(listOf(fr), en, "fr", {})
            }
        }
        compose.onNodeWithText("French").assertExists()
        compose.onNodeWithText("English").assertExists()
        compose.onNodeWithText("TO").assertIsNotEnabled().performClick()
        compose.onNodeWithText("TRANSLATE TO").assertDoesNotExist()
        compose.onNodeWithContentDescription("Swap languages").assertDoesNotExist()
    }

    @Test
    fun fromListsOnlyInstalledSourcesAndClosesOnSelect() {
        var source = "fr"
        compose.setContent {
            TranslatteTheme(dark = false) {
                LanguageBar(listOf(fr, ja), en, source, { source = it })
            }
        }
        val chip = compose.onNodeWithText("FROM")
        chip.assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Collapsed"))
        chip.performClick()
        compose.onNodeWithText("TRANSLATE FROM").assertExists()
        chip.assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Expanded"))
        compose.onNode(hasText("French") and isSelectable()).assertIsSelected()
        compose.onNode(hasText("Japanese") and isSelectable()).assertIsNotSelected()
            .performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()
        assertEquals("ja", source)
        compose.onNodeWithText("TRANSLATE FROM").assertDoesNotExist()
    }
}

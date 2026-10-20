package co.huggingface.translatte.ui.result

import android.graphics.Bitmap
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.click
import androidx.compose.ui.test.doubleClick
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.percentOffset
import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.Phase
import co.huggingface.translatte.ui.theme.TranslatteTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.translatte_ffi.TranslationResult

@RunWith(AndroidJUnit4::class)
class ResultScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val bitmap = Bitmap.createBitmap(400, 300, Bitmap.Config.ARGB_8888)

    private fun show(
        phase: Phase,
        onRetry: () -> Unit = {},
        onHintSeen: () -> Unit = {},
        image: Bitmap = bitmap,
        result: TranslationResult? = null,
    ) {
        compose.setContent {
            TranslatteTheme(dark = false) {
                ResultScreen(
                    image = image,
                    result = result,
                    phase = phase,
                    direction = "French → English",
                    hintSeen = false,
                    onHintSeen = onHintSeen,
                    onRetry = onRetry,
                    onBack = {},
                )
            }
        }
    }

    @Test
    fun doubleTapZoomsInAndBackToFit() {
        var hintSeen = false
        show(Phase.Done, onHintSeen = { hintSeen = true })
        compose.onNodeWithText("250%").assertDoesNotExist()

        compose.onNodeWithTag("viewer").performTouchInput { doubleClick(center) }
        compose.onNodeWithText("250%").assertExists()
        assertTrue(hintSeen)

        compose.onNodeWithTag("viewer").performTouchInput { doubleClick(center) }
        compose.onNodeWithText("250%").assertDoesNotExist()
    }

    @Test
    fun doubleTapOnABoxZoomsWithoutFlippingIt() {
        show(Phase.Done, image = Bitmap.createBitmap(100, 100, Bitmap.Config.ARGB_8888), result = testResult())
        fun clickLabel(label: String) =
            SemanticsMatcher("click label $label") { it.config.getOrNull(SemanticsActions.OnClick)?.label == label }

        compose.onNodeWithContentDescription("Hello").performTouchInput { doubleClick(center) }
        compose.onNodeWithText("250%").assertExists()
        compose.onNodeWithContentDescription("Hello").assert(clickLabel("Show original"))

        compose.onNodeWithContentDescription("Hello").performTouchInput { click(percentOffset(0.75f, 0.5f)) }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNodeWithContentDescription("Hello").assert(clickLabel("Show translation"))
    }

    @Test
    fun progressShowsTheStage() {
        show(Phase.Working("Translating…"))
        compose.onNodeWithText("Translating…").assertExists()
    }

    @Test
    fun retryableErrorsOfferRetry() {
        var retries = 0
        show(Phase.Failed("Translation failed: boom"), onRetry = { retries++ })
        compose.onNodeWithText("Retry").performClick()
        assertEquals(1, retries)
    }

    @Test
    fun noTextFoundHasNoRetry() {
        show(Phase.Failed("No text found. Try moving closer or reducing glare.", retryable = false))
        compose.onNodeWithText("No text found. Try moving closer or reducing glare.").assertExists()
        compose.onNodeWithText("Retry").assertDoesNotExist()
        compose.onNodeWithText("Back").assertExists()
    }
}

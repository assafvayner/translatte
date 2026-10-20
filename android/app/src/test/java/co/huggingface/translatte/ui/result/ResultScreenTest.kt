package co.huggingface.translatte.ui.result

import android.graphics.Bitmap
import androidx.compose.ui.test.doubleClick
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.Phase
import co.huggingface.translatte.ui.theme.TranslatteTheme
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ResultScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private val bitmap = Bitmap.createBitmap(400, 300, Bitmap.Config.ARGB_8888)

    private fun show(phase: Phase, onRetry: () -> Unit = {}, onHintSeen: () -> Unit = {}) {
        compose.setContent {
            TranslatteTheme(dark = false) {
                ResultScreen(
                    image = bitmap,
                    result = null,
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

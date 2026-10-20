package co.huggingface.translatte.ui.result

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb
import uniffi.translatte_ffi.TranslationResult

fun testRegion(translated: String, source: String, x: Float, y: Float) = Region(
    corners = listOf(Point(x, y), Point(x + 40f, y), Point(x + 40f, y + 15f), Point(x, y + 15f)),
    sourceText = source,
    translatedText = translated,
    background = Rgb(255.toUByte(), 255.toUByte(), 255.toUByte()),
    foreground = Rgb(0.toUByte(), 0.toUByte(), 0.toUByte()),
    angle = 0f,
)

fun testResult() = TranslationResult(
    imageWidth = 100u,
    imageHeight = 100u,
    regions = listOf(testRegion("Hello", "Bonjour", 10f, 10f), testRegion("Exit", "Sortie", 10f, 60f)),
    translationError = null,
)

@RunWith(AndroidJUnit4::class)
class OverlayTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun tappingABoxFlipsOnlyThatBox() {
        val bitmap = Bitmap.createBitmap(100, 100, Bitmap.Config.ARGB_8888)
        compose.setContent {
            var flipped by remember { mutableStateOf(emptySet<Int>()) }
            Column {
                OverlayImage(
                    bitmap,
                    testResult(),
                    showOriginal = false,
                    modifier = Modifier.size(200.dp),
                    flipped = flipped,
                    onToggle = { i -> flipped = if (i in flipped) flipped - i else flipped + i },
                )
                Text("flipped=${flipped.sorted()}")
            }
        }

        compose.onNodeWithContentDescription("Hello")
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, "Bonjour"))
            .performClick()
        compose.onNodeWithText("flipped=[0]").assertExists()

        compose.onNodeWithContentDescription("Hello").performClick()
        compose.onNodeWithText("flipped=[]").assertExists()
    }
}

package co.huggingface.translatte.ui.home

import android.graphics.Bitmap
import androidx.compose.material3.Text
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

@RunWith(AndroidJUnit4::class)
class HomeScreenTest {
    @get:Rule
    val compose = createComposeRule()

    private var opened = 0

    private fun show(lastImage: Bitmap?, hasCamera: Boolean = true, memoryWarning: String? = null) {
        compose.setContent {
            TranslatteTheme(dark = false) {
                HomeScreen(
                    languageBar = { Text("language bar") },
                    lastImage = lastImage,
                    lastResult = null,
                    working = false,
                    memoryWarning = memoryWarning,
                    hasCamera = hasCamera,
                    onCamera = {},
                    onPhotos = {},
                    onOpenResult = { opened++ },
                    onLanguages = {},
                    onSettings = {},
                )
            }
        }
    }

    @Test
    fun emptyStateWithoutALastResult() {
        show(lastImage = null)
        compose.onNodeWithText("translatte").assertExists()
        compose.onNodeWithText("Point at a sign, menu or label").assertExists()
        compose.onNodeWithText("Camera").assertExists()
        compose.onNodeWithText("Photos").assertExists()
        compose.onNodeWithContentDescription("Settings").assertExists()
    }

    @Test
    fun lastResultReplacesTheEmptyStateAndReopens() {
        show(lastImage = Bitmap.createBitmap(40, 30, Bitmap.Config.ARGB_8888))
        compose.onNodeWithText("Point at a sign, menu or label").assertDoesNotExist()
        compose.onNodeWithContentDescription("Last result").performClick()
        assertEquals(1, opened)
    }

    @Test
    fun noCameraHidesTheCameraButton() {
        show(lastImage = null, hasCamera = false)
        compose.onNodeWithText("Camera").assertDoesNotExist()
        compose.onNodeWithText("Photos").assertExists()
    }

    @Test
    fun lowMemoryShowsABanner() {
        show(lastImage = null, memoryWarning = "Low memory (1 GB free)")
        compose.onNodeWithText("Low memory (1 GB free)").assertExists()
    }
}

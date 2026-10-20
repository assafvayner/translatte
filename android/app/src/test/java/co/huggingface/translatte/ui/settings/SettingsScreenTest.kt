package co.huggingface.translatte.ui.settings

import android.content.Context
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import co.huggingface.translatte.BuildConfig
import co.huggingface.translatte.ui.theme.AmericanoColors
import co.huggingface.translatte.ui.theme.TranslatteColors
import co.huggingface.translatte.ui.theme.TranslatteTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SettingsScreenTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun choosingAmericanoPersistsAndApplies() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val prefs = UiPrefs(context)
        var background = Color.Unspecified
        compose.setContent {
            var theme by remember { mutableStateOf(prefs.theme) }
            TranslatteTheme(dark = theme.isDark(systemDark = false)) {
                background = MaterialTheme.colorScheme.background
                SettingsScreen(theme, onTheme = {
                    prefs.theme = it
                    theme = it
                }, onBack = {})
            }
        }
        assertEquals(TranslatteColors.background, background)

        compose.onNodeWithText("americano").performClick()
        compose.waitForIdle()

        assertEquals(AmericanoColors.background, background)
        assertEquals(ThemeSetting.AMERICANO, UiPrefs(context).theme)
    }

    @Test
    fun aboutShowsVersionAndCredits() {
        compose.setContent { TranslatteTheme(dark = false) { SettingsScreen(ThemeSetting.SYSTEM, {}, {}) } }
        compose.onNodeWithText(BuildConfig.VERSION_NAME).assertExists()
        compose.onNodeWithText("PP-OCRv5", substring = true).assertExists()
        compose.onNodeWithText("TranslateGemma", substring = true).assertExists()
    }
}

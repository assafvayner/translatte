package co.huggingface.translatte.ui.settings

import android.app.UiModeManager
import android.content.Context
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelectable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
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

    /** Goes through [AppTheme], the same wiring MainActivity uses. */
    @Test
    fun choosingAmericanoPersistsAndApplies() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val prefs = UiPrefs(context).apply { theme = ThemeSetting.SYSTEM }
        val applied = mutableListOf<Int>()
        var background = Color.Unspecified
        compose.setContent {
            AppTheme(prefs, applyNightMode = { applied += it.nightMode }) { theme, _, onTheme ->
                background = MaterialTheme.colorScheme.background
                SettingsScreen(theme, onTheme, onBack = {})
            }
        }
        assertEquals(TranslatteColors.background, background)

        compose.onNodeWithText("americano").performClick()
        compose.waitForIdle()

        assertEquals(AmericanoColors.background, background)
        assertEquals(ThemeSetting.AMERICANO, UiPrefs(context).theme)
        assertEquals(listOf(UiModeManager.MODE_NIGHT_YES), applied)

        compose.onNodeWithText("translatte").performClick()
        compose.waitForIdle()
        assertEquals(TranslatteColors.background, background)
        assertEquals(UiModeManager.MODE_NIGHT_NO, applied.last())
    }

    @Test
    fun aSavedChoiceAppliesOnLaunch() {
        val prefs = UiPrefs(ApplicationProvider.getApplicationContext()).apply { theme = ThemeSetting.AMERICANO }
        var dark = false
        compose.setContent { AppTheme(prefs, applyNightMode = {}) { _, d, _ -> dark = d } }
        compose.waitForIdle()
        assertEquals(true, dark)
    }

    @Test
    fun aboutShowsVersionAndCredits() {
        compose.setContent { TranslatteTheme(dark = false) { SettingsScreen(ThemeSetting.SYSTEM, {}, {}) } }
        compose.onNodeWithText(BuildConfig.VERSION_NAME).assertExists()
        compose.onNodeWithText("PP-OCRv5", substring = true).assertExists()
        compose.onNodeWithText("TranslateGemma", substring = true).assertExists()
    }

    @Test
    fun theFontLicenseOpensInTheApp() {
        compose.setContent { TranslatteTheme(dark = false) { SettingsScreen(ThemeSetting.SYSTEM, {}, {}) } }
        compose.onNodeWithText("Font").performScrollTo().performClick()
        compose.onNodeWithText("Fraunces · SIL Open Font License 1.1").assertExists()
        compose.onNodeWithText("SIL Open Font License 1.1").assertExists()
        compose.onNodeWithText("SIL OPEN FONT LICENSE Version 1.1", substring = true).assertExists()
        compose.onNodeWithText("Close").performClick()
    }

    @Test
    fun themeRowsCheckTheCurrentChoice() {
        var chosen = ThemeSetting.SYSTEM
        compose.setContent {
            TranslatteTheme(dark = false) { SettingsScreen(chosen, { chosen = it }, {}) }
        }
        compose.onNode(hasText("System") and isSelectable()).assertIsSelected()
        compose.onNode(hasText("americano") and isSelectable()).assertIsNotSelected().performClick()
        assertEquals(ThemeSetting.AMERICANO, chosen)
        compose.onNodeWithText("Light").assertExists()
        compose.onNodeWithText("Dark").assertExists()
    }
}

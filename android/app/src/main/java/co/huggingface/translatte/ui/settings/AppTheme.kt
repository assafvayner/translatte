package co.huggingface.translatte.ui.settings

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import co.huggingface.translatte.ui.theme.TranslatteTheme

/**
 * Holds the theme choice for the whole app: [content] gets the current setting, whether it resolves
 * to dark, and a setter that persists the choice and passes it to [applyNightMode].
 */
@Composable
fun AppTheme(
    prefs: UiPrefs,
    applyNightMode: (ThemeSetting) -> Unit,
    content: @Composable (theme: ThemeSetting, dark: Boolean, onTheme: (ThemeSetting) -> Unit) -> Unit,
) {
    var theme by remember { mutableStateOf(prefs.theme) }
    val dark = theme.isDark(isSystemInDarkTheme())
    TranslatteTheme(dark) {
        content(theme, dark) {
            prefs.theme = it
            theme = it
            applyNightMode(it)
        }
    }
}

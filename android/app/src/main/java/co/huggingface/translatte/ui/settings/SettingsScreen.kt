package co.huggingface.translatte.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import co.huggingface.translatte.BuildConfig
import co.huggingface.translatte.ui.common.AppDialog
import co.huggingface.translatte.ui.common.AppTopBar
import co.huggingface.translatte.ui.common.CheckRow
import co.huggingface.translatte.ui.common.ListRow
import co.huggingface.translatte.ui.common.SectionCard
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.Dimens

@Composable
fun SettingsScreen(theme: ThemeSetting, onTheme: (ThemeSetting) -> Unit, onBack: () -> Unit) {
    BackHandler(onBack = onBack)
    val uri = LocalUriHandler.current
    var fontLicense by rememberSaveable { mutableStateOf(false) }
    Scaffold(topBar = { AppTopBar("Settings", onBack) }) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(Dimens.gutter),
            verticalArrangement = Arrangement.spacedBy(Dimens.spacing),
        ) {
            SectionCard("Theme") {
                Column(Modifier.selectableGroup()) {
                    ThemeSetting.entries.forEach { t ->
                        CheckRow(t.label, selected = t == theme, onClick = { onTheme(t) }, subtitle = t.description)
                    }
                }
            }
            SectionCard("About") {
                ListRow("Version", subtitle = BuildConfig.VERSION_NAME)
                InfoRow(
                    "Text detection and recognition",
                    "PP-OCRv5 by PaddlePaddle · Apache License 2.0",
                    onOpen = { uri.openUri("https://www.apache.org/licenses/LICENSE-2.0") },
                )
                InfoRow(
                    "Translation",
                    "TranslateGemma 4B by Google · Gemma Terms of Use",
                    onOpen = { uri.openUri("https://ai.google.dev/gemma/terms") },
                )
                InfoRow(
                    "Font",
                    "Fraunces · SIL Open Font License 1.1",
                    external = false,
                    onOpen = { fontLicense = true },
                )
            }
        }
    }
    if (fontLicense) LicenseDialog("SIL Open Font License 1.1", FONT_LICENSE) { fontLicense = false }
}

private const val FONT_LICENSE = "licenses/fraunces-ofl.txt"

@Composable
private fun LicenseDialog(title: String, asset: String, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val text = remember(asset) { context.assets.open(asset).bufferedReader().use { it.readText() } }
    AppDialog(title, onDismiss, confirm = "Close" to onDismiss) {
        Text(text, style = AppType.caption, modifier = Modifier.verticalScroll(rememberScrollState()))
    }
}

@Composable
private fun InfoRow(title: String, value: String, external: Boolean = true, onOpen: () -> Unit) {
    ListRow(
        title,
        Modifier.clickable(onClickLabel = "Open license", onClick = onOpen),
        subtitle = value,
        trailing = {
            Icon(
                if (external) Icons.AutoMirrored.Outlined.OpenInNew else Icons.AutoMirrored.Outlined.KeyboardArrowRight,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        },
    )
}

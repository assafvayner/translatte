package co.huggingface.translatte.ui.settings

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.BuildConfig
import co.huggingface.translatte.ui.theme.appBarColors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(theme: ThemeSetting, onTheme: (ThemeSetting) -> Unit, onBack: () -> Unit) {
    BackHandler(onBack = onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Settings") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = appBarColors(),
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(24.dp),
        ) {
            Section("Theme") {
                Column(Modifier.selectableGroup()) {
                    ThemeSetting.entries.forEach { t ->
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .selectable(selected = t == theme, onClick = { onTheme(t) }, role = Role.RadioButton)
                                .padding(horizontal = 16.dp, vertical = 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(selected = t == theme, onClick = null)
                            Spacer(Modifier.width(12.dp))
                            Column {
                                Text(t.label, style = MaterialTheme.typography.bodyLarge)
                                Text(
                                    t.description,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            }
            Section("About") {
                InfoRow("Version", BuildConfig.VERSION_NAME, link = null)
                InfoRow(
                    "Text detection and recognition",
                    "PP-OCRv5 by PaddlePaddle · Apache License 2.0",
                    link = "https://www.apache.org/licenses/LICENSE-2.0",
                )
                InfoRow(
                    "Translation",
                    "TranslateGemma 4B by Google · Gemma Terms of Use",
                    link = "https://ai.google.dev/gemma/terms",
                )
                InfoRow(
                    "Wordmark font",
                    "Fraunces · SIL Open Font License 1.1",
                    link = "https://openfontlicense.org/open-font-license-official-text/",
                )
            }
        }
    }
}

@Composable
private fun Section(title: String, content: @Composable ColumnScope.() -> Unit) {
    Column {
        Text(
            title,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 4.dp, bottom = 8.dp),
        )
        Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surface) {
            Column(Modifier.padding(vertical = 4.dp), content = content)
        }
    }
}

@Composable
private fun InfoRow(title: String, value: String, link: String?) {
    val uri = LocalUriHandler.current
    Row(
        Modifier
            .fillMaxWidth()
            .then(if (link != null) Modifier.clickable(onClickLabel = "Open license") { uri.openUri(link) } else Modifier)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(value, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (link != null) {
            Icon(
                Icons.AutoMirrored.Outlined.OpenInNew,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

package co.huggingface.translatte.ui.home

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.SwapHoriz
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.canSwap
import co.huggingface.translatte.ui.common.PickerDropdown
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.CardShape
import co.huggingface.translatte.ui.theme.InnerShape
import co.huggingface.translatte.ui.theme.overlineText
import uniffi.translatte_ffi.LanguageInfo

fun directionLabel(sources: List<LanguageInfo>, targets: List<LanguageInfo>, source: String, target: String): String {
    fun name(list: List<LanguageInfo>, code: String) = list.firstOrNull { it.code == code }?.name ?: code
    return "${name(sources, source)} → ${name(targets, target)}"
}

private enum class Side { From, To }

/** From lists installed source packs only; To lists every language. Each opens a dropdown under its chip. */
@Composable
fun LanguageBar(
    sources: List<LanguageInfo>,
    targets: List<LanguageInfo>,
    source: String,
    target: String,
    onSource: (String) -> Unit,
    onTarget: (String) -> Unit,
    onSwap: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var picking by rememberSaveable { mutableStateOf<Side?>(null) }
    Surface(modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surface) {
        Row(
            Modifier.padding(8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            LanguageTile("From", sources, source, { picking = Side.From }, Modifier.weight(1f)) { width ->
                PickerDropdown(
                    expanded = picking == Side.From,
                    onDismiss = { picking = null },
                    title = "Translate from",
                    options = sources,
                    selected = sources.firstOrNull { it.code == source },
                    label = { it.name },
                    onSelect = { onSource(it.code) },
                    anchorWidth = width,
                )
            }
            FilledIconButton(
                onClick = onSwap,
                enabled = canSwap(sources, target),
                shape = CircleShape,
                modifier = Modifier.size(44.dp),
                colors = IconButtonDefaults.filledIconButtonColors(
                    containerColor = MaterialTheme.colorScheme.primaryContainer,
                    contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
                ),
            ) {
                Icon(Icons.Filled.SwapHoriz, contentDescription = "Swap languages")
            }
            LanguageTile("To", targets, target, { picking = Side.To }, Modifier.weight(1f)) { width ->
                PickerDropdown(
                    expanded = picking == Side.To,
                    onDismiss = { picking = null },
                    title = "Translate to",
                    options = targets,
                    selected = targets.firstOrNull { it.code == target },
                    label = { it.name },
                    onSelect = { onTarget(it.code) },
                    anchorWidth = width,
                    alignEnd = true,
                )
            }
        }
    }
}

@Composable
private fun LanguageTile(
    label: String,
    languages: List<LanguageInfo>,
    selected: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    dropdown: @Composable (anchorWidth: Dp) -> Unit,
) {
    val density = LocalDensity.current
    var width by remember { mutableStateOf(0.dp) }
    Box(modifier.onSizeChanged { width = with(density) { it.width.toDp() } }) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(InnerShape)
                .background(MaterialTheme.colorScheme.background)
                .clickable(onClickLabel = "Choose $label language", onClick = onClick)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(overlineText(label), style = AppType.overline, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    languages.firstOrNull { it.code == selected }?.name ?: "—",
                    style = AppType.headline,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                Icon(Icons.Filled.KeyboardArrowDown, contentDescription = null, modifier = Modifier.size(20.dp))
            }
        }
        dropdown(width)
    }
}

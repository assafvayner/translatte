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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.Icon
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
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.ui.common.PickerDropdown
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.CardShape
import co.huggingface.translatte.ui.theme.InnerShape
import co.huggingface.translatte.ui.theme.overlineText
import uniffi.translatte_ffi.LanguageInfo

fun directionLabel(sources: List<LanguageInfo>, source: String, target: LanguageInfo?): String {
    val from = sources.firstOrNull { it.code == source }?.name ?: source
    return "$from → ${target?.name ?: "English"}"
}

/** From lists installed source languages in a dropdown under its chip; To is the fixed target. */
@Composable
fun LanguageBar(
    sources: List<LanguageInfo>,
    target: LanguageInfo?,
    source: String,
    onSource: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var picking by rememberSaveable { mutableStateOf(false) }
    val selected = sources.firstOrNull { it.code == source }
    Surface(modifier.fillMaxWidth(), shape = CardShape, color = MaterialTheme.colorScheme.surface) {
        Row(
            Modifier.padding(8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            LanguageTile("From", selected?.name, picking, { picking = true }, Modifier.weight(1f)) { bounds ->
                PickerDropdown(
                    expanded = picking,
                    onDismiss = { picking = false },
                    title = "Translate from",
                    options = sources,
                    selected = selected,
                    label = { it.name },
                    onSelect = { onSource(it.code) },
                    anchor = bounds,
                )
            }
            LanguageTile("To", target?.name, expanded = false, onClick = null, modifier = Modifier.weight(1f))
        }
    }
}

/** A FROM or TO chip; without [onClick] it's a fixed, disabled label. */
@Composable
private fun LanguageTile(
    label: String,
    name: String?,
    expanded: Boolean,
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    dropdown: (@Composable (anchor: Rect) -> Unit)? = null,
) {
    var bounds by remember { mutableStateOf(Rect.Zero) }
    val interaction = if (onClick != null) {
        Modifier
            .clickable(onClickLabel = "Choose $label language", role = Role.DropdownList, onClick = onClick)
            .semantics { stateDescription = if (expanded) "Expanded" else "Collapsed" }
    } else {
        Modifier.semantics(mergeDescendants = true) { disabled() }
    }
    Box(modifier.onGloballyPositioned { bounds = it.boundsInWindow() }) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(InnerShape)
                .background(MaterialTheme.colorScheme.background)
                .then(interaction)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(overlineText(label), style = AppType.overline, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    name ?: "—",
                    style = AppType.headline,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (onClick != null) {
                    Icon(Icons.Filled.KeyboardArrowDown, contentDescription = null, modifier = Modifier.size(20.dp))
                }
            }
        }
        dropdown?.invoke(bounds)
    }
}

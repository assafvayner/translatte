package co.huggingface.translatte.ui.common

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.CardShape
import co.huggingface.translatte.ui.theme.DialogShape
import co.huggingface.translatte.ui.theme.Dimens
import co.huggingface.translatte.ui.theme.LocalGlass
import co.huggingface.translatte.ui.theme.overlineText

@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    shape: Shape = CircleShape,
    content: @Composable () -> Unit,
) {
    Surface(
        modifier = modifier,
        shape = shape,
        color = LocalGlass.current,
        contentColor = MaterialTheme.colorScheme.onBackground,
        content = content,
    )
}

@Composable
fun GlassIconButton(icon: ImageVector, description: String, onClick: () -> Unit) {
    GlassSurface(shape = CircleShape) {
        IconButton(onClick = onClick) { Icon(icon, contentDescription = description) }
    }
}

/** A glass chip over the photo: label text in a 44dp capsule. */
@Composable
fun GlassChip(text: String, modifier: Modifier = Modifier) {
    GlassSurface(modifier) {
        Box(Modifier.height(Dimens.chipHeight).padding(horizontal = 16.dp), contentAlignment = Alignment.Center) {
            Text(text, style = AppType.label, maxLines = 1)
        }
    }
}

@Composable
fun GlassCard(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    GlassSurface(modifier.widthIn(max = 320.dp), CardShape) {
        Column(
            Modifier.padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            content = content,
        )
    }
}

/** An error-tinted notice, e.g. the low-memory warning on Home or a download error. */
@Composable
fun NoticeBanner(message: String, modifier: Modifier = Modifier, icon: ImageVector = Icons.Filled.Warning) {
    Surface(
        modifier.fillMaxWidth(),
        shape = CardShape,
        color = MaterialTheme.colorScheme.error.copy(alpha = 0.14f),
        contentColor = MaterialTheme.colorScheme.onBackground,
    ) {
        Row(
            Modifier.padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Icon(icon, contentDescription = null, tint = MaterialTheme.colorScheme.error)
            Text(message, style = AppType.body)
        }
    }
}

/** Settings and Languages: a leading title (as on iOS) over the page background, with a back arrow. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppTopBar(title: String, onBack: () -> Unit) {
    TopAppBar(
        title = { Text(title) },
        navigationIcon = {
            IconButton(onClick = onBack) {
                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
            }
        },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
    )
}

/** A surface card with an overline heading; rows inside pad themselves (see [ListRow]). */
@Composable
fun SectionCard(title: String, modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Surface(modifier, shape = CardShape, color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.fillMaxWidth().padding(top = 14.dp, bottom = 6.dp)) {
            Text(
                overlineText(title),
                style = AppType.overline,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 4.dp).semantics { heading() },
            )
            content()
        }
    }
}

/** A 52dp row: body title, optional caption, optional trailing content. */
@Composable
fun ListRow(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        modifier.fillMaxWidth().heightIn(min = Dimens.rowMinHeight).padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = AppType.body)
            subtitle?.let { Text(it, style = AppType.caption, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        if (trailing != null) {
            Spacer(Modifier.width(12.dp))
            trailing()
        }
    }
}

/** A selectable [ListRow] with a checkmark on the selected one; used by pickers and the theme choice. */
@Composable
fun CheckRow(title: String, selected: Boolean, onClick: () -> Unit, subtitle: String? = null) {
    ListRow(
        title,
        Modifier.selectable(selected = selected, onClick = onClick, role = Role.RadioButton),
        subtitle = subtitle,
        trailing = {
            if (selected) {
                Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
            } else {
                Spacer(Modifier.size(24.dp))
            }
        },
    )
}

/** One dialog look for every prompt: sheet color, 28dp corners, title/body tokens, text buttons. */
@Composable
fun AppDialog(
    title: String,
    onDismiss: () -> Unit,
    confirm: Pair<String, () -> Unit>,
    dismiss: List<Pair<String, () -> Unit>> = emptyList(),
    text: @Composable () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = text,
        confirmButton = { TextButton(onClick = confirm.second) { Text(confirm.first) } },
        dismissButton = if (dismiss.isEmpty()) {
            null
        } else {
            { Row { dismiss.forEach { (label, onClick) -> TextButton(onClick = onClick) { Text(label) } } } }
        },
        shape = DialogShape,
        containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
        tonalElevation = 0.dp,
    )
}

@Composable
fun AppDialog(
    title: String,
    message: String,
    onDismiss: () -> Unit,
    confirm: Pair<String, () -> Unit>,
    dismiss: List<Pair<String, () -> Unit>> = emptyList(),
) = AppDialog(title, onDismiss, confirm, dismiss) { Text(message) }

private val SmallButtonPadding = PaddingValues(horizontal = 16.dp)

@Composable
fun SmallButton(label: String, modifier: Modifier = Modifier, enabled: Boolean = true, onClick: () -> Unit) {
    Button(
        onClick,
        modifier.height(Dimens.smallButtonHeight),
        enabled = enabled,
        contentPadding = SmallButtonPadding,
    ) { Text(label, style = AppType.label) }
}

@Composable
fun SmallOutlinedButton(
    label: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    icon: ImageVector? = null,
    onClick: () -> Unit,
) {
    OutlinedButton(
        onClick,
        modifier.height(Dimens.smallButtonHeight),
        enabled = enabled,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
        contentPadding = SmallButtonPadding,
    ) {
        if (icon != null) {
            Icon(icon, contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(6.dp))
        }
        Text(label, style = AppType.label, color = MaterialTheme.colorScheme.onBackground)
    }
}

/** Two-option capsule toggle (Translated / Original), the same look as on iOS. */
@Composable
fun SegmentedToggle(options: List<String>, selected: Int, onSelect: (Int) -> Unit, modifier: Modifier = Modifier) {
    GlassSurface(modifier) {
        Row(Modifier.padding(4.dp).selectableGroup()) {
            options.forEachIndexed { i, label ->
                val on = i == selected
                Box(
                    Modifier
                        .clip(CircleShape)
                        .background(if (on) MaterialTheme.colorScheme.primary else Color.Transparent)
                        .selectable(selected = on, onClick = { onSelect(i) }, role = Role.Tab)
                        .widthIn(min = 116.dp)
                        .height(Dimens.smallButtonHeight)
                        .padding(horizontal = 16.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        label,
                        style = AppType.label,
                        color = if (on) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onBackground,
                    )
                }
            }
        }
    }
}

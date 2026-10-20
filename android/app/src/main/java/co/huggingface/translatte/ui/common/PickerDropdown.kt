package co.huggingface.translatte.ui.common

import androidx.compose.animation.core.MutableTransitionState
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.Dimens
import co.huggingface.translatte.ui.theme.overlineText

/**
 * A compact list anchored to the chip that opened it: directly below it, or above when it doesn't fit
 * below and there is more room above, aligned to the chip's leading edge (trailing with [alignEnd]).
 * Its height is capped to that room, so it never covers the chip. Place it next to the chip inside the
 * chip's parent Box; [anchor] is the chip's bounds in its window. Choosing an option closes it.
 */
@Composable
fun <T> PickerDropdown(
    expanded: Boolean,
    onDismiss: () -> Unit,
    title: String,
    options: List<T>,
    selected: T?,
    label: (T) -> String,
    onSelect: (T) -> Unit,
    anchor: Rect,
    alignEnd: Boolean = false,
) {
    val visible = remember { MutableTransitionState(false) }
    visible.targetState = expanded
    if (!visible.currentState && !visible.targetState) return

    val density = LocalDensity.current
    val gap = with(density) { Dimens.dropdownGap.roundToPx() }
    val margin = with(density) { 8.dp.roundToPx() }
    var origin by remember { mutableStateOf(TransformOrigin(0f, 0f)) }
    val view = LocalView.current
    val visibleFrame = remember(anchor) { android.graphics.Rect().also(view::getWindowVisibleDisplayFrame) }
    val frameBottom = if (visibleFrame.height() > 0) visibleFrame.bottom else view.rootView.height
    val roomBelow = (frameBottom - margin - (anchor.bottom + gap)).toInt()
    val roomAbove = (anchor.top - gap - margin - visibleFrame.top).toInt()
    val screenCap = LocalConfiguration.current.screenHeightDp * 0.6f
    val maxHeight = with(density) { minOf(maxOf(roomBelow, roomAbove).coerceAtLeast(0).toDp().value, screenCap).dp }
    val anchorWidth = with(density) { anchor.width.toDp() }
    val position = remember(alignEnd, gap, margin, roomBelow, roomAbove) {
        AnchoredBelow(gap, margin, alignEnd, roomBelow, roomAbove) { above ->
            origin = TransformOrigin(if (alignEnd) 1f else 0f, if (above) 1f else 0f)
        }
    }
    val start = options.indexOf(selected).coerceAtLeast(0)
    val list = rememberLazyListState(initialFirstVisibleItemIndex = (start - 2).coerceAtLeast(0))

    Popup(popupPositionProvider = position, onDismissRequest = onDismiss, properties = PopupProperties(focusable = true)) {
        AnimatedVisibility(
            visibleState = visible,
            enter = scaleIn(tween(120), initialScale = 0.8f, transformOrigin = origin) + fadeIn(tween(90)),
            exit = scaleOut(tween(75), targetScale = 0.9f, transformOrigin = origin) + fadeOut(tween(75)),
        ) {
            Surface(
                Modifier.width(anchorWidth.coerceIn(Dimens.dropdownMinWidth, Dimens.dropdownMaxWidth)).heightIn(max = maxHeight),
                shape = RoundedCornerShape(Dimens.dropdownRadius),
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                shadowElevation = 8.dp,
            ) {
                Column(Modifier.padding(vertical = 8.dp)) {
                    Text(
                        overlineText(title),
                        style = AppType.overline,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp).semantics { heading() },
                    )
                    LazyColumn(Modifier.weight(1f, fill = false).selectableGroup(), state = list) {
                        itemsIndexed(options) { _, option ->
                            DropdownRow(label(option), option == selected) {
                                onSelect(option)
                                onDismiss()
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun DropdownRow(text: String, selected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Dimens.dropdownRowHeight)
            .selectable(selected = selected, onClick = onClick, role = Role.RadioButton)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, style = AppType.body, modifier = Modifier.weight(1f))
        if (selected) {
            Icon(Icons.Filled.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(20.dp))
        } else {
            Spacer(Modifier.size(20.dp))
        }
    }
}

private class AnchoredBelow(
    private val gap: Int,
    private val margin: Int,
    private val alignEnd: Boolean,
    private val roomBelow: Int,
    private val roomAbove: Int,
    private val onPlaced: (above: Boolean) -> Unit,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect,
        windowSize: IntSize,
        layoutDirection: LayoutDirection,
        popupContentSize: IntSize,
    ): IntOffset {
        val end = alignEnd != (layoutDirection == LayoutDirection.Rtl)
        val x = (if (end) anchorBounds.right - popupContentSize.width else anchorBounds.left)
            .coerceIn(margin, (windowSize.width - popupContentSize.width - margin).coerceAtLeast(margin))
        val above = popupContentSize.height > roomBelow && roomAbove > roomBelow
        onPlaced(above)
        val y = if (above) anchorBounds.top - gap - popupContentSize.height else anchorBounds.bottom + gap
        return IntOffset(x, y)
    }
}

package co.huggingface.translatte.ui.result

import android.graphics.Bitmap
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.Phase
import co.huggingface.translatte.ui.common.GlassCard
import co.huggingface.translatte.ui.common.GlassIconButton
import co.huggingface.translatte.ui.common.GlassSurface
import uniffi.translatte_ffi.TranslationResult
import kotlin.math.roundToInt

/** [image] is null only while a new photo is still loading. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ResultScreen(
    image: Bitmap?,
    result: TranslationResult?,
    phase: Phase,
    direction: String,
    hintSeen: Boolean,
    onHintSeen: () -> Unit,
    onRetry: () -> Unit,
    onBack: () -> Unit,
) {
    BackHandler(onBack = onBack)
    val zoom = remember(image) { ZoomState() }
    var showOriginal by rememberSaveable { mutableStateOf(false) }
    var flipped by remember(result) { mutableStateOf(emptySet<Int>()) }
    var hint by remember { mutableStateOf(!hintSeen) }
    fun gestured() {
        if (hint) {
            hint = false
            onHintSeen()
        }
    }

    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        if (image != null) {
            BoxWithConstraints(
                Modifier
                    .fillMaxSize()
                    .testTag("viewer")
                    .pointerInput(image) {
                        detectTransformGestures { centroid, pan, z, _ ->
                            zoom.transform(centroid, pan, z)
                            gestured()
                        }
                    },
            ) {
                val viewport = Size(constraints.maxWidth.toFloat(), constraints.maxHeight.toFloat())
                LaunchedEffect(viewport, image) { zoom.setBounds(viewport, fitRect(image.width, image.height, viewport)) }
                OverlayImage(
                    image,
                    result,
                    showOriginal,
                    Modifier.fillMaxSize().graphicsLayer {
                        scaleX = zoom.scale
                        scaleY = zoom.scale
                        translationX = zoom.offset.x
                        translationY = zoom.offset.y
                        transformOrigin = TransformOrigin(0f, 0f)
                    },
                    flipped = flipped,
                    onToggle = { i ->
                        flipped = if (i in flipped) flipped - i else flipped + i
                        gestured()
                    },
                    onDoubleTap = { local ->
                        zoom.doubleTap(local * zoom.scale + zoom.offset)
                        gestured()
                    },
                )
            }
        }

        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            GlassIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back", onBack)
            GlassSurface {
                Text(
                    direction,
                    Modifier.padding(horizontal = 14.dp, vertical = 10.dp),
                    style = MaterialTheme.typography.labelLarge,
                )
            }
            Spacer(Modifier.weight(1f))
            if (zoom.zoomed) {
                GlassSurface {
                    Text(
                        "${(zoom.scale * 100).roundToInt()}%",
                        Modifier.padding(horizontal = 12.dp, vertical = 10.dp),
                        style = MaterialTheme.typography.labelLarge,
                    )
                }
            }
        }

        when (phase) {
            is Phase.Working -> GlassCard(Modifier.align(Alignment.Center)) {
                Text(phase.label, style = MaterialTheme.typography.titleMedium)
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            is Phase.Failed -> GlassCard(Modifier.align(Alignment.Center)) {
                Icon(Icons.Outlined.ErrorOutline, contentDescription = null, tint = MaterialTheme.colorScheme.error)
                Text(phase.message, textAlign = TextAlign.Center, style = MaterialTheme.typography.bodyMedium)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = onBack) { Text("Back") }
                    if (phase.retryable) Button(onClick = onRetry) { Text("Retry") }
                }
            }
            else -> {}
        }

        if (phase is Phase.Done) {
            Column(
                Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                GlassSurface {
                    SingleChoiceSegmentedButtonRow(Modifier.padding(4.dp)) {
                        SegmentedButton(
                            selected = !showOriginal,
                            onClick = { showOriginal = false },
                            shape = SegmentedButtonDefaults.itemShape(0, 2),
                        ) { Text("Translated") }
                        SegmentedButton(
                            selected = showOriginal,
                            onClick = { showOriginal = true },
                            shape = SegmentedButtonDefaults.itemShape(1, 2),
                        ) { Text("Original") }
                    }
                }
                if (hint) {
                    GlassSurface {
                        Text(
                            "Pinch to zoom · tap a box to see the original",
                            Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                            style = MaterialTheme.typography.labelMedium,
                        )
                    }
                }
            }
        }
    }
}

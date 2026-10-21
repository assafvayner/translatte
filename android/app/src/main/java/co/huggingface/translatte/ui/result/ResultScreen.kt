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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
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
import co.huggingface.translatte.ui.common.GlassChip
import co.huggingface.translatte.ui.common.SegmentedToggle
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.common.GlassIconButton
import co.huggingface.translatte.ui.common.GlassSurface
import co.huggingface.translatte.ui.theme.LocalGlass
import uniffi.translatte_ffi.TranslationResult
import kotlin.math.roundToInt

/** [image] is null only while a new photo is still loading. */
@Composable
fun ResultScreen(
    image: Bitmap?,
    result: TranslationResult?,
    phase: Phase,
    direction: String,
    hintSeen: Boolean,
    onHintSeen: () -> Unit,
    onBack: () -> Unit,
) {
    BackHandler(onBack = onBack)
    val zoom = remember(image) { ZoomState() }
    var showOriginal by rememberSaveable { mutableStateOf(false) }
    var flipped by remember(result) { mutableStateOf(emptySet<Int>()) }
    var hint by remember { mutableStateOf(!hintSeen) }
    val currentPhase by rememberUpdatedState(phase)
    fun gestured() {
        if (hint && currentPhase is Phase.Done) {
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

        Box(Modifier.fillMaxWidth().windowInsetsTopHeight(WindowInsets.statusBars).background(LocalGlass.current))
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing).padding(horizontal = 16.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            GlassIconButton(Icons.AutoMirrored.Filled.ArrowBack, "Back", onBack)
            GlassChip(direction, Modifier.weight(1f, fill = false))
            Spacer(Modifier.weight(1f))
            if (zoom.zoomed) {
                GlassSurface {
                    Box(Modifier.height(32.dp).padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                        Text("${(zoom.scale * 100).roundToInt()}%", style = AppType.caption)
                    }
                }
            }
        }

        when (phase) {
            is Phase.Working -> GlassCard(Modifier.align(Alignment.Center)) {
                Text(phase.label, style = AppType.headline)
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            is Phase.Failed -> GlassCard(Modifier.align(Alignment.Center)) {
                Icon(Icons.Outlined.ErrorOutline, contentDescription = null, tint = MaterialTheme.colorScheme.error)
                Text(phase.message, textAlign = TextAlign.Center, style = AppType.body)
                TextButton(onClick = onBack) { Text("Back") }
            }
            else -> {}
        }

        if (phase is Phase.Done) {
            Column(
                Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                SegmentedToggle(
                    listOf("Translated", "Original"),
                    selected = if (showOriginal) 1 else 0,
                    onSelect = { showOriginal = it == 1 },
                )
                if (hint) {
                    GlassSurface {
                        Text(
                            "Pinch to zoom · tap a box to see the original",
                            Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                            style = AppType.caption,
                        )
                    }
                }
            }
        }
    }
}

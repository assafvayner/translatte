package co.huggingface.translatte.ui.camera

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import co.huggingface.translatte.ui.theme.AppType
import co.huggingface.translatte.ui.theme.InnerShape
import co.huggingface.translatte.ui.result.RegionBox
import uniffi.translatte_ffi.Point

/**
 * Paints live regions over the camera preview in the result viewer's style, placed with
 * [LiveRules.fill] from the upright frame size [imageSize]. Each update slides kept boxes from
 * their previous position over [LiveRules.MOVE_MS].
 */
@Composable
fun LiveOverlay(translator: LiveTranslator, modifier: Modifier = Modifier) {
    val tracked = translator.overlay
    val imageSize = translator.imageSize
    val cache = remember { HashMap<Long, RegionBox>() }
    val boxes = remember(tracked) {
        val next = tracked.map { RegionBox(it.region, cache[it.id]) to center(it.from) }
        cache.clear()
        tracked.forEachIndexed { i, t -> cache[t.id] = next[i].first }
        next
    }
    val progress = remember { Animatable(1f) }
    LaunchedEffect(tracked) {
        progress.snapTo(0f)
        progress.animateTo(1f, tween(LiveRules.MOVE_MS))
    }
    Canvas(modifier) {
        if (imageSize == IntSize.Zero) return@Canvas
        val fill = LiveRules.fill(imageSize.width.toFloat(), imageSize.height.toFloat(), size.width, size.height)
        val rest = 1f - progress.value
        drawIntoCanvas { c ->
            boxes.forEach { (box, from) ->
                box.draw(c.nativeCanvas, fill.scale, fill.left, fill.top, (from.x - box.cx) * rest, (from.y - box.cy) * rest)
            }
        }
    }
}

/** The debug readout of [translator]'s latest update, read here so the rest of the screen doesn't recompose with it. */
@Composable
fun LiveReadout(translator: LiveTranslator, modifier: Modifier = Modifier) {
    translator.readout?.let {
        Surface(shape = InnerShape, color = Scrim, contentColor = OnScrim, modifier = modifier) {
            Text(it, Modifier.padding(horizontal = 8.dp, vertical = 4.dp), style = AppType.caption)
        }
    }
}

private fun center(corners: List<Point>) =
    Offset(corners.map { it.x }.average().toFloat(), corners.map { it.y }.average().toFloat())

package co.huggingface.translatte.ui.result

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import kotlin.math.max
import kotlin.math.min

/** Where an image of the given size sits when fitted, centered, inside [viewport]. */
fun fitRect(imageWidth: Int, imageHeight: Int, viewport: Size): Rect {
    val s = min(viewport.width / max(imageWidth, 1), viewport.height / max(imageHeight, 1))
    val w = imageWidth * s
    val h = imageHeight * s
    val left = (viewport.width - w) / 2
    val top = (viewport.height - h) / 2
    return Rect(left, top, left + w, top + h)
}

/**
 * Pinch, pan and double-tap state for a layer drawn as `content * scale + offset` with a top-left
 * transform origin. Pan is clamped to the photo's edges; an axis smaller than the viewport stays
 * centered.
 */
@Stable
class ZoomState {
    var scale by mutableFloatStateOf(1f)
        private set
    var offset by mutableStateOf(Offset.Zero)
        private set

    private var viewport = Size.Zero
    private var content = Rect.Zero

    val zoomed: Boolean get() = scale > 1.01f

    fun setBounds(viewport: Size, content: Rect) {
        this.viewport = viewport
        this.content = content
        offset = clamp(offset, scale)
    }

    fun transform(centroid: Offset, pan: Offset, zoom: Float) {
        val s = (scale * zoom).coerceIn(MIN_SCALE, MAX_SCALE)
        offset = clamp(centroid - (centroid - offset) * (s / scale) + pan, s)
        scale = s
    }

    fun doubleTap(at: Offset) {
        if (zoomed) {
            scale = 1f
            offset = Offset.Zero
        } else {
            offset = clamp(at - (at - offset) * (DOUBLE_TAP_SCALE / scale), DOUBLE_TAP_SCALE)
            scale = DOUBLE_TAP_SCALE
        }
    }

    private fun clamp(o: Offset, s: Float): Offset {
        fun axis(v: Float, view: Float, start: Float, end: Float): Float {
            val length = (end - start) * s
            return if (length <= view) (view - length) / 2 - start * s else v.coerceIn(view - end * s, 0f - start * s)
        }
        return Offset(
            axis(o.x, viewport.width, content.left, content.right),
            axis(o.y, viewport.height, content.top, content.bottom),
        )
    }

    companion object {
        const val MIN_SCALE = 1f
        const val MAX_SCALE = 6f
        const val DOUBLE_TAP_SCALE = 2.5f
    }
}

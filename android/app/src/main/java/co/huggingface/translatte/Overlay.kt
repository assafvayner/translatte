package co.huggingface.translatte

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.toSize
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb
import uniffi.translatte_ffi.TranslationResult
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** Draws the photo with each translated region painted over its source text. */
@Composable
fun OverlayImage(
    bitmap: Bitmap,
    result: TranslationResult?,
    showOriginal: Boolean,
    onTap: (Region) -> Unit,
    modifier: Modifier = Modifier,
) {
    val image = remember(bitmap) { bitmap.asImageBitmap() }
    val regions = remember(result) { result?.regions.orEmpty().filter { it.translatedText != null }.map(::Box) }
    Canvas(
        modifier.pointerInput(regions) {
            detectTapGestures { tap ->
                val fit = Fit(bitmap, size.toSize())
                val p = Offset((tap.x - fit.left) / fit.scale, (tap.y - fit.top) / fit.scale)
                regions.lastOrNull { it.contains(p) }?.let { onTap(it.region) }
            }
        },
    ) {
        val fit = Fit(bitmap, size)
        drawImage(
            image,
            dstOffset = IntOffset(fit.left.toInt(), fit.top.toInt()),
            dstSize = IntSize((bitmap.width * fit.scale).toInt(), (bitmap.height * fit.scale).toInt()),
        )
        if (!showOriginal) {
            drawIntoCanvas { c -> regions.forEach { it.draw(c.nativeCanvas, fit) } }
        }
    }
}

private class Fit(bitmap: Bitmap, container: Size) {
    val scale = min(container.width / max(bitmap.width, 1), container.height / max(bitmap.height, 1))
    val left = (container.width - bitmap.width * scale) / 2
    val top = (container.height - bitmap.height * scale) / 2
}

/** A region as a rotated rectangle in image pixels. */
private class Box(val region: Region) {
    private val c = region.corners
    val width = max(dist(0, 1), dist(3, 2))
    val height = max(dist(0, 3), dist(1, 2))
    val cx = c.sumOf { it.x.toDouble() }.toFloat() / c.size
    val cy = c.sumOf { it.y.toDouble() }.toFloat() / c.size

    private val background = Paint().apply { color = region.background.argb() }

    /** The fitted text for the last on-screen box size; refitting only happens on resize. */
    private var layout: Pair<Pair<Float, Float>, StaticLayout>? = null

    private fun dist(a: Int, b: Int) = hypot(c[a].x - c[b].x, c[a].y - c[b].y)

    private fun layoutFor(w: Float, h: Float): StaticLayout {
        layout?.let { (size, l) -> if (size == (w to h)) return l }
        return fitText(region.translatedText.orEmpty(), region.foreground.argb(), w, h).also { layout = (w to h) to it }
    }

    fun contains(p: Offset): Boolean {
        val dx = p.x - cx
        val dy = p.y - cy
        val a = -region.angle
        val x = dx * cos(a) - dy * sin(a)
        val y = dx * sin(a) + dy * cos(a)
        return kotlin.math.abs(x) <= width / 2 && kotlin.math.abs(y) <= height / 2
    }

    fun draw(canvas: Canvas, fit: Fit) {
        val w = width * fit.scale
        val h = height * fit.scale
        val pad = max(1f, h * 0.05f)
        canvas.save()
        canvas.translate(fit.left + cx * fit.scale, fit.top + cy * fit.scale)
        canvas.rotate(Math.toDegrees(region.angle.toDouble()).toFloat())
        canvas.drawRect(-w / 2, -h / 2, w / 2, h / 2, background)
        val layout = layoutFor(w - 2 * pad, h - 2 * pad)
        canvas.translate(-w / 2 + pad, -layout.height / 2f)
        layout.draw(canvas)
        canvas.restore()
    }
}

private fun Rgb.argb() =
    android.graphics.Color.rgb(r.toInt(), g.toInt(), b.toInt())

/**
 * Lays [text] out at the largest size that fits [width] x [height], without breaking words when
 * the text has spaces to wrap at.
 */
private fun fitText(text: String, color: Int, width: Float, height: Float): StaticLayout {
    val paint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color }
    val w = max(width, 1f).toInt()
    val words = if (text.contains(' ')) text.split(' ') else emptyList()
    fun layout(size: Float): StaticLayout {
        paint.textSize = size
        return StaticLayout.Builder.obtain(text, 0, text.length, paint, w)
            .setAlignment(Layout.Alignment.ALIGN_NORMAL)
            .setIncludePad(false)
            .build()
    }
    fun fits(size: Float): Boolean {
        val l = layout(size)
        return l.height <= height && words.all { paint.measureText(it) <= w }
    }
    var lo = 4f
    var hi = max(height, lo)
    repeat(10) {
        val mid = (lo + hi) / 2
        if (fits(mid)) lo = mid else hi = mid
    }
    return layout(lo)
}

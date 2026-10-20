package co.huggingface.translatte.ui.result

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb
import uniffi.translatte_ffi.TranslationResult
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.roundToInt

/**
 * Draws the photo with each translated region painted over its source text. Regions whose index is
 * in [flipped] are left unpainted, showing the original. With [onToggle], each region is also a
 * tappable, accessible node: its label is the translation and its state the original text.
 */
@Composable
fun OverlayImage(
    bitmap: Bitmap,
    result: TranslationResult?,
    showOriginal: Boolean,
    modifier: Modifier = Modifier,
    flipped: Set<Int> = emptySet(),
    onToggle: ((Int) -> Unit)? = null,
) {
    val image = remember(bitmap) { bitmap.asImageBitmap() }
    val boxes = remember(result) { result?.regions.orEmpty().filter { it.translatedText != null }.map(::RegionBox) }
    BoxWithConstraints(modifier) {
        Canvas(Modifier.fillMaxSize()) {
            val fit = Fit(bitmap, size)
            drawImage(
                image,
                dstOffset = IntOffset(fit.left.toInt(), fit.top.toInt()),
                dstSize = IntSize((bitmap.width * fit.scale).toInt(), (bitmap.height * fit.scale).toInt()),
            )
            if (!showOriginal) {
                drawIntoCanvas { c ->
                    boxes.forEachIndexed { i, box -> if (i !in flipped) box.draw(c.nativeCanvas, fit) }
                }
            }
        }
        if (onToggle != null && !showOriginal) {
            val fit = Fit(bitmap, Size(constraints.maxWidth.toFloat(), constraints.maxHeight.toFloat()))
            val density = LocalDensity.current
            boxes.forEachIndexed { i, box ->
                val w = box.width * fit.scale
                val h = box.height * fit.scale
                Box(
                    Modifier
                        .offset {
                            IntOffset(
                                (fit.left + box.cx * fit.scale - w / 2).roundToInt(),
                                (fit.top + box.cy * fit.scale - h / 2).roundToInt(),
                            )
                        }
                        .size(with(density) { w.toDp() }, with(density) { h.toDp() })
                        .rotate(Math.toDegrees(box.region.angle.toDouble()).toFloat())
                        .semantics {
                            contentDescription = box.region.translatedText.orEmpty()
                            stateDescription = box.region.sourceText
                        }
                        .clickable(
                            interactionSource = null,
                            indication = null,
                            onClickLabel = if (i in flipped) "Show translation" else "Show original",
                        ) { onToggle(i) },
                )
            }
        }
    }
}

private class Fit(bitmap: Bitmap, container: Size) {
    private val rect = fitRect(bitmap.width, bitmap.height, container)
    val scale = rect.width / max(bitmap.width, 1)
    val left = rect.left
    val top = rect.top
}

/** A region as a rotated rectangle in image pixels. */
private class RegionBox(val region: Region) {
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

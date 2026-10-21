package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.StageTimings
import uniffi.translatte_ffi.TranslationResult
import java.nio.ByteBuffer
import java.util.Locale
import kotlin.math.max

/** Places an upright frame of image pixels over the preview: view = image * scale + (left, top). */
data class FillTransform(val scale: Float, val left: Float, val top: Float)

/** Live translation rules, the same as iOS's `LiveRules`; keep the two in step. */
object LiveRules {
    /** Frames reach the engine at most this large on their long side. */
    const val MAX_FRAME_SIDE = 1280

    /** Minimum time between frame starts while the device is hot. */
    const val HOT_INTERVAL_MS = 1000L

    /** How long a tracked box takes to slide to its new place. */
    const val MOVE_MS = 150

    fun isActive(enabled: Boolean, sourceInstalled: Boolean): Boolean = enabled && sourceInstalled

    /** Start a frame when none is in flight; on a hot device, also wait a second after the last start. */
    fun shouldTake(busy: Boolean, hot: Boolean, nowMs: Long, lastStartMs: Long?): Boolean =
        !busy && (!hot || lastStartMs == null || nowMs - lastStartMs >= HOT_INTERVAL_MS)

    /** The pixel step that brings the long side within [MAX_FRAME_SIDE]. */
    fun downscaleFactor(width: Int, height: Int): Int =
        max(1, (max(width, height) + MAX_FRAME_SIDE - 1) / MAX_FRAME_SIDE)

    /** Snap freezes the live result only when it translated something; otherwise the photo pipeline runs. */
    fun snapUsesLive(result: TranslationResult?): Boolean =
        result != null && result.translationError == null && result.regions.any { it.translatedText != null }

    /** Debug readout, e.g. "2.5/s · det 120 · rec 85 · tr 40 ms"; [intervalMs] is the time since the previous update. */
    fun readout(intervalMs: Long?, timings: StageTimings): String {
        val rate = if (intervalMs == null || intervalMs <= 0) "–" else String.format(Locale.US, "%.1f", 1000.0 / intervalMs)
        return "$rate/s · det ${timings.detectMs} · rec ${timings.recognizeMs} · tr ${timings.translateMs} ms"
    }

    /** Scales an image to cover the view and centers it, cropping the overflow, as the camera preview does. */
    fun fill(imageWidth: Float, imageHeight: Float, viewWidth: Float, viewHeight: Float): FillTransform {
        val scale = max(viewWidth / max(imageWidth, 1f), viewHeight / max(imageHeight, 1f))
        return FillTransform(scale, (viewWidth - imageWidth * scale) / 2, (viewHeight - imageHeight * scale) / 2)
    }

    /**
     * Copies a 4-byte-per-pixel frame whose rows are [rowStride] bytes apart into tightly packed
     * RGBA, keeping every [factor]th pixel of every [factor]th row (output: width / factor by
     * height / factor). [swapRedBlue] reads BGRA.
     */
    fun packRgba(src: ByteBuffer, width: Int, height: Int, rowStride: Int, factor: Int, swapRedBlue: Boolean): ByteArray {
        val w = width / factor
        val h = height / factor
        val out = ByteArray(w * h * 4)
        if (factor == 1 && !swapRedBlue && rowStride == width * 4) {
            src.duplicate().apply { rewind() }.get(out)
            return out
        }
        val r = if (swapRedBlue) 2 else 0
        val b = 2 - r
        var o = 0
        for (y in 0 until h) {
            val row = y * factor * rowStride
            for (x in 0 until w) {
                val i = row + x * factor * 4
                out[o] = src.get(i + r)
                out[o + 1] = src.get(i + 1)
                out[o + 2] = src.get(i + b)
                out[o + 3] = src.get(i + 3)
                o += 4
            }
        }
        return out
    }
}

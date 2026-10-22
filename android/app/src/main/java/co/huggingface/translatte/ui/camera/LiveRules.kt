package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.StageTimings
import uniffi.translatte_ffi.TranslationResult
import java.nio.ByteBuffer
import java.util.Locale
import kotlin.math.max
import kotlin.math.min

/** Places an upright frame of image pixels over the preview: view = image * scale + (left, top). */
data class FillTransform(val scale: Float, val left: Float, val top: Float)

/**
 * The two stages of live translation, the same as iOS's `LiveStages`: reading a frame (OCR) and
 * translating what was read, each with at most one frame. A frame read while the translator is
 * busy waits for it; no new frame is read while one waits, so none goes stale. Not thread-safe.
 */
class LiveStages<T> {
    var reading = false
        private set
    var translating = false
        private set
    private var waiting: T? = null

    /** Whether a new frame can't be read yet (pass as `busy` to [LiveRules.shouldTake]). */
    val readBusy: Boolean get() = reading || waiting != null

    fun startRead() {
        reading = true
    }

    /** A frame was read: returns it to translate now, or null when it waits for the translator. */
    fun readDone(item: T): T? {
        reading = false
        if (translating) {
            waiting = item
            return null
        }
        translating = true
        return item
    }

    fun readFailed() {
        reading = false
    }

    /** A translation finished or failed: returns the waiting frame to translate next, if any. */
    fun translateDone(): T? {
        val next = waiting
        waiting = null
        translating = next != null
        return next
    }

    /** Drops the waiting frame; frames in a stage finish and are ignored by their caller. */
    fun reset() {
        waiting = null
    }
}

/** Live translation rules, the same as iOS's `LiveRules`; keep the two in step. */
object LiveRules {
    /** Frames reach the engine at most this large on their long side. */
    const val MAX_FRAME_SIDE = 1280

    /** Minimum time between frame starts while the device is hot. */
    const val HOT_INTERVAL_MS = 1000L

    /** Wait after the first failed frame; doubles with each further failure in a row. */
    const val BACKOFF_BASE_MS = 1000L

    /** The longest wait after failed frames. */
    const val BACKOFF_MAX_MS = 8000L

    /** How long a tracked box takes to slide to its new place. */
    const val MOVE_MS = 150

    /** How often stale boxes are pruned while updates stall. */
    const val PRUNE_INTERVAL_MS = 100L

    fun isActive(enabled: Boolean, sourceInstalled: Boolean): Boolean = enabled && sourceInstalled

    /** The wait after [failures] frames failed in a row: none for 0, then [BACKOFF_BASE_MS] doubling up to [BACKOFF_MAX_MS]. */
    fun backoffMs(failures: Int): Long =
        if (failures <= 0) 0L else min(BACKOFF_MAX_MS, BACKOFF_BASE_MS shl min(failures - 1, 16))

    /**
     * Start a frame when none is in flight and the [backoffMs] for [failures] has passed since
     * [lastFailMs]; on a hot device, also wait a second after the last start.
     */
    fun shouldTake(
        busy: Boolean,
        hot: Boolean,
        nowMs: Long,
        lastStartMs: Long?,
        failures: Int = 0,
        lastFailMs: Long? = null,
    ): Boolean =
        !busy &&
            (lastFailMs == null || nowMs - lastFailMs >= backoffMs(failures)) &&
            (!hot || lastStartMs == null || nowMs - lastStartMs >= HOT_INTERVAL_MS)

    /** The pixel step that brings the long side within [MAX_FRAME_SIDE]. */
    fun downscaleFactor(width: Int, height: Int): Int =
        max(1, (max(width, height) + MAX_FRAME_SIDE - 1) / MAX_FRAME_SIDE)

    /** Snap freezes the live result only when it translated something; otherwise the photo pipeline runs. */
    fun snapUsesLive(result: TranslationResult?): Boolean =
        result != null && result.translationError == null && result.regions.any { it.translatedText != null }

    /** Debug readout, e.g. "ort · 2.5/s · det 120 · rec 85 · tr 40 ms"; [intervalMs] is the time since the previous update. */
    fun readout(intervalMs: Long?, timings: StageTimings): String {
        val rate = if (intervalMs == null || intervalMs <= 0) "–" else String.format(Locale.US, "%.1f", 1000.0 / intervalMs)
        return "${timings.ocrEngine} · $rate/s · det ${timings.detectMs} · rec ${timings.recognizeMs} · tr ${timings.translateMs} ms"
    }

    /** Scales an image to cover the view and centers it, cropping the overflow, as the camera preview does. */
    fun fill(imageWidth: Float, imageHeight: Float, viewWidth: Float, viewHeight: Float): FillTransform {
        val scale = max(viewWidth / max(imageWidth, 1f), viewHeight / max(imageHeight, 1f))
        return FillTransform(scale, (viewWidth - imageWidth * scale) / 2, (viewHeight - imageHeight * scale) / 2)
    }

    /**
     * Copies a [width] x [height] block, starting at pixel ([left], [top]), of a 4-byte-per-pixel
     * frame whose rows are [rowStride] bytes apart into tightly packed RGBA, keeping every
     * [factor]th pixel of every [factor]th row (output: width / factor by height / factor).
     * [swapRedBlue] reads BGRA. Writes into [reuse] when it is exactly the output size.
     */
    fun packRgba(
        src: ByteBuffer,
        width: Int,
        height: Int,
        rowStride: Int,
        factor: Int,
        swapRedBlue: Boolean,
        left: Int = 0,
        top: Int = 0,
        reuse: ByteArray? = null,
    ): ByteArray {
        val w = width / factor
        val h = height / factor
        val out = reuse?.takeIf { it.size == w * h * 4 } ?: ByteArray(w * h * 4)
        if (factor == 1 && !swapRedBlue && left == 0 && top == 0 && rowStride == width * 4) {
            src.duplicate().apply { rewind() }.get(out)
            return out
        }
        val r = if (swapRedBlue) 2 else 0
        val b = 2 - r
        var o = 0
        for (y in 0 until h) {
            val row = (top + y * factor) * rowStride + left * 4
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

package co.huggingface.translatte.ui.camera

import android.graphics.Bitmap
import android.os.SystemClock
import android.util.Log
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.IntSize
import co.huggingface.translatte.BuildConfig
import co.huggingface.translatte.Images
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.translatte_ffi.TranslationResult

private const val TAG = "translatte"

/**
 * One camera frame as the engine takes it: tightly packed RGBA rows, before rotating upright.
 * [ptsMs] is a replayed video frame's position.
 */
class LiveFrame(val rgba: ByteArray, val width: Int, val height: Int, val rotation: Int, val ptsMs: Long? = null)

/** A frame [LiveTranslator] read, waiting to be translated; [generation] drops it after a reset. */
private class ReadFrame(val frame: LiveFrame, val result: TranslationResult, val generation: Int)

/**
 * Live translation of camera frames, mirroring iOS's `LiveTranslator`: two [LiveStages], so one
 * frame is translated while the next is read, paced by [LiveRules], with results steadied by
 * [LiveOverlayTracker]. [scope] is a main-thread scope that publishes results; [read] runs OCR on
 * a frame and [translate] translates what it read; [hot] reports a serious thermal state.
 */
class LiveTranslator(
    private val scope: CoroutineScope,
    private val read: suspend (LiveFrame) -> TranslationResult,
    private val translate: suspend (TranslationResult) -> TranslationResult,
    private val hot: () -> Boolean,
) : ImageAnalysis.Analyzer {
    var overlay by mutableStateOf<List<TrackedRegion>>(emptyList())
        private set
    var imageSize by mutableStateOf(IntSize.Zero)
        private set

    /** Debug builds: updates/s and stage times of the latest update. */
    var readout by mutableStateOf<String?>(null)
        private set

    @Volatile
    var enabled = false

    private val lock = Any()
    private val stages = LiveStages<ReadFrame>()
    private var lastStartMs: Long? = null
    private val tracker = LiveOverlayTracker()
    @Volatile
    private var latest: Pair<LiveFrame, TranslationResult>? = null
    private var failures = 0
    private var lastFailMs: Long? = null
    /** Frame buffers, reused once neither [latest] nor the frame being translated holds them. */
    private val buffers = arrayOfNulls<ByteArray>(3)
    @Volatile
    private var translating: LiveFrame? = null
    private var lastDoneMs: Long? = null
    private var generation = 0
    private var pruner: Job? = null
    private val events = if (BuildConfig.DEBUG) LiveEventLog { Log.i(TAG, "liveev $it") } else null

    /** Clears the overlay and drops results still in flight (source change, live turned off). Main thread. */
    fun reset() {
        generation++
        pruner?.cancel()
        pruner = null
        synchronized(lock) {
            failures = 0
            stages.reset()
        }
        events?.reset(SystemClock.elapsedRealtime(), overlay)
        tracker.reset()
        overlay = emptyList()
        latest = null
        lastDoneMs = null
        readout = null
    }

    /** The latest live frame, upright, and its result when snap should show them rather than take a photo. */
    suspend fun snapshot(): Pair<Bitmap, TranslationResult>? {
        val (frame, result) = latest ?: return null
        if (!LiveRules.snapUsesLive(result)) return null
        val bitmap = withContext(Dispatchers.Default) { Images.fromRgba(frame.rgba, frame.width, frame.height, frame.rotation) }
        return bitmap to result
    }

    /** Runs on the analysis executor for every frame; converts and translates only those [LiveRules] lets through. */
    override fun analyze(image: ImageProxy) {
        image.use {
            if (!enabled || !take()) return
            val plane = it.planes[0]
            val crop = it.cropRect
            val factor = LiveRules.downscaleFactor(crop.width(), crop.height())
            val held = listOf(latest?.first?.rgba, translating?.rgba)
            val free = buffers.indices.first { i -> held.none { it != null && it === buffers[i] } }
            val rgba = LiveRules.packRgba(
                plane.buffer, crop.width(), crop.height(), plane.rowStride, factor, swapRedBlue = false,
                left = crop.left, top = crop.top, reuse = buffers[free],
            )
            buffers[free] = rgba
            val frame = LiveFrame(rgba, crop.width() / factor, crop.height() / factor, it.imageInfo.rotationDegrees)
            scope.launch { run(frame) }
        }
    }

    /** A replayed video frame: [grab] makes the frame only when one is let through. Main thread. */
    fun offer(grab: () -> LiveFrame?) {
        if (!enabled || !take()) return
        val frame = grab() ?: return synchronized(lock) { stages.readFailed() }
        scope.launch { run(frame) }
    }

    /** Debug builds: event times count from the start of a replayed video, now at [positionMs]. */
    fun markStart(positionMs: Long) {
        events?.start(SystemClock.elapsedRealtime() - positionMs)
    }

    private fun take(): Boolean = synchronized(lock) {
        val now = SystemClock.elapsedRealtime()
        if (!LiveRules.shouldTake(stages.readBusy, hot(), now, lastStartMs, failures, lastFailMs)) {
            false
        } else {
            stages.startRead()
            lastStartMs = now
            true
        }
    }

    /** Reads [frame], then translates it now or once the translation in flight is done. */
    private suspend fun run(frame: LiveFrame) {
        val gen = generation
        val result = try {
            read(frame)
        } catch (e: CancellationException) {
            synchronized(lock) { stages.readFailed() }
            throw e
        } catch (e: Exception) {
            fail(e)
            synchronized(lock) { stages.readFailed() }
            return
        }
        val next = synchronized(lock) {
            stages.readDone(ReadFrame(frame, result, gen))?.also { translating = it.frame }
        } ?: return
        translateFrom(next)
    }

    /** Translates [first], then each frame read meanwhile, until none waits; skips frames read before a reset. */
    private suspend fun translateFrom(first: ReadFrame) {
        var item: ReadFrame? = first
        while (item != null) {
            if (item.generation == generation) {
                try {
                    val result = translate(item.result)
                    synchronized(lock) { failures = 0 }
                    if (item.generation == generation) publish(item.frame, result)
                } catch (e: CancellationException) {
                    synchronized(lock) {
                        stages.translateDone()
                        translating = null
                    }
                    throw e
                } catch (e: Exception) {
                    fail(e)
                }
            }
            item = synchronized(lock) { stages.translateDone().also { translating = it?.frame } }
        }
    }

    private fun fail(e: Exception) {
        Log.w(TAG, "live frame failed", e)
        synchronized(lock) {
            failures++
            lastFailMs = SystemClock.elapsedRealtime()
        }
    }

    private fun publish(frame: LiveFrame, result: TranslationResult) {
        val now = SystemClock.elapsedRealtime()
        val prev = overlay
        overlay = tracker.update(result.regions, now)
        if (overlay.isNotEmpty() && pruner == null) pruner = scope.launch { prune() }
        imageSize = IntSize(result.imageWidth.toInt(), result.imageHeight.toInt())
        latest = frame to result
        if (BuildConfig.DEBUG) {
            val line = LiveRules.readout(lastDoneMs?.let { now - it }, result.timings)
            readout = line
            Log.i(TAG, "live $line regions ${result.regions.size}")
            events?.update(now, prev, overlay, result, lastDoneMs?.let { now - it }, frame.ptsMs)
        }
        lastDoneMs = now
    }

    private suspend fun prune() {
        while (overlay.isNotEmpty()) {
            delay(LiveRules.PRUNE_INTERVAL_MS)
            val now = SystemClock.elapsedRealtime()
            val kept = tracker.prune(now)
            if (kept.size != overlay.size) {
                events?.prune(now, overlay, kept)
                overlay = kept
            }
        }
        pruner = null
    }
}

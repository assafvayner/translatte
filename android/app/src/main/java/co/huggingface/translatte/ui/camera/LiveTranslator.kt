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

/** One camera frame as the engine takes it: tightly packed RGBA rows, before rotating upright. */
class LiveFrame(val rgba: ByteArray, val width: Int, val height: Int, val rotation: Int)

/**
 * Live translation of camera frames, mirroring iOS's `LiveTranslator`: at most one frame in
 * flight (the newest frame after it finishes is next), paced by [LiveRules], with results
 * steadied by [LiveOverlayTracker]. [scope] is a main-thread scope that publishes results;
 * [translate] runs a frame through the engine; [hot] reports a serious thermal state.
 */
class LiveTranslator(
    private val scope: CoroutineScope,
    private val translate: suspend (LiveFrame) -> TranslationResult,
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
    private var busy = false
    private var lastStartMs: Long? = null
    private val tracker = LiveOverlayTracker()
    @Volatile
    private var latest: Pair<LiveFrame, TranslationResult>? = null
    private var failures = 0
    private var lastFailMs: Long? = null
    private val buffers = arrayOfNulls<ByteArray>(2)
    private var lastDoneMs: Long? = null
    private var generation = 0
    private var pruner: Job? = null

    /** Clears the overlay and drops results still in flight (source change, live turned off). Main thread. */
    fun reset() {
        generation++
        pruner?.cancel()
        pruner = null
        synchronized(lock) { failures = 0 }
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
            val free = if (latest?.first?.rgba === buffers[0]) 1 else 0
            val rgba = LiveRules.packRgba(
                plane.buffer, crop.width(), crop.height(), plane.rowStride, factor, swapRedBlue = false,
                left = crop.left, top = crop.top, reuse = buffers[free],
            )
            buffers[free] = rgba
            val frame = LiveFrame(rgba, crop.width() / factor, crop.height() / factor, it.imageInfo.rotationDegrees)
            scope.launch { run(frame) }
        }
    }

    private fun take(): Boolean = synchronized(lock) {
        val now = SystemClock.elapsedRealtime()
        if (busy || !LiveRules.shouldTake(busy, hot(), now, lastStartMs, failures, lastFailMs)) {
            false
        } else {
            busy = true
            lastStartMs = now
            true
        }
    }

    private suspend fun run(frame: LiveFrame) {
        val gen = generation
        try {
            val result = translate(frame)
            synchronized(lock) { failures = 0 }
            if (gen == generation) publish(frame, result)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.w(TAG, "live frame failed", e)
            synchronized(lock) {
                failures++
                lastFailMs = SystemClock.elapsedRealtime()
            }
        } finally {
            synchronized(lock) { busy = false }
        }
    }

    private fun publish(frame: LiveFrame, result: TranslationResult) {
        val now = SystemClock.elapsedRealtime()
        overlay = tracker.update(result.regions, now)
        if (overlay.isNotEmpty() && pruner == null) pruner = scope.launch { prune() }
        imageSize = IntSize(result.imageWidth.toInt(), result.imageHeight.toInt())
        latest = frame to result
        if (BuildConfig.DEBUG) {
            val line = LiveRules.readout(lastDoneMs?.let { now - it }, result.timings)
            readout = line
            Log.i(TAG, "live $line regions ${result.regions.size}")
        }
        lastDoneMs = now
    }

    private suspend fun prune() {
        while (overlay.isNotEmpty()) {
            delay(LiveRules.PRUNE_INTERVAL_MS)
            val kept = tracker.prune(SystemClock.elapsedRealtime())
            if (kept.size != overlay.size) overlay = kept
        }
        pruner = null
    }
}

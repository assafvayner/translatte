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
import kotlinx.coroutines.launch
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
    private var latest: Pair<LiveFrame, TranslationResult>? = null
    private var lastDoneMs: Long? = null
    private var generation = 0

    /** Clears the overlay and drops results still in flight (source change, live turned off). Main thread. */
    fun reset() {
        generation++
        tracker.reset()
        overlay = emptyList()
        latest = null
        lastDoneMs = null
        readout = null
    }

    /** The latest live frame, upright, and its result when snap should show them rather than take a photo. */
    fun snapshot(): Pair<Bitmap, TranslationResult>? {
        val (frame, result) = latest ?: return null
        if (!LiveRules.snapUsesLive(result)) return null
        return Images.fromRgba(frame.rgba, frame.width, frame.height, frame.rotation) to result
    }

    /** Runs on the analysis executor for every frame; converts and translates only those [LiveRules] lets through. */
    override fun analyze(image: ImageProxy) {
        image.use {
            if (!enabled || !take()) return
            val plane = it.planes[0]
            val factor = LiveRules.downscaleFactor(it.width, it.height)
            val frame = LiveFrame(
                LiveRules.packRgba(plane.buffer, it.width, it.height, plane.rowStride, factor, swapRedBlue = false),
                it.width / factor,
                it.height / factor,
                it.imageInfo.rotationDegrees,
            )
            scope.launch { run(frame) }
        }
    }

    private fun take(): Boolean = synchronized(lock) {
        val now = SystemClock.elapsedRealtime()
        if (!LiveRules.shouldTake(busy, hot(), now, lastStartMs)) {
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
            if (gen == generation) publish(frame, result)
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Log.w(TAG, "live frame failed", e)
        } finally {
            synchronized(lock) { busy = false }
        }
    }

    private fun publish(frame: LiveFrame, result: TranslationResult) {
        val now = SystemClock.elapsedRealtime()
        overlay = tracker.update(result.regions)
        imageSize = IntSize(result.imageWidth.toInt(), result.imageHeight.toInt())
        latest = frame to result
        if (BuildConfig.DEBUG) {
            val line = LiveRules.readout(lastDoneMs?.let { now - it }, result.timings)
            readout = line
            Log.i(TAG, "live $line regions ${result.regions.size}")
        }
        lastDoneMs = now
    }
}

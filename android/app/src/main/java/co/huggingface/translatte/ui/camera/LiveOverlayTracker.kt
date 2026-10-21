package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import kotlin.math.max
import kotlin.math.min

/** A live region whose identity survives updates; [from] is where its box was shown before this update. */
data class TrackedRegion(val id: Long, val region: Region, val from: List<Point>, val missed: Int)

/**
 * Keeps the live overlay steady between updates, with the same rules as iOS's `LiveOverlayTracker`.
 * Each translated new region takes the not-yet-taken previous region it overlaps most (IoU at least
 * [MIN_IOU]). With the same source text, it keeps that region's identity and translation and slides
 * from the old box. With different text, it replaces it. Previous regions nothing took stay for
 * [MAX_MISSED] updates.
 */
class LiveOverlayTracker {
    var regions: List<TrackedRegion> = emptyList()
        private set
    private var nextId = 0L

    fun update(incoming: List<Region>): List<TrackedRegion> {
        val taken = BooleanArray(regions.size)
        val out = mutableListOf<TrackedRegion>()
        for (r in incoming) {
            if (r.translatedText == null) continue
            var best = -1
            var bestIou = 0f
            regions.forEachIndexed { i, old ->
                val v = iou(old.region.corners, r.corners)
                if (!taken[i] && v >= MIN_IOU && v > bestIou) {
                    best = i
                    bestIou = v
                }
            }
            if (best >= 0) taken[best] = true
            val old = regions.getOrNull(best)
            out += if (old != null && old.region.sourceText == r.sourceText) {
                TrackedRegion(old.id, r.copy(translatedText = old.region.translatedText), old.region.corners, 0)
            } else {
                TrackedRegion(nextId++, r, r.corners, 0)
            }
        }
        regions.forEachIndexed { i, old ->
            if (!taken[i] && old.missed < MAX_MISSED) {
                out += TrackedRegion(old.id, old.region, old.region.corners, old.missed + 1)
            }
        }
        regions = out
        return out
    }

    fun reset() {
        regions = emptyList()
    }

    companion object {
        const val MIN_IOU = 0.3f
        const val MAX_MISSED = 2

        /** Intersection over union of the two quads' axis-aligned bounds. */
        fun iou(a: List<Point>, b: List<Point>): Float {
            val ax0 = a.minOf { it.x }
            val ax1 = a.maxOf { it.x }
            val ay0 = a.minOf { it.y }
            val ay1 = a.maxOf { it.y }
            val bx0 = b.minOf { it.x }
            val bx1 = b.maxOf { it.x }
            val by0 = b.minOf { it.y }
            val by1 = b.maxOf { it.y }
            val inter = max(0f, min(ax1, bx1) - max(ax0, bx0)) * max(0f, min(ay1, by1) - max(ay0, by0))
            val union = (ax1 - ax0) * (ay1 - ay0) + (bx1 - bx0) * (by1 - by0) - inter
            return if (union <= 0f) 0f else inter / union
        }
    }
}

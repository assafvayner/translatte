package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import kotlin.math.max
import kotlin.math.min

/** A live region whose identity survives updates; [from] is where its box was shown before this update. */
data class TrackedRegion(val id: Long, val region: Region, val from: List<Point>, val lastSeenMs: Long)

/**
 * Keeps the live overlay steady between updates, with the same rules as iOS's `LiveOverlayTracker`.
 * Each new region takes the not-yet-taken previous region it overlaps most (IoU at least [MIN_IOU]).
 * With the same source text, it keeps that region's identity and translation and slides from the old
 * box, even when its own translation is missing; with different text, a translated one replaces it
 * and an untranslated one is skipped. Previous regions nothing took stay until they have gone
 * [MAX_UNSEEN_MS] without a match, unless a new region overlaps them. An update with no regions
 * clears everything.
 */
class LiveOverlayTracker {
    var regions: List<TrackedRegion> = emptyList()
        private set
    private var nextId = 0L

    fun update(incoming: List<Region>, nowMs: Long): List<TrackedRegion> {
        if (incoming.isEmpty()) {
            regions = emptyList()
            return regions
        }
        val taken = BooleanArray(regions.size)
        val out = mutableListOf<TrackedRegion>()
        for (r in incoming) {
            var best = -1
            var bestIou = 0f
            regions.forEachIndexed { i, old ->
                val v = iou(old.region.corners, r.corners)
                if (!taken[i] && v >= MIN_IOU && v > bestIou) {
                    best = i
                    bestIou = v
                }
            }
            val old = regions.getOrNull(best)
            if (old != null && old.region.sourceText == r.sourceText) {
                taken[best] = true
                out += TrackedRegion(old.id, r.copy(translatedText = old.region.translatedText), old.region.corners, nowMs)
            } else if (r.translatedText != null) {
                if (best >= 0) taken[best] = true
                out += TrackedRegion(nextId++, r, r.corners, nowMs)
            }
        }
        regions.forEachIndexed { i, old ->
            if (!taken[i] && nowMs - old.lastSeenMs < MAX_UNSEEN_MS && incoming.none { intersects(old.region.corners, it.corners) }) {
                out += TrackedRegion(old.id, old.region, old.region.corners, old.lastSeenMs)
            }
        }
        regions = out
        return out
    }

    /** Drops regions unseen for [MAX_UNSEEN_MS], for when updates stall. */
    fun prune(nowMs: Long): List<TrackedRegion> {
        val kept = regions.filter { nowMs - it.lastSeenMs < MAX_UNSEEN_MS }
        if (kept.size != regions.size) regions = kept
        return regions
    }

    fun reset() {
        regions = emptyList()
    }

    companion object {
        const val MIN_IOU = 0.3f
        const val MAX_UNSEEN_MS = 500L

        /** Whether the two quads' axis-aligned bounds overlap by a positive area. */
        fun intersects(a: List<Point>, b: List<Point>): Boolean =
            min(a.maxOf { it.x }, b.maxOf { it.x }) > max(a.minOf { it.x }, b.minOf { it.x }) &&
                min(a.maxOf { it.y }, b.maxOf { it.y }) > max(a.minOf { it.y }, b.minOf { it.y })

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

package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import kotlin.math.abs
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min

/**
 * A live region whose identity survives updates; [from] is where its box was shown before this
 * update, [fadeFrom] the translation it showed before this update when that changed, and [drift]
 * how far the camera moved since an update last saw it.
 */
data class TrackedRegion(
    val id: Long,
    val region: Region,
    val from: List<Point>,
    val missedSinceMs: Long?,
    val fadeFrom: String? = null,
    val drift: Point = LiveMotion.ZERO,
)

/**
 * Keeps the live overlay steady between updates, with the same rules as iOS's `LiveOverlayTracker`.
 * Previous regions first move by the camera's motion since the last update. Then new and previous
 * regions pair up one to one, best overlap first (IoU at least [MIN_IOU]): a new region keeps its
 * pair's identity and slides from its box. When the source text is [similar], the old translation
 * stays unless the new one isn't similar to it (or is missing); otherwise the new translation
 * replaces it, and an untranslated new region can't pair. Unpaired new regions with a translation
 * are added. Unpaired previous regions stay until [MAX_UNSEEN_MS] after the first update that
 * missed them, however long updates take, unless a new region overlaps them or the camera moved
 * more than [MAX_DRIFT_LINES] of their lines since they were seen. Regions come out in drawing
 * order: missed ones first, then from the smallest box to the largest.
 */
class LiveOverlayTracker {
    var regions: List<TrackedRegion> = emptyList()
        private set
    private var nextId = 0L

    /** [shift] is how far the image moved from the last update's frame to this one's, if known. */
    fun update(incoming: List<Region>, nowMs: Long, shift: Point? = null): List<TrackedRegion> {
        val d = shift ?: LiveMotion.ZERO
        val old = regions.map { it.copy(region = it.region.copy(corners = moved(it.region.corners, d))) }
        val pairs = mutableListOf<Triple<Int, Int, Float>>()
        old.forEachIndexed { i, o ->
            incoming.forEachIndexed { j, r ->
                val v = iou(o.region.corners, r.corners)
                if (v >= MIN_IOU && (r.translatedText != null || similar(o.region.sourceText, r.sourceText))) pairs += Triple(i, j, v)
            }
        }
        pairs.sortByDescending { it.third }
        val taken = BooleanArray(old.size)
        val match = IntArray(incoming.size) { -1 }
        for ((i, j) in pairs) {
            if (!taken[i] && match[j] < 0) {
                taken[i] = true
                match[j] = i
            }
        }
        val out = mutableListOf<TrackedRegion>()
        incoming.forEachIndexed { j, r ->
            val o = old.getOrNull(match[j])
            if (o != null) {
                val text = translation(o.region, r)
                val fade = o.region.translatedText.takeIf { it != text }
                out += TrackedRegion(o.id, r.copy(translatedText = text), o.region.corners, null, fade)
            } else if (r.translatedText != null) {
                out += TrackedRegion(nextId++, r, r.corners, null)
            }
        }
        old.forEachIndexed { i, o ->
            if (taken[i] || incoming.any { intersects(o.region.corners, it.corners) }) return@forEachIndexed
            val drift = Point(o.drift.x + d.x, o.drift.y + d.y)
            val missedSince = o.missedSinceMs ?: nowMs
            if (nowMs - missedSince < MAX_UNSEEN_MS && hypot(drift.x, drift.y) <= MAX_DRIFT_LINES * lineHeight(o.region)) {
                out += TrackedRegion(o.id, o.region, o.region.corners, missedSince, drift = drift)
            }
        }
        regions = out.sortedWith(compareBy({ it.missedSinceMs == null }, { area(it.region.corners) }))
        return regions
    }

    /** Drops missed regions [MAX_UNSEEN_MS] after the update that missed them; never drops on a lack of updates. */
    fun prune(nowMs: Long): List<TrackedRegion> {
        val kept = regions.filter { it.missedSinceMs == null || nowMs - it.missedSinceMs < MAX_UNSEEN_MS }
        if (kept.size != regions.size) regions = kept
        return regions
    }

    fun reset() {
        regions = emptyList()
    }

    companion object {
        const val MIN_IOU = 0.3f
        const val MAX_UNSEEN_MS = 500L

        /** Texts at least this similar ([similarity]) are the same text read again. */
        const val SIMILAR_TEXT = 0.6f

        /** Missed regions go once the camera moved this many of their lines: they'd be out of place. */
        const val MAX_DRIFT_LINES = 1.5f

        /** The translation a region that paired with [old] shows. */
        fun translation(old: Region, new: Region): String? {
            val was = old.translatedText ?: return new.translatedText
            val now = new.translatedText ?: return was
            return if (similar(old.sourceText, new.sourceText) && similar(was, now)) was else now
        }

        fun similar(a: String, b: String): Boolean = similarity(a, b) >= SIMILAR_TEXT

        /** 1 minus the edit distance over the longer length, ignoring case and whitespace. */
        fun similarity(a: String, b: String): Float {
            val x = a.lowercase().filterNot { it.isWhitespace() }
            val y = b.lowercase().filterNot { it.isWhitespace() }
            if (x.isEmpty() && y.isEmpty()) return 1f
            var prev = IntArray(y.length + 1) { it }
            var cur = IntArray(y.length + 1)
            for (i in 1..x.length) {
                cur[0] = i
                for (j in 1..y.length) {
                    cur[j] = minOf(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + if (x[i - 1] == y[j - 1]) 0 else 1)
                }
                prev = cur.also { cur = prev }
            }
            return 1f - prev[y.length].toFloat() / max(x.length, y.length)
        }

        /** The region's line height, or its box's short side when unknown. */
        fun lineHeight(r: Region): Float {
            if (r.lineHeight > 0f) return r.lineHeight
            val c = r.corners
            return min(hypot(c[1].x - c[0].x, c[1].y - c[0].y), hypot(c[3].x - c[0].x, c[3].y - c[0].y))
        }

        fun moved(corners: List<Point>, by: Point): List<Point> = corners.map { Point(it.x + by.x, it.y + by.y) }

        /** The area of the quad's axis-aligned bounds. */
        fun area(c: List<Point>): Float = abs((c.maxOf { it.x } - c.minOf { it.x }) * (c.maxOf { it.y } - c.minOf { it.y }))

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

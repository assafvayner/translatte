package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.TranslationResult
import kotlin.math.roundToInt

/** One change to the live overlay: "add", "move", "retext" or "remove" (with a [reason]) of [tracked]. */
data class LiveEvent(val ev: String, val tracked: TrackedRegion, val reason: String? = null)

/**
 * Debug builds: the live overlay's updates and changes as JSON lines for scripts/live/analyze.py,
 * in the same format as iOS's `LiveEventLog`. Times are ms since [start] (a replay's start) or,
 * with the camera, since the first event.
 */
class LiveEventLog(private val emit: (String) -> Unit) {
    private var startMs: Long? = null

    fun start(nowMs: Long) {
        startMs = nowMs
    }

    fun update(nowMs: Long, prev: List<TrackedRegion>, next: List<TrackedRegion>, result: TranslationResult, intervalMs: Long?, ptsMs: Long?) {
        val t = elapsed(nowMs)
        emit(updateJson(t, result, next.filter { it.missedSinceMs == null }.map { it.id }, next.size, intervalMs, ptsMs))
        diff(prev, next, result.regions).forEach { emit(json(t, it)) }
    }

    fun prune(nowMs: Long, prev: List<TrackedRegion>, next: List<TrackedRegion>) {
        val t = elapsed(nowMs)
        diff(prev, next, null).forEach { emit(json(t, it)) }
    }

    fun reset(nowMs: Long, prev: List<TrackedRegion>) {
        if (prev.isEmpty()) return
        val t = elapsed(nowMs)
        prev.forEach { emit(json(t, LiveEvent("remove", it, "reset"))) }
    }

    private fun elapsed(nowMs: Long): Long = nowMs - (startMs ?: nowMs.also { startMs = it })

    companion object {
        /**
         * What changed from [prev] to [next]. A removed region is "empty" when the update had no
         * regions, "overlap" when a new region covers it, and otherwise "expired" ([incoming] is
         * null for a prune).
         */
        fun diff(prev: List<TrackedRegion>, next: List<TrackedRegion>, incoming: List<Region>?): List<LiveEvent> {
            val nextIds = next.mapTo(HashSet()) { it.id }
            val prevById = prev.associateBy { it.id }
            val out = mutableListOf<LiveEvent>()
            for (old in prev) {
                if (old.id in nextIds) continue
                val reason = when {
                    incoming == null -> "expired"
                    incoming.isEmpty() -> "empty"
                    incoming.any { intersects(old.region.corners, it.corners) } -> "overlap"
                    else -> "expired"
                }
                out += LiveEvent("remove", old, reason)
            }
            for (t in next) {
                val old = prevById[t.id]
                when {
                    old == null -> out += LiveEvent("add", t)
                    old.region.translatedText != t.region.translatedText -> out += LiveEvent("retext", t)
                    box(old.region.corners) != box(t.region.corners) -> out += LiveEvent("move", t)
                }
            }
            return out
        }

        /**
         * [seen] are the ids this update matched or added; [shown] counts the boxes on screen after
         * it. `blocks` lists every region of the update, matched or not.
         */
        fun updateJson(tMs: Long, result: TranslationResult, seen: List<Long>, shown: Int, intervalMs: Long?, ptsMs: Long?): String {
            val t = result.timings
            return buildString {
                append("{\"t_ms\":$tMs,\"ev\":\"update\",\"regions\":${result.regions.size},\"shown\":$shown,\"seen\":[${seen.joinToString(",")}]")
                append(",\"det_ms\":${t.detectMs},\"rec_ms\":${t.recognizeMs},\"tr_ms\":${t.translateMs},\"engine\":\"${t.ocrEngine}\"")
                append(",\"w\":${result.imageWidth},\"h\":${result.imageHeight}")
                intervalMs?.let { append(",\"interval_ms\":$it") }
                ptsMs?.let { append(",\"pts_ms\":$it") }
                append(",\"blocks\":[")
                result.regions.forEachIndexed { i, r ->
                    if (i > 0) append(",")
                    append("{\"src\":")
                    appendString(r.sourceText)
                    append(",\"box\":[${box(r.corners).joinToString(",")}]}")
                }
                append("]}")
            }
        }

        fun json(tMs: Long, e: LiveEvent): String = buildString {
            val r = e.tracked.region
            append("{\"t_ms\":$tMs,\"ev\":\"${e.ev}\",\"id\":${e.tracked.id},\"src\":")
            appendString(r.sourceText)
            append(",\"tr\":")
            r.translatedText?.let { appendString(it) } ?: append("null")
            append(",\"box\":[${box(r.corners).joinToString(",")}]")
            e.reason?.let { append(",\"reason\":\"$it\"") }
            append("}")
        }

        /** The quad's axis-aligned bounds, [x0, y0, x1, y1], rounded to whole pixels. */
        fun box(corners: List<Point>): List<Int> = listOf(
            corners.minOf { it.x }.roundToInt(),
            corners.minOf { it.y }.roundToInt(),
            corners.maxOf { it.x }.roundToInt(),
            corners.maxOf { it.y }.roundToInt(),
        )

        private fun intersects(a: List<Point>, b: List<Point>): Boolean =
            minOf(a.maxOf { it.x }, b.maxOf { it.x }) > maxOf(a.minOf { it.x }, b.minOf { it.x }) &&
                minOf(a.maxOf { it.y }, b.maxOf { it.y }) > maxOf(a.minOf { it.y }, b.minOf { it.y })

        private fun StringBuilder.appendString(s: String) {
            append('"')
            for (c in s) {
                when {
                    c == '"' -> append("\\\"")
                    c == '\\' -> append("\\\\")
                    c == '\n' -> append("\\n")
                    c == '\r' -> append("\\r")
                    c == '\t' -> append("\\t")
                    c < ' ' -> append("\\u%04x".format(c.code))
                    else -> append(c)
                }
            }
            append('"')
        }
    }
}

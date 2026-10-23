package co.huggingface.translatte.ui.camera

import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.motionReference

/** A frame's [LiveRules.thumbnail] and the upright size, in frame pixels, of the frame it shrinks. */
class Thumb(val pixels: ByteArray, val width: Int, val height: Int)

/**
 * Follows camera motion since the frame the overlay's boxes were read from (the reference), as a
 * global translation in that frame's pixels, the same as iOS's `LiveMotion`. [frame] takes each
 * preview frame and [rebase] makes a published result's frame the reference. Thread-safe.
 *
 * Each frame is measured against the reference and against the frame before it. Recent frames
 * keep their position along that frame-to-frame chain, which breaks where two frames can't be
 * matched; when a frame is too far from the reference or too blurred to measure directly, its
 * motion along the chain from the last frame that was (or from the reference itself) adds on. A
 * frame that can't be measured either way leaves [shift] as it was.
 *
 * [measure] makes a reference of a thumbnail: how far a later thumbnail moved from it, in
 * thumbnail pixels, or null when unsure; null for a thumbnail too flat to match.
 */
class LiveMotion(private val measure: (ByteArray) -> ((ByteArray) -> Point?)? = ::ffiMeasure) {
    /** A recent frame and its position along chain [segment], in thumbnail pixels. */
    private class Entry(val thumb: ByteArray, val pos: Point, val segment: Int)

    private val lock = Any()
    private val recent = ArrayDeque<Entry>()
    private var segment = 0
    private var reference: Thumb? = null
    private var estimate: ((ByteArray) -> Point?)? = null

    /** The last frame whose motion from the reference is known, and that motion. */
    private var known: Entry? = null
    private var knownShift = ZERO

    /** The motion from the reference to the latest frame, in reference frame pixels. */
    var shift = ZERO
        get() = synchronized(lock) { field }
        private set

    /** A new preview frame's thumbnail; returns whether [shift] changed. */
    fun frame(thumb: ByteArray): Boolean = synchronized(lock) {
        val prev = recent.lastOrNull()
        val step = prev?.let { measure(it.thumb)?.invoke(thumb) }
        val entry = if (prev != null && step != null) {
            Entry(thumb, Point(prev.pos.x + step.x, prev.pos.y + step.y), prev.segment)
        } else {
            Entry(thumb, ZERO, ++segment)
        }
        recent.addLast(entry)
        if (recent.size > RECENT_FRAMES) recent.removeFirst()
        val direct = estimate?.invoke(thumb)?.let(::toFrame)
        if (direct != null) {
            known = entry
            knownShift = direct
        }
        shift = direct ?: chained(entry) ?: return false
        true
    }

    /**
     * Makes [source] the reference, with [shift] measured from it to the latest frame; returns the
     * motion from the previous reference to [source], or null when unknown.
     */
    fun rebase(source: Thumb?): Point? = synchronized(lock) {
        val sourceEntry = source?.let { s -> recent.lastOrNull { it.thumb.contentEquals(s.pixels) } }
        val between = source?.let { estimate?.invoke(it.pixels) }?.let(::toFrame) ?: sourceEntry?.let(::chained)
        val before = shift
        reference = source
        estimate = source?.let { measure(it.pixels) }
        known = sourceEntry
        knownShift = ZERO
        val latest = recent.lastOrNull()
        val direct = latest?.let { estimate?.invoke(it.thumb) }?.let(::toFrame)
        if (direct != null) {
            known = latest
            knownShift = direct
        }
        shift = direct
            ?: latest?.let(::chained)
            ?: between?.let { Point(before.x - it.x, before.y - it.y) }
            ?: ZERO
        between
    }

    fun reset() = synchronized(lock) {
        recent.clear()
        reference = null
        estimate = null
        known = null
        shift = ZERO
    }

    /** The motion from the reference to [e] along the chain from the last known frame. */
    private fun chained(e: Entry): Point? {
        val k = known?.takeIf { it.segment == e.segment } ?: return null
        val d = toFrame(Point(e.pos.x - k.pos.x, e.pos.y - k.pos.y))
        return Point(knownShift.x + d.x, knownShift.y + d.y)
    }

    private fun toFrame(p: Point): Point {
        val r = reference ?: return ZERO
        return Point(p.x * r.width / LiveRules.THUMB_SIDE, p.y * r.height / LiveRules.THUMB_SIDE)
    }

    companion object {
        val ZERO = Point(0f, 0f)

        /** About 3 s of preview frames: results are rarely older. */
        const val RECENT_FRAMES = 90
    }
}

private fun ffiMeasure(thumb: ByteArray): ((ByteArray) -> Point?)? {
    val side = LiveRules.THUMB_SIDE.toUInt()
    val reference = motionReference(thumb, side, side) ?: return null
    return { current -> reference.shiftTo(current)?.let { Point(it.dx, it.dy) } }
}

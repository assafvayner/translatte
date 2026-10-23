package co.huggingface.translatte.ui.camera

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.translatte_ffi.Point
import kotlin.math.abs

class LiveMotionTest {
    /** A fake thumbnail of a scene at [x] thumbnail pixels; [sure] = false can't be measured. */
    private fun thumb(x: Int, sure: Boolean = true) = byteArrayOf(x.toByte(), if (sure) 0 else 1)

    /** Measures the difference of the scene positions up to 20, unless either thumbnail is unsure. */
    private val measure: (ByteArray) -> ((ByteArray) -> Point?)? = { ref ->
        if (ref[1].toInt() == 1) {
            null
        } else {
            { cur -> if (cur[1].toInt() == 1 || abs(cur[0] - ref[0]) > 20) null else Point((cur[0] - ref[0]).toFloat(), 0f) }
        }
    }

    /** A frame twice the thumbnail's size, so frame pixels are double thumbnail pixels. */
    private fun source(x: Int, sure: Boolean = true) = Thumb(thumb(x, sure), 2 * LiveRules.THUMB_SIDE, LiveRules.THUMB_SIDE)

    @Test
    fun measuresFramesAgainstTheReferenceInFramePixels() {
        val m = LiveMotion(measure)
        assertFalse(m.frame(thumb(3)))
        assertNull(m.rebase(source(0)))
        assertEquals(Point(6f, 0f), m.shift)
        assertTrue(m.frame(thumb(5)))
        assertEquals(Point(10f, 0f), m.shift)
    }

    @Test
    fun anUnsureFrameKeepsTheLastShift() {
        val m = LiveMotion(measure)
        m.rebase(source(0))
        m.frame(thumb(5))
        assertFalse(m.frame(thumb(9, sure = false)))
        assertEquals(Point(10f, 0f), m.shift)
    }

    @Test
    fun motionTooFarToMeasureAddsOnAlongTheChainOfFrames() {
        val m = LiveMotion(measure)
        m.rebase(source(0))
        m.frame(thumb(10))
        assertTrue(m.frame(thumb(25)))
        assertEquals(Point(50f, 0f), m.shift)
        assertTrue(m.frame(thumb(40)))
        assertEquals(Point(80f, 0f), m.shift)
        assertFalse(m.frame(thumb(41, sure = false)))
        assertFalse(m.frame(thumb(42)))
        assertEquals(Point(80f, 0f), m.shift)
        assertTrue(m.frame(thumb(18)))
        assertEquals(Point(36f, 0f), m.shift)
    }

    @Test
    fun aRecentSourceIsFollowedAlongTheChain() {
        val m = LiveMotion(measure)
        m.frame(thumb(0))
        m.frame(thumb(15))
        m.frame(thumb(30))
        assertNull(m.rebase(source(0)))
        assertEquals(Point(60f, 0f), m.shift)
        assertTrue(m.frame(thumb(45)))
        assertEquals(Point(90f, 0f), m.shift)
        assertEquals(Point(60f, 0f), m.rebase(source(30)))
        assertEquals(Point(30f, 0f), m.shift)
    }

    @Test
    fun rebaseReturnsTheMotionBetweenSources() {
        val m = LiveMotion(measure)
        m.rebase(source(0))
        m.frame(thumb(7))
        assertEquals(Point(8f, 0f), m.rebase(source(4)))
        assertEquals(Point(6f, 0f), m.shift)
    }

    @Test
    fun anUnsureLatestFrameFollowsFromTheMotionBetweenSources() {
        val m = LiveMotion(measure)
        m.rebase(source(0))
        m.frame(thumb(7))
        m.frame(thumb(7, sure = false))
        m.rebase(source(4))
        assertEquals(Point(6f, 0f), m.shift)
    }

    @Test
    fun anUnmeasurableSourceHasNoMotion() {
        val m = LiveMotion(measure)
        m.rebase(source(0))
        m.frame(thumb(7))
        assertNull(m.rebase(source(4, sure = false)))
        assertEquals(LiveMotion.ZERO, m.shift)
        assertFalse(m.frame(thumb(9)))
    }

    @Test
    fun resetForgetsTheReference() {
        val m = LiveMotion(measure)
        m.rebase(source(0))
        m.frame(thumb(7))
        m.reset()
        assertEquals(LiveMotion.ZERO, m.shift)
        assertFalse(m.frame(thumb(9)))
    }
}

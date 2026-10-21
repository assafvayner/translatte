package co.huggingface.translatte.ui.camera

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb
import uniffi.translatte_ffi.StageTimings
import uniffi.translatte_ffi.TranslationResult
import java.nio.ByteBuffer

class LiveRulesTest {
    private val noTimings = StageTimings(0u, 0u, 0u)

    private fun result(translated: String?, error: String? = null) = TranslationResult(
        10u,
        10u,
        listOf(
            Region(
                listOf(Point(0f, 0f), Point(1f, 0f), Point(1f, 1f), Point(0f, 1f)),
                "s",
                translated,
                Rgb(0.toUByte(), 0.toUByte(), 0.toUByte()),
                Rgb(0.toUByte(), 0.toUByte(), 0.toUByte()),
                0f,
            ),
        ),
        error,
        noTimings,
    )

    @Test
    fun activeOnlyWhenEnabledAndTheSourceIsInstalled() {
        assertTrue(LiveRules.isActive(enabled = true, sourceInstalled = true))
        assertFalse(LiveRules.isActive(enabled = false, sourceInstalled = true))
        assertFalse(LiveRules.isActive(enabled = true, sourceInstalled = false))
    }

    @Test
    fun takesAFrameWhenIdleAndOnAHotDeviceAtMostOncePerSecond() {
        assertFalse(LiveRules.shouldTake(busy = true, hot = false, nowMs = 5000, lastStartMs = 0))
        assertTrue(LiveRules.shouldTake(busy = false, hot = false, nowMs = 10, lastStartMs = 0))
        assertTrue(LiveRules.shouldTake(busy = false, hot = true, nowMs = 10, lastStartMs = null))
        assertFalse(LiveRules.shouldTake(busy = false, hot = true, nowMs = 999, lastStartMs = 0))
        assertTrue(LiveRules.shouldTake(busy = false, hot = true, nowMs = 1000, lastStartMs = 0))
    }

    @Test
    fun downscalesTheLongSideTo1280OrLess() {
        assertEquals(1, LiveRules.downscaleFactor(960, 720))
        assertEquals(1, LiveRules.downscaleFactor(1280, 960))
        assertEquals(2, LiveRules.downscaleFactor(1920, 1440))
        assertEquals(4, LiveRules.downscaleFactor(4032, 3024))
    }

    @Test
    fun snapUsesTheLiveResultOnlyWithATranslation() {
        assertFalse(LiveRules.snapUsesLive(null))
        assertFalse(LiveRules.snapUsesLive(result(null)))
        assertFalse(LiveRules.snapUsesLive(result("t", error = "boom")))
        assertFalse(LiveRules.snapUsesLive(result("t").copy(regions = emptyList())))
        assertTrue(LiveRules.snapUsesLive(result("t")))
    }

    @Test
    fun readoutShowsTheRateAndStageTimes() {
        assertEquals("2.5/s · det 120 · rec 85 · tr 40 ms", LiveRules.readout(400, StageTimings(120u, 85u, 40u)))
        assertEquals("–/s · det 0 · rec 0 · tr 0 ms", LiveRules.readout(null, noTimings))
    }

    @Test
    fun fillCentersAndCropsLikeThePreview() {
        assertEquals(FillTransform(2.5f, -360f, 0f), LiveRules.fill(720f, 960f, 1080f, 2400f))
        assertEquals(FillTransform(4f, 0f, -100f), LiveRules.fill(100f, 100f, 400f, 200f))
    }

    /** A 2x2 RGBA frame with 4 bytes of row padding: pixel n is (n, 10+n, 20+n, 30+n). */
    private fun padded(): ByteBuffer {
        val bytes = ByteArray(24)
        for (y in 0..1) for (x in 0..1) {
            val n = y * 2 + x
            for (c in 0..3) bytes[y * 12 + x * 4 + c] = (c * 10 + n).toByte()
        }
        return ByteBuffer.wrap(bytes)
    }

    @Test
    fun packsRowsWithoutPadding() {
        val out = LiveRules.packRgba(padded(), 2, 2, rowStride = 12, factor = 1, swapRedBlue = false)
        assertArrayEquals(byteArrayOf(0, 10, 20, 30, 1, 11, 21, 31, 2, 12, 22, 32, 3, 13, 23, 33), out)
    }

    @Test
    fun swapsRedAndBlueForBgraFrames() {
        val out = LiveRules.packRgba(padded(), 2, 2, rowStride = 12, factor = 1, swapRedBlue = true)
        assertArrayEquals(byteArrayOf(20, 10, 0, 30, 21, 11, 1, 31, 22, 12, 2, 32, 23, 13, 3, 33), out)
    }

    @Test
    fun keepsEveryNthPixelOfEveryNthRow() {
        val out = LiveRules.packRgba(padded(), 2, 2, rowStride = 12, factor = 2, swapRedBlue = false)
        assertArrayEquals(byteArrayOf(0, 10, 20, 30), out)
    }

    @Test
    fun aTightlyPackedFrameIsCopiedAsIs() {
        val bytes = ByteArray(16) { it.toByte() }
        assertArrayEquals(bytes, LiveRules.packRgba(ByteBuffer.wrap(bytes), 2, 2, rowStride = 8, factor = 1, swapRedBlue = false))
    }
}

package co.huggingface.translatte.ui.camera

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb

class LiveOverlayTrackerTest {
    /** A 40x20 box at ([x], 0). */
    private fun region(source: String, x: Float, translated: String? = source.uppercase()) = Region(
        corners = listOf(Point(x, 0f), Point(x + 40f, 0f), Point(x + 40f, 20f), Point(x, 20f)),
        sourceText = source,
        translatedText = translated,
        background = Rgb(255.toUByte(), 255.toUByte(), 255.toUByte()),
        foreground = Rgb(0.toUByte(), 0.toUByte(), 0.toUByte()),
        angle = 0f,
    )

    @Test
    fun tracksOnlyTranslatedRegions() {
        val out = LiveOverlayTracker().update(listOf(region("a", 0f), region("b", 100f, translated = null)))
        assertEquals(1, out.size)
        assertEquals("a", out[0].region.sourceText)
        assertEquals(out[0].region.corners, out[0].from)
        assertEquals(0, out[0].missed)
    }

    @Test
    fun theSameTextKeepsItsIdentityAndTranslationAndMovesFromItsOldBox() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f, "A")))[0]
        val out = t.update(listOf(region("a", 5f, "A2")))
        assertEquals(1, out.size)
        assertEquals(first.id, out[0].id)
        assertEquals("A", out[0].region.translatedText)
        assertEquals(first.region.corners, out[0].from)
        assertEquals(5f, out[0].region.corners[0].x)
    }

    @Test
    fun changedTextReplacesTheRegion() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)))[0]
        val out = t.update(listOf(region("b", 5f)))
        assertEquals(1, out.size)
        assertNotEquals(first.id, out[0].id)
        assertEquals("B", out[0].region.translatedText)
        assertEquals(out[0].region.corners, out[0].from)
    }

    @Test
    fun unmatchedRegionsPersistForTwoUpdates() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)))
        assertEquals(1, t.update(emptyList()).single().missed)
        assertEquals(2, t.update(emptyList()).single().missed)
        assertTrue(t.update(emptyList()).isEmpty())
    }

    @Test
    fun lowOverlapIsNotAMatch() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)))[0]
        val out = t.update(listOf(region("a", 30f)))
        assertEquals(2, out.size)
        assertNotEquals(first.id, out[0].id)
        assertEquals(first.id, out[1].id)
        assertEquals(1, out[1].missed)
    }

    @Test
    fun eachPreviousRegionMatchesOnce() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)))[0]
        val out = t.update(listOf(region("a", 0f), region("a", 2f)))
        assertEquals(1, out.count { it.id == first.id })
        assertEquals(2, out.size)
    }

    @Test
    fun resetForgetsEverything() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)))
        t.reset()
        assertTrue(t.update(emptyList()).isEmpty())
    }

    @Test
    fun iouOfBounds() {
        val a = region("a", 0f).corners
        assertEquals(1f, LiveOverlayTracker.iou(a, a), 1e-6f)
        assertEquals(0f, LiveOverlayTracker.iou(a, region("a", 100f).corners), 1e-6f)
        assertEquals(1f / 7f, LiveOverlayTracker.iou(a, region("a", 30f).corners), 1e-6f)
    }
}

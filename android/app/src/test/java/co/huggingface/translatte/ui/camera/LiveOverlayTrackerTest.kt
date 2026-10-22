package co.huggingface.translatte.ui.camera

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb

class LiveOverlayTrackerTest {
    /** A 40x20 box at ([x], 0), or [width] wide. */
    private fun region(source: String, x: Float, translated: String? = source.uppercase(), width: Float = 40f, lineHeight: Float = 0f) = Region(
        corners = listOf(Point(x, 0f), Point(x + width, 0f), Point(x + width, 20f), Point(x, 20f)),
        sourceText = source,
        translatedText = translated,
        background = Rgb(255.toUByte(), 255.toUByte(), 255.toUByte()),
        foreground = Rgb(0.toUByte(), 0.toUByte(), 0.toUByte()),
        angle = 0f,
        lineHeight = lineHeight,
    )

    @Test
    fun tracksOnlyTranslatedRegionsWithoutAnOldMatch() {
        val out = LiveOverlayTracker().update(listOf(region("a", 0f), region("b", 100f, translated = null)), 0)
        assertEquals(1, out.size)
        assertEquals("a", out[0].region.sourceText)
        assertEquals(out[0].region.corners, out[0].from)
        assertNull(out[0].missedSinceMs)
    }

    @Test
    fun theSameTextKeepsItsIdentityAndTranslationAndMovesFromItsOldBox() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f, "Exit")), 0)[0]
        val out = t.update(listOf(region("a", 5f, "Exits")), 1000)
        assertEquals(1, out.size)
        assertEquals(first.id, out[0].id)
        assertEquals("Exit", out[0].region.translatedText)
        assertNull(out[0].fadeFrom)
        assertEquals(first.region.corners, out[0].from)
        assertEquals(5f, out[0].region.corners[0].x)
        assertNull(out[0].missedSinceMs)
    }

    @Test
    fun changedTextKeepsTheBoxAndFadesToTheNewTranslation() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        val out = t.update(listOf(region("b", 5f)), 100)
        assertEquals(1, out.size)
        assertEquals(first.id, out[0].id)
        assertEquals("B", out[0].region.translatedText)
        assertEquals("A", out[0].fadeFrom)
        assertEquals(first.region.corners, out[0].from)
    }

    @Test
    fun similarTextKeepsTheOldTranslationUntilTheNewOneDiffers() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("sortie", 0f, "Exit")), 0)
        assertEquals("Exit", t.update(listOf(region("sortle", 0f, "Exits")), 100)[0].region.translatedText)
        val out = t.update(listOf(region("sortie", 0f, "Way out")), 200)
        assertEquals("Way out", out[0].region.translatedText)
        assertEquals("Exit", out[0].fadeFrom)
    }

    @Test
    fun pairsTheBestOverlapsFirst() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        val out = t.update(listOf(region("a", 12f), region("a", 1f)), 100)
        assertEquals(first.id, out.first { it.region.corners[0].x == 1f }.id)
        assertNotEquals(first.id, out.first { it.region.corners[0].x == 12f }.id)
    }

    @Test
    fun anEmptyUpdateKeepsBoxesUntilTheyExpire() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        val out = t.update(emptyList(), 100)
        assertEquals(listOf(first.id), out.map { it.id })
        assertEquals(100L, out[0].missedSinceMs)
        assertTrue(t.prune(600).isEmpty())
    }

    @Test
    fun aMissedRegionGoesOnceTheCameraMovedFurtherThanItsLines() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f), region("b", 200f, lineHeight = 10f)), 0)
        val out = t.update(emptyList(), 100, shift = Point(0f, 16f))
        assertEquals(listOf("a"), out.map { it.region.sourceText })
        assertEquals(Point(0f, 16f), out[0].drift)
        assertTrue(t.update(emptyList(), 200, shift = Point(0f, 15f)).isEmpty())
    }

    @Test
    fun missedRegionsDrawFirstThenSmallToLarge() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 300f)), 0)
        val out = t.update(listOf(region("big", 0f, width = 80f), region("small", 100f, width = 30f)), 100)
        assertEquals(listOf("a", "small", "big"), out.map { it.region.sourceText })
    }

    @Test
    fun similarityIgnoresCaseAndSpaces() {
        assertEquals(1f, LiveOverlayTracker.similarity("PURE GREEN", "puregreen"), 1e-6f)
        assertEquals(2f / 3f, LiveOverlayTracker.similarity("abc", "abd"), 1e-6f)
        assertEquals(0f, LiveOverlayTracker.similarity("abc", ""), 1e-6f)
        assertEquals(1f, LiveOverlayTracker.similarity(" ", ""), 1e-6f)
    }

    @Test
    fun aRegionMissedByAnUpdateExpiresAfter500msFromThatUpdate() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        val other = region("b", 200f)
        assertEquals(first.id, t.update(listOf(other), 1800).first { it.region.sourceText == "a" }.id)
        assertEquals(2, t.prune(2299).size)
        assertEquals(listOf("b"), t.prune(2300).map { it.region.sourceText })
    }

    @Test
    fun aMissedRegionKeepsItsFirstMissedTime() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)), 1000)
        val other = region("b", 200f)
        val out = t.update(listOf(other), 1300)
        assertEquals(1300L, out.first { it.region.sourceText == "a" }.missedSinceMs)
        assertNull(out.first { it.region.sourceText == "b" }.missedSinceMs)
        assertEquals(1300L, t.update(listOf(other), 1400).first { it.region.sourceText == "a" }.missedSinceMs)
    }

    @Test
    fun aMissedRegionThatIsMatchedAgainStays() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        t.update(listOf(region("b", 200f)), 1800)
        val out = t.update(listOf(region("a", 0f), region("b", 200f)), 2200)
        val a = out.first { it.region.sourceText == "a" }
        assertEquals(first.id, a.id)
        assertNull(a.missedSinceMs)
        assertEquals(2, t.prune(5000).size)
    }

    @Test
    fun anUnmatchedRegionIsDroppedWhenANewRegionOverlapsIt() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)), 0)
        val out = t.update(listOf(region("b", 35f)), 100)
        assertEquals(listOf("b"), out.map { it.region.sourceText })
    }

    @Test
    fun touchingEdgesDoNotOverlap() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)), 0)
        val out = t.update(listOf(region("b", 40f)), 100)
        assertEquals(listOf("a", "b"), out.map { it.region.sourceText })
    }

    @Test
    fun overlapRemovalAlsoCountsRegionsWithoutATranslation() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)), 0)
        val out = t.update(listOf(region("b", 35f, translated = null), region("c", 200f)), 100)
        assertEquals(listOf("c"), out.map { it.region.sourceText })
    }

    @Test
    fun aMissingTranslationOfTheSameTextKeepsTheOldOne() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f, "A")), 0)[0]
        val out = t.update(listOf(region("a", 5f, translated = null)), 1000)
        assertEquals(1, out.size)
        assertEquals(first.id, out[0].id)
        assertEquals("A", out[0].region.translatedText)
        assertNull(out[0].missedSinceMs)
    }

    @Test
    fun aMissingTranslationOfDifferentTextDoesNotTakeTheOldRegion() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f, "A")), 0)
        val out = t.update(listOf(region("b", 5f, translated = null)), 100)
        assertTrue(out.isEmpty())
    }

    @Test
    fun aRegionSurvivesALongGapBetweenUpdates() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)), 0)
        assertEquals(1, t.prune(1900).size)
    }

    @Test
    fun lowOverlapIsNotAMatchAndDropsTheOldRegion() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        val out = t.update(listOf(region("a", 30f)), 100)
        assertEquals(1, out.size)
        assertNotEquals(first.id, out[0].id)
        assertEquals(out[0].region.corners, out[0].from)
    }

    @Test
    fun previousRegionsFollowTheCameraBeforeMatching() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f), region("b", 200f)), 0)
        val out = t.update(listOf(region("a", 30f)), 100, shift = Point(30f, 0f))
        val a = out.first { it.region.sourceText == "a" }
        assertEquals(first[0].id, a.id)
        assertEquals(first[0].region.corners.map { Point(it.x + 30f, it.y) }, a.from)
        assertEquals(230f, out.first { it.region.sourceText == "b" }.region.corners[0].x)
    }

    @Test
    fun eachPreviousRegionMatchesOnce() {
        val t = LiveOverlayTracker()
        val first = t.update(listOf(region("a", 0f)), 0)[0]
        val out = t.update(listOf(region("a", 0f), region("a", 2f)), 100)
        assertEquals(1, out.count { it.id == first.id })
        assertEquals(2, out.size)
    }

    @Test
    fun resetForgetsEverything() {
        val t = LiveOverlayTracker()
        t.update(listOf(region("a", 0f)), 0)
        t.reset()
        assertTrue(t.regions.isEmpty())
    }

    @Test
    fun iouOfBounds() {
        val a = region("a", 0f).corners
        assertEquals(1f, LiveOverlayTracker.iou(a, a), 1e-6f)
        assertEquals(0f, LiveOverlayTracker.iou(a, region("a", 100f).corners), 1e-6f)
        assertEquals(1f / 7f, LiveOverlayTracker.iou(a, region("a", 30f).corners), 1e-6f)
    }
}

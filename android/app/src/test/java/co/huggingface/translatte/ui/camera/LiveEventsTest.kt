package co.huggingface.translatte.ui.camera

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.translatte_ffi.Point
import uniffi.translatte_ffi.Region
import uniffi.translatte_ffi.Rgb
import uniffi.translatte_ffi.StageTimings
import uniffi.translatte_ffi.TranslationResult

class LiveEventsTest {
    /** A 40x20 box at ([x], 0). */
    private fun region(source: String, x: Float, translated: String? = source.uppercase()) = Region(
        corners = listOf(Point(x, 0f), Point(x + 40f, 0f), Point(x + 40f, 20f), Point(x, 20f)),
        sourceText = source,
        translatedText = translated,
        background = Rgb(255.toUByte(), 255.toUByte(), 255.toUByte()),
        foreground = Rgb(0.toUByte(), 0.toUByte(), 0.toUByte()),
        angle = 0f,
    )

    private fun tracked(id: Long, r: Region) = TrackedRegion(id, r, r.corners, null)

    private fun result(regions: List<Region>) = TranslationResult(
        imageWidth = 720u,
        imageHeight = 1280u,
        regions = regions,
        translationError = null,
        timings = StageTimings(120u, 85u, 40u),
    )

    @Test
    fun diffReportsAddsMovesRetextsAndRemovals() {
        val prev = listOf(tracked(0, region("a", 0f)), tracked(1, region("b", 100f)), tracked(2, region("c", 200f)))
        val next = listOf(tracked(0, region("a", 5f)), tracked(1, region("b", 100f, "B2")), tracked(3, region("d", 300f)))
        val events = LiveEventLog.diff(prev, next, listOf(region("a", 5f), region("d", 300f)))
        assertEquals(listOf("remove", "move", "retext", "add"), events.map { it.ev })
        assertEquals(listOf(2L, 0L, 1L, 3L), events.map { it.tracked.id })
        assertEquals("expired", events[0].reason)
    }

    @Test
    fun removalReasons() {
        val prev = listOf(tracked(0, region("a", 0f)))
        assertEquals("empty", LiveEventLog.diff(prev, emptyList(), emptyList()).single().reason)
        assertEquals("overlap", LiveEventLog.diff(prev, emptyList(), listOf(region("x", 20f))).single().reason)
        assertEquals("expired", LiveEventLog.diff(prev, emptyList(), listOf(region("x", 100f))).single().reason)
        assertEquals("expired", LiveEventLog.diff(prev, emptyList(), null).single().reason)
    }

    @Test
    fun unchangedRegionsAreQuiet() {
        val prev = listOf(tracked(0, region("a", 0f)))
        assertEquals(emptyList<LiveEvent>(), LiveEventLog.diff(prev, prev, listOf(region("a", 0f))))
    }

    @Test
    fun logsJsonLinesTimedFromTheStart() {
        val lines = mutableListOf<String>()
        val log = LiveEventLog { lines += it }
        log.start(1000)
        val first = listOf(tracked(4, region("say \"hi\"\n", 0.4f, null)))
        log.update(1250, emptyList(), first, result(listOf(region("a", 0f))), null, 3000)
        log.update(1500, first, first, result(listOf(region("a", 0f))), 250, null)
        log.reset(1600, first)
        assertEquals(
            listOf(
                """{"t_ms":250,"ev":"update","regions":1,"shown":1,"seen":[4],"det_ms":120,"rec_ms":85,"tr_ms":40,"w":720,"h":1280,"pts_ms":3000}""",
                """{"t_ms":250,"ev":"add","id":4,"src":"say \"hi\"\n","tr":null,"box":[0,0,40,20]}""",
                """{"t_ms":500,"ev":"update","regions":1,"shown":1,"seen":[4],"det_ms":120,"rec_ms":85,"tr_ms":40,"w":720,"h":1280,"interval_ms":250}""",
                """{"t_ms":600,"ev":"remove","id":4,"src":"say \"hi\"\n","tr":null,"box":[0,0,40,20],"reason":"reset"}""",
            ),
            lines,
        )
    }
}

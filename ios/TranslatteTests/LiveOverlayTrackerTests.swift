import XCTest
@testable import Translatte

final class LiveOverlayTrackerTests: XCTestCase {
    /// A 40x20 box at (x, 0), or `width` wide.
    private func region(_ source: String, x: Float, translated: String?? = .none, width: Float = 40, lineHeight: Float = 0) -> Region {
        Region(
            corners: [Point(x: x, y: 0), Point(x: x + width, y: 0), Point(x: x + width, y: 20), Point(x: x, y: 20)],
            sourceText: source,
            translatedText: translated ?? source.uppercased(),
            background: Rgb(r: 255, g: 255, b: 255),
            foreground: Rgb(r: 0, g: 0, b: 0),
            angle: 0,
            lineHeight: lineHeight
        )
    }

    func testTracksOnlyTranslatedRegionsWithoutAnOldMatch() {
        var t = LiveOverlayTracker()
        let out = t.update([region("a", x: 0), region("b", x: 100, translated: .some(nil))], nowMs: 0)
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].region.sourceText, "a")
        XCTAssertEqual(out[0].from, out[0].region.corners)
        XCTAssertNil(out[0].missedSinceMs)
    }

    func testTheSameTextKeepsItsIdentityAndTranslationAndMovesFromItsOldBox() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0, translated: "Exit")], nowMs: 0)[0]
        let out = t.update([region("a", x: 5, translated: "Exits")], nowMs: 1000)
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "Exit")
        XCTAssertNil(out[0].fadeFrom)
        XCTAssertEqual(out[0].from, first.region.corners)
        XCTAssertEqual(out[0].region.corners[0].x, 5)
        XCTAssertNil(out[0].missedSinceMs)
    }

    func testPreviousRegionsFollowTheCameraBeforeMatching() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0), region("b", x: 200)], nowMs: 0)
        let out = t.update([region("a", x: 30)], nowMs: 100, shift: Point(x: 30, y: 0))
        let a = out.first { $0.region.sourceText == "a" }
        XCTAssertEqual(a?.id, first[0].id)
        XCTAssertEqual(a?.from, first[0].region.corners.map { Point(x: $0.x + 30, y: $0.y) })
        XCTAssertEqual(out.first { $0.region.sourceText == "b" }?.region.corners[0].x, 230)
    }

    func testChangedTextKeepsTheBoxAndFadesToTheNewTranslation() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let out = t.update([region("b", x: 5)], nowMs: 100)
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "B")
        XCTAssertEqual(out[0].fadeFrom, "A")
        XCTAssertEqual(out[0].from, first.region.corners)
    }

    func testSimilarTextKeepsTheOldTranslationUntilTheNewOneDiffers() {
        var t = LiveOverlayTracker()
        _ = t.update([region("sortie", x: 0, translated: "Exit")], nowMs: 0)
        XCTAssertEqual(t.update([region("sortle", x: 0, translated: "Exits")], nowMs: 100)[0].region.translatedText, "Exit")
        let out = t.update([region("sortie", x: 0, translated: "Way out")], nowMs: 200)
        XCTAssertEqual(out[0].region.translatedText, "Way out")
        XCTAssertEqual(out[0].fadeFrom, "Exit")
    }

    func testPairsTheBestOverlapsFirst() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let out = t.update([region("a", x: 12), region("a", x: 1)], nowMs: 100)
        XCTAssertEqual(out.first { $0.region.corners[0].x == 1 }?.id, first.id)
        XCTAssertNotEqual(out.first { $0.region.corners[0].x == 12 }?.id, first.id)
    }

    func testAnEmptyUpdateKeepsBoxesUntilTheyExpire() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let out = t.update([], nowMs: 100)
        XCTAssertEqual(out.map(\.id), [first.id])
        XCTAssertEqual(out[0].missedSinceMs, 100)
        XCTAssertTrue(t.prune(nowMs: 600).isEmpty)
    }

    func testAMissedRegionGoesOnceTheCameraMovedFurtherThanItsLines() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0), region("b", x: 200, lineHeight: 10)], nowMs: 0)
        let out = t.update([], nowMs: 100, shift: Point(x: 0, y: 16))
        XCTAssertEqual(out.map(\.region.sourceText), ["a"])
        XCTAssertEqual(out[0].drift, Point(x: 0, y: 16))
        XCTAssertTrue(t.update([], nowMs: 200, shift: Point(x: 0, y: 15)).isEmpty)
    }

    func testMissedRegionsDrawFirstThenSmallToLarge() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 300)], nowMs: 0)
        let out = t.update([region("big", x: 0, width: 80), region("small", x: 100, width: 30)], nowMs: 100)
        XCTAssertEqual(out.map(\.region.sourceText), ["a", "small", "big"])
    }

    func testSimilarityIgnoresCaseAndSpaces() {
        XCTAssertEqual(LiveOverlayTracker.similarity("PURE GREEN", "puregreen"), 1, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.similarity("abc", "abd"), 2 / 3, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.similarity("abc", ""), 0, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.similarity(" ", ""), 1, accuracy: 1e-6)
    }

    func testARegionMissedByAnUpdateExpiresAfter500msFromThatUpdate() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let other = region("b", x: 200)
        XCTAssertEqual(t.update([other], nowMs: 1800).first { $0.region.sourceText == "a" }?.id, first.id)
        XCTAssertEqual(t.prune(nowMs: 2299).count, 2)
        XCTAssertEqual(t.prune(nowMs: 2300).map(\.region.sourceText), ["b"])
    }

    func testAMissedRegionKeepsItsFirstMissedTime() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 1000)
        let other = region("b", x: 200)
        let out = t.update([other], nowMs: 1300)
        XCTAssertEqual(out.first { $0.region.sourceText == "a" }?.missedSinceMs, 1300)
        XCTAssertNil(out.first { $0.region.sourceText == "b" }?.missedSinceMs)
        XCTAssertEqual(t.update([other], nowMs: 1400).first { $0.region.sourceText == "a" }?.missedSinceMs, 1300)
    }

    func testAMissedRegionThatIsMatchedAgainStays() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        _ = t.update([region("b", x: 200)], nowMs: 1800)
        let out = t.update([region("a", x: 0), region("b", x: 200)], nowMs: 2200)
        let a = out.first { $0.region.sourceText == "a" }
        XCTAssertEqual(a?.id, first.id)
        XCTAssertNil(a?.missedSinceMs)
        XCTAssertEqual(t.prune(nowMs: 5000).count, 2)
    }

    func testAnUnmatchedRegionIsDroppedWhenANewRegionOverlapsIt() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        XCTAssertEqual(t.update([region("b", x: 35)], nowMs: 100).map(\.region.sourceText), ["b"])
    }

    func testTouchingEdgesDoNotOverlap() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        XCTAssertEqual(t.update([region("b", x: 40)], nowMs: 100).map(\.region.sourceText), ["a", "b"])
    }

    func testOverlapRemovalAlsoCountsRegionsWithoutATranslation() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        let out = t.update([region("b", x: 35, translated: .some(nil)), region("c", x: 200)], nowMs: 100)
        XCTAssertEqual(out.map(\.region.sourceText), ["c"])
    }

    func testAMissingTranslationOfTheSameTextKeepsTheOldOne() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0, translated: "A")], nowMs: 0)[0]
        let out = t.update([region("a", x: 5, translated: .some(nil))], nowMs: 1000)
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "A")
        XCTAssertNil(out[0].missedSinceMs)
    }

    func testAMissingTranslationOfDifferentTextDoesNotTakeTheOldRegion() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0, translated: "A")], nowMs: 0)
        XCTAssertTrue(t.update([region("b", x: 5, translated: .some(nil))], nowMs: 100).isEmpty)
    }

    func testARegionSurvivesALongGapBetweenUpdates() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        XCTAssertEqual(t.prune(nowMs: 1900).count, 1)
    }

    func testLowOverlapIsNotAMatchAndDropsTheOldRegion() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let out = t.update([region("a", x: 30)], nowMs: 100)
        XCTAssertEqual(out.count, 1)
        XCTAssertNotEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].from, out[0].region.corners)
    }

    func testEachPreviousRegionMatchesOnce() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let out = t.update([region("a", x: 0), region("a", x: 2)], nowMs: 100)
        XCTAssertEqual(out.filter { $0.id == first.id }.count, 1)
        XCTAssertEqual(out.count, 2)
    }

    func testResetForgetsEverything() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        t.reset()
        XCTAssertTrue(t.regions.isEmpty)
    }

    func testIoUOfBounds() {
        let a = region("a", x: 0).corners
        XCTAssertEqual(LiveOverlayTracker.iou(a, a), 1, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.iou(a, region("a", x: 100).corners), 0, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.iou(a, region("a", x: 30).corners), 1.0 / 7.0, accuracy: 1e-6)
    }
}

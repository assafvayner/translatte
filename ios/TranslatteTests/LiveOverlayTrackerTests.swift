import XCTest
@testable import Translatte

final class LiveOverlayTrackerTests: XCTestCase {
    /// A 40x20 box at (x, 0).
    private func region(_ source: String, x: Float, translated: String?? = .none) -> Region {
        Region(
            corners: [Point(x: x, y: 0), Point(x: x + 40, y: 0), Point(x: x + 40, y: 20), Point(x: x, y: 20)],
            sourceText: source,
            translatedText: translated ?? source.uppercased(),
            background: Rgb(r: 255, g: 255, b: 255),
            foreground: Rgb(r: 0, g: 0, b: 0),
            angle: 0
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
        let first = t.update([region("a", x: 0, translated: "A")], nowMs: 0)[0]
        let out = t.update([region("a", x: 5, translated: "A2")], nowMs: 1000)
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "A")
        XCTAssertEqual(out[0].from, first.region.corners)
        XCTAssertEqual(out[0].region.corners[0].x, 5)
        XCTAssertNil(out[0].missedSinceMs)
    }

    func testChangedTextReplacesTheRegion() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)], nowMs: 0)[0]
        let out = t.update([region("b", x: 5)], nowMs: 100)
        XCTAssertEqual(out.count, 1)
        XCTAssertNotEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "B")
        XCTAssertEqual(out[0].from, out[0].region.corners)
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
        XCTAssertEqual(t.update([region("b", x: 40)], nowMs: 100).map(\.region.sourceText), ["b", "a"])
    }

    func testOverlapRemovalAlsoCountsRegionsWithoutATranslation() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        let out = t.update([region("b", x: 35, translated: .some(nil)), region("c", x: 200)], nowMs: 100)
        XCTAssertEqual(out.map(\.region.sourceText), ["c"])
    }

    func testAnUpdateWithNoRegionsClearsEverything() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)], nowMs: 0)
        XCTAssertTrue(t.update([], nowMs: 10).isEmpty)
        XCTAssertTrue(t.regions.isEmpty)
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

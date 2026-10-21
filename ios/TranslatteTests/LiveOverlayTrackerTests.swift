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

    func testTracksOnlyTranslatedRegions() {
        var t = LiveOverlayTracker()
        let out = t.update([region("a", x: 0), region("b", x: 100, translated: .some(nil))])
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].region.sourceText, "a")
        XCTAssertEqual(out[0].from, out[0].region.corners)
        XCTAssertEqual(out[0].missed, 0)
    }

    func testTheSameTextKeepsItsIdentityAndTranslationAndMovesFromItsOldBox() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0, translated: "A")])[0]
        let out = t.update([region("a", x: 5, translated: "A2")])
        XCTAssertEqual(out.count, 1)
        XCTAssertEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "A")
        XCTAssertEqual(out[0].from, first.region.corners)
        XCTAssertEqual(out[0].region.corners[0].x, 5)
    }

    func testChangedTextReplacesTheRegion() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)])[0]
        let out = t.update([region("b", x: 5)])
        XCTAssertEqual(out.count, 1)
        XCTAssertNotEqual(out[0].id, first.id)
        XCTAssertEqual(out[0].region.translatedText, "B")
        XCTAssertEqual(out[0].from, out[0].region.corners)
    }

    func testUnmatchedRegionsPersistForTwoUpdates() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)])
        XCTAssertEqual(t.update([]).map(\.missed), [1])
        XCTAssertEqual(t.update([]).map(\.missed), [2])
        XCTAssertTrue(t.update([]).isEmpty)
    }

    func testLowOverlapIsNotAMatch() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)])[0]
        let out = t.update([region("a", x: 30)])
        XCTAssertEqual(out.count, 2)
        XCTAssertNotEqual(out[0].id, first.id)
        XCTAssertEqual(out[1].id, first.id)
        XCTAssertEqual(out[1].missed, 1)
    }

    func testEachPreviousRegionMatchesOnce() {
        var t = LiveOverlayTracker()
        let first = t.update([region("a", x: 0)])[0]
        let out = t.update([region("a", x: 0), region("a", x: 2)])
        XCTAssertEqual(out.filter { $0.id == first.id }.count, 1)
        XCTAssertEqual(out.count, 2)
    }

    func testResetForgetsEverything() {
        var t = LiveOverlayTracker()
        _ = t.update([region("a", x: 0)])
        t.reset()
        XCTAssertTrue(t.update([]).isEmpty)
    }

    func testIoUOfBounds() {
        let a = region("a", x: 0).corners
        XCTAssertEqual(LiveOverlayTracker.iou(a, a), 1, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.iou(a, region("a", x: 100).corners), 0, accuracy: 1e-6)
        XCTAssertEqual(LiveOverlayTracker.iou(a, region("a", x: 30).corners), 1.0 / 7.0, accuracy: 1e-6)
    }
}

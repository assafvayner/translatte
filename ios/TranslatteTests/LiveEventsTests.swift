import XCTest
@testable import Translatte

final class LiveEventsTests: XCTestCase {
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

    private func tracked(_ id: Int, _ r: Region) -> TrackedRegion {
        TrackedRegion(id: id, region: r, from: r.corners, lastSeenMs: 0)
    }

    private func result(_ regions: [Region]) -> TranslationResult {
        TranslationResult(
            imageWidth: 720, imageHeight: 1280, regions: regions, translationError: nil,
            timings: StageTimings(detectMs: 120, recognizeMs: 85, translateMs: 40)
        )
    }

    func testDiffReportsAddsMovesRetextsAndRemovals() {
        let prev = [tracked(0, region("a", x: 0)), tracked(1, region("b", x: 100)), tracked(2, region("c", x: 200))]
        let next = [tracked(0, region("a", x: 5)), tracked(1, region("b", x: 100, translated: "B2")), tracked(3, region("d", x: 300))]
        let events = LiveEventLog.diff(prev: prev, next: next, incoming: [region("a", x: 5), region("d", x: 300)])
        XCTAssertEqual(events.map(\.ev), ["remove", "move", "retext", "add"])
        XCTAssertEqual(events.map(\.tracked.id), [2, 0, 1, 3])
        XCTAssertEqual(events[0].reason, "expired")
    }

    func testRemovalReasons() {
        let prev = [tracked(0, region("a", x: 0))]
        XCTAssertEqual(LiveEventLog.diff(prev: prev, next: [], incoming: []).first?.reason, "empty")
        XCTAssertEqual(LiveEventLog.diff(prev: prev, next: [], incoming: [region("x", x: 20)]).first?.reason, "overlap")
        XCTAssertEqual(LiveEventLog.diff(prev: prev, next: [], incoming: [region("x", x: 100)]).first?.reason, "expired")
        XCTAssertEqual(LiveEventLog.diff(prev: prev, next: [], incoming: nil).first?.reason, "expired")
    }

    func testUnchangedRegionsAreQuiet() {
        let prev = [tracked(0, region("a", x: 0))]
        XCTAssertEqual(LiveEventLog.diff(prev: prev, next: prev, incoming: [region("a", x: 0)]), [])
    }

    func testLogsJsonLinesTimedFromTheStart() {
        var lines: [String] = []
        let log = LiveEventLog { lines.append($0) }
        log.start(nowMs: 1000)
        let first = [tracked(4, region("say \"hi\"\n", x: 0.4, translated: .some(nil)))]
        log.update(nowMs: 1250, prev: [], next: first, result: result([region("a", x: 0)]), intervalMs: nil, ptsMs: 3000)
        log.update(nowMs: 1500, prev: first, next: first, result: result([region("a", x: 0)]), intervalMs: 250, ptsMs: nil)
        log.reset(nowMs: 1600, prev: first)
        XCTAssertEqual(lines, [
            #"{"t_ms":250,"ev":"update","regions":1,"shown":1,"seen":[],"det_ms":120,"rec_ms":85,"tr_ms":40,"w":720,"h":1280,"pts_ms":3000}"#,
            #"{"t_ms":250,"ev":"add","id":4,"src":"say \"hi\"\n","tr":null,"box":[0,0,40,20]}"#,
            #"{"t_ms":500,"ev":"update","regions":1,"shown":1,"seen":[],"det_ms":120,"rec_ms":85,"tr_ms":40,"w":720,"h":1280,"interval_ms":250}"#,
            #"{"t_ms":600,"ev":"remove","id":4,"src":"say \"hi\"\n","tr":null,"box":[0,0,40,20],"reason":"reset"}"#,
        ])
    }
}

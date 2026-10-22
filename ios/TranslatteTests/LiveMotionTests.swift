import XCTest
@testable import Translatte

final class LiveMotionTests: XCTestCase {
    /// A fake thumbnail of a scene at `x` thumbnail pixels; `sure: false` can't be measured.
    private func thumb(_ x: UInt8, sure: Bool = true) -> Data {
        Data([x, sure ? 0 : 1])
    }

    /// Measures the difference of the scene positions up to 20, unless either thumbnail is unsure.
    private func motion() -> LiveMotion {
        LiveMotion { ref in
            guard ref[1] == 0 else { return nil }
            return { cur in
                let d = Float(cur[0]) - Float(ref[0])
                return cur[1] == 0 && abs(d) <= 20 ? Point(x: d, y: 0) : nil
            }
        }
    }

    /// A frame twice the thumbnail's size, so frame pixels are double thumbnail pixels.
    private func source(_ x: UInt8, sure: Bool = true) -> Thumb {
        Thumb(pixels: thumb(x, sure: sure), width: 2 * LiveRules.thumbSide, height: LiveRules.thumbSide)
    }

    func testMeasuresFramesAgainstTheReferenceInFramePixels() {
        let m = motion()
        XCTAssertFalse(m.frame(thumb(3)))
        XCTAssertNil(m.rebase(source(0)))
        XCTAssertEqual(m.shift, Point(x: 6, y: 0))
        XCTAssertTrue(m.frame(thumb(5)))
        XCTAssertEqual(m.shift, Point(x: 10, y: 0))
    }

    func testAnUnsureFrameKeepsTheLastShift() {
        let m = motion()
        _ = m.rebase(source(0))
        _ = m.frame(thumb(5))
        XCTAssertFalse(m.frame(thumb(9, sure: false)))
        XCTAssertEqual(m.shift, Point(x: 10, y: 0))
    }

    func testMotionTooFarToMeasureAddsOnAlongTheChainOfFrames() {
        let m = motion()
        _ = m.rebase(source(0))
        _ = m.frame(thumb(10))
        XCTAssertTrue(m.frame(thumb(25)))
        XCTAssertEqual(m.shift, Point(x: 50, y: 0))
        XCTAssertTrue(m.frame(thumb(40)))
        XCTAssertEqual(m.shift, Point(x: 80, y: 0))
        XCTAssertFalse(m.frame(thumb(41, sure: false)))
        XCTAssertFalse(m.frame(thumb(42)))
        XCTAssertEqual(m.shift, Point(x: 80, y: 0))
        XCTAssertTrue(m.frame(thumb(18)))
        XCTAssertEqual(m.shift, Point(x: 36, y: 0))
    }

    func testARecentSourceIsFollowedAlongTheChain() {
        let m = motion()
        _ = m.frame(thumb(0))
        _ = m.frame(thumb(15))
        _ = m.frame(thumb(30))
        XCTAssertNil(m.rebase(source(0)))
        XCTAssertEqual(m.shift, Point(x: 60, y: 0))
        XCTAssertTrue(m.frame(thumb(45)))
        XCTAssertEqual(m.shift, Point(x: 90, y: 0))
        XCTAssertEqual(m.rebase(source(30)), Point(x: 60, y: 0))
        XCTAssertEqual(m.shift, Point(x: 30, y: 0))
    }

    func testRebaseReturnsTheMotionBetweenSources() {
        let m = motion()
        _ = m.rebase(source(0))
        _ = m.frame(thumb(7))
        XCTAssertEqual(m.rebase(source(4)), Point(x: 8, y: 0))
        XCTAssertEqual(m.shift, Point(x: 6, y: 0))
    }

    func testAnUnsureLatestFrameFollowsFromTheMotionBetweenSources() {
        let m = motion()
        _ = m.rebase(source(0))
        _ = m.frame(thumb(7))
        _ = m.frame(thumb(7, sure: false))
        _ = m.rebase(source(4))
        XCTAssertEqual(m.shift, Point(x: 6, y: 0))
    }

    func testAnUnmeasurableSourceHasNoMotion() {
        let m = motion()
        _ = m.rebase(source(0))
        _ = m.frame(thumb(7))
        XCTAssertNil(m.rebase(source(4, sure: false)))
        XCTAssertEqual(m.shift, LiveMotion.zero)
        XCTAssertFalse(m.frame(thumb(9)))
    }

    func testResetForgetsTheReference() {
        let m = motion()
        _ = m.rebase(source(0))
        _ = m.frame(thumb(7))
        m.reset()
        XCTAssertEqual(m.shift, LiveMotion.zero)
        XCTAssertFalse(m.frame(thumb(9)))
    }
}

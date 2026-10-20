import CoreGraphics
import XCTest
@testable import Translatte

final class RegionHitTestTests: XCTestCase {
    private func box(_ x: Float, _ y: Float, _ w: Float, _ h: Float, translated: String? = "t") -> Region {
        Region(
            corners: [Point(x: x, y: y), Point(x: x + w, y: y), Point(x: x + w, y: y + h), Point(x: x, y: y + h)],
            sourceText: "s",
            translatedText: translated,
            background: Rgb(r: 0, g: 0, b: 0),
            foreground: Rgb(r: 255, g: 255, b: 255),
            angle: 0
        )
    }

    func testFindsTheBoxUnderThePoint() {
        let regions = [box(0, 0, 100, 20), box(0, 40, 100, 20)]
        XCTAssertEqual(RegionHitTest.region(at: CGPoint(x: 50, y: 10), in: regions), 0)
        XCTAssertEqual(RegionHitTest.region(at: CGPoint(x: 50, y: 50), in: regions), 1)
        XCTAssertNil(RegionHitTest.region(at: CGPoint(x: 50, y: 30), in: regions))
    }

    func testIgnoresUntranslatedRegions() {
        XCTAssertNil(RegionHitTest.region(at: CGPoint(x: 10, y: 10), in: [box(0, 0, 100, 20, translated: nil)]))
    }

    func testPrefersTheTopmostOverlappingBox() {
        let regions = [box(0, 0, 100, 100), box(10, 10, 20, 20)]
        XCTAssertEqual(RegionHitTest.region(at: CGPoint(x: 15, y: 15), in: regions), 1)
    }

    func testRotatedBox() {
        let diamond = [CGPoint(x: 50, y: 0), CGPoint(x: 100, y: 50), CGPoint(x: 50, y: 100), CGPoint(x: 0, y: 50)]
        XCTAssertTrue(RegionHitTest.contains(diamond, CGPoint(x: 50, y: 50)))
        XCTAssertFalse(RegionHitTest.contains(diamond, CGPoint(x: 5, y: 5)))
    }
}

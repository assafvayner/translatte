import CoreGraphics
import XCTest
@testable import Translatte

final class ZoomTransformTests: XCTestCase {
    private let container = CGSize(width: 300, height: 600)

    func testClampKeepsScaleBetweenFitAndSixTimes() {
        var zoom = ZoomTransform(scale: 10, offset: .zero)
        zoom.clamp(content: container, container: container)
        XCTAssertEqual(zoom.scale, 6)
        zoom.scale = 0.5
        zoom.clamp(content: container, container: container)
        XCTAssertEqual(zoom.scale, 1)
    }

    func testClampKeepsTheImageEdgesOutsideTheContainer() {
        var zoom = ZoomTransform(scale: 2, offset: CGSize(width: 500, height: -500))
        zoom.clamp(content: CGSize(width: 300, height: 400), container: container)
        XCTAssertEqual(zoom.offset, CGSize(width: 150, height: -100))
    }

    func testFitImageCannotPan() {
        var zoom = ZoomTransform(scale: 1, offset: CGSize(width: 40, height: 40))
        zoom.clamp(content: CGSize(width: 300, height: 400), container: container)
        XCTAssertEqual(zoom.offset, .zero)
        XCTAssertFalse(zoom.isZoomed)
    }

    func testDoubleTapZoomsAroundTheTapThenBackToFit() {
        var zoom = ZoomTransform()
        let tap = CGPoint(x: 200, y: 400)
        zoom.toggleDoubleTap(at: tap, content: container, container: container)
        XCTAssertEqual(zoom.scale, 2.5)
        XCTAssertEqual(zoom.offset, CGSize(width: -75, height: -150))
        XCTAssertEqual(zoom.unzoomed(tap, container: container), tap, "the tapped point stays under the finger")
        XCTAssertTrue(zoom.isZoomed)
        XCTAssertEqual(zoom.percent, 250)
        zoom.toggleDoubleTap(at: tap, content: container, container: container)
        XCTAssertEqual(zoom, ZoomTransform())
    }

    func testDoubleTapNearAnEdgeIsClamped() {
        var zoom = ZoomTransform()
        zoom.toggleDoubleTap(at: CGPoint(x: 150, y: 0), content: CGSize(width: 300, height: 400), container: container)
        XCTAssertEqual(zoom.offset, CGSize(width: 0, height: 200))
    }

    func testUnzoomedMapsBackThroughScaleAndOffset() {
        let zoom = ZoomTransform(scale: 2, offset: CGSize(width: 10, height: -20))
        XCTAssertEqual(zoom.unzoomed(CGPoint(x: 160, y: 280), container: container), CGPoint(x: 150, y: 300))
    }
}

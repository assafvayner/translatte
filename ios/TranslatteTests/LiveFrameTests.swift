import UIKit
import XCTest
@testable import Translatte

final class LiveFrameTests: XCTestCase {
    /// A 2x1 image: red on the left, blue on the right.
    private func redBlue() -> UIImage {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: CGSize(width: 2, height: 1), format: format).image { ctx in
            UIColor.red.setFill()
            ctx.fill(CGRect(x: 0, y: 0, width: 1, height: 1))
            UIColor.blue.setFill()
            ctx.fill(CGRect(x: 1, y: 0, width: 1, height: 1))
        }
    }

    func testAStillBecomesAnUprightRgbaFrame() throws {
        let frame = try XCTUnwrap(LiveFrame(image: redBlue()))
        XCTAssertEqual(frame.width, 2)
        XCTAssertEqual(frame.height, 1)
        XCTAssertEqual(frame.rotation, 0)
        let bytes = [UInt8](frame.rgba)
        XCTAssertEqual(bytes[0], 255, "red first")
        XCTAssertEqual(bytes[2], 0)
        XCTAssertEqual(bytes[4], 0)
        XCTAssertEqual(bytes[6], 255, "then blue")
    }

    func testTheFrozenImageIsUpright() throws {
        let frame = LiveFrame(rgba: Data(count: 2 * 1 * 4), width: 2, height: 1, rotation: 90)
        let image = try XCTUnwrap(frame.image())
        XCTAssertEqual(image.size, CGSize(width: 1, height: 2))
    }
}

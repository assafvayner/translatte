import CoreMedia
import UIKit
import XCTest
@testable import Translatte

final class TranslationRunsTests: XCTestCase {
    private let photo = UIImage()

    private func output(regions: Int, error: String? = nil) -> TranslationResult {
        let region = Region(
            corners: [Point(x: 0, y: 0), Point(x: 10, y: 0), Point(x: 10, y: 10), Point(x: 0, y: 10)],
            sourceText: "s",
            translatedText: "t",
            background: Rgb(r: 0, g: 0, b: 0),
            foreground: Rgb(r: 255, g: 255, b: 255),
            angle: 0
        )
        return TranslationResult(imageWidth: 10, imageHeight: 10, regions: Array(repeating: region, count: regions), translationError: error)
    }

    func testRetriesCountAttemptsAndANewPhotoResetsThem() {
        var runs = TranslationRuns()
        _ = runs.start()
        XCTAssertEqual(runs.attempt, 0)
        _ = runs.start(retry: true)
        _ = runs.start(retry: true)
        XCTAssertEqual(runs.attempt, 2)
        _ = runs.start()
        XCTAssertEqual(runs.attempt, 0)
    }

    func testANewRunSupersedesTheRunningOne() {
        var runs = TranslationRuns()
        let first = runs.start()
        let second = runs.start()
        XCTAssertFalse(runs.isCurrent(first))
        XCTAssertNil(runs.finish(first, image: photo, output: output(regions: 1)), "stale results are dropped")
        XCTAssertNil(runs.lastSuccess)
        XCTAssertEqual(runs.finish(second, image: photo, output: output(regions: 1)), .done)
        XCTAssertNotNil(runs.lastSuccess)
    }

    func testFailuresKeepTheLastSuccess() {
        var runs = TranslationRuns()
        let success = output(regions: 2)
        _ = runs.finish(runs.start(), image: photo, output: success)
        XCTAssertEqual(
            runs.finish(runs.start(), image: UIImage(), output: output(regions: 0)),
            .failed(TranslationRuns.noTextMessage)
        )
        XCTAssertEqual(
            runs.finish(runs.start(), image: UIImage(), output: output(regions: 1, error: "boom")),
            .failed("Translation failed: boom")
        )
        XCTAssertTrue(runs.lastSuccess?.image === photo)
        XCTAssertEqual(runs.lastSuccess?.result.regions.count, 2)
    }

    func testPhotosDecodeToTheOCRSizeWithOrientationApplied() throws {
        let size = CGSize(width: 4000, height: 3000)
        let image = UIGraphicsImageRenderer(size: size, format: { let f = UIGraphicsImageRendererFormat(); f.scale = 1; return f }())
            .image { ctx in UIColor.white.setFill(); ctx.fill(CGRect(origin: .zero, size: size)) }
        let jpeg = try XCTUnwrap(image.jpegData(compressionQuality: 0.8))
        let decoded = try XCTUnwrap(PhotoDecoder.preparedForOCR(jpeg))
        XCTAssertEqual(decoded.size, CGSize(width: 2048, height: 1536))
        XCTAssertEqual(decoded.imageOrientation, .up)
        XCTAssertNil(PhotoDecoder.preparedForOCR(Data("not an image".utf8)))
    }

    func testSmallPhotosKeepTheirSize() throws {
        let image = UIGraphicsImageRenderer(size: CGSize(width: 300, height: 200), format: { let f = UIGraphicsImageRendererFormat(); f.scale = 1; return f }())
            .image { ctx in UIColor.white.setFill(); ctx.fill(CGRect(x: 0, y: 0, width: 300, height: 200)) }
        let decoded = try XCTUnwrap(PhotoDecoder.preparedForOCR(try XCTUnwrap(image.pngData())))
        XCTAssertEqual(decoded.size, CGSize(width: 300, height: 200))
    }

    func testCaptureIsCappedNearTwelveMegapixels() {
        let sizes = [(4032, 3024), (8064, 6048), (1920, 1440)].map { CMVideoDimensions(width: Int32($0.0), height: Int32($0.1)) }
        let chosen = CameraController.captureDimensions(sizes)
        XCTAssertEqual(chosen?.width, 4032)
        XCTAssertEqual(CameraController.captureDimensions([CMVideoDimensions(width: 8064, height: 6048)])?.width, 8064)
    }
}

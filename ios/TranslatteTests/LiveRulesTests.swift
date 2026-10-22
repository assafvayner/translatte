import XCTest
@testable import Translatte

final class LiveRulesTests: XCTestCase {
    private let noTimings = StageTimings(detectMs: 0, recognizeMs: 0, translateMs: 0)

    private func result(_ translated: String?, error: String? = nil) -> TranslationResult {
        let region = Region(
            corners: [Point(x: 0, y: 0), Point(x: 1, y: 0), Point(x: 1, y: 1), Point(x: 0, y: 1)],
            sourceText: "s",
            translatedText: translated,
            background: Rgb(r: 0, g: 0, b: 0),
            foreground: Rgb(r: 0, g: 0, b: 0),
            angle: 0
        )
        return TranslationResult(imageWidth: 10, imageHeight: 10, regions: [region], translationError: error, timings: noTimings)
    }

    func testActiveOnlyWhenEnabledAndTheSourceIsInstalled() {
        XCTAssertTrue(LiveRules.isActive(enabled: true, sourceInstalled: true))
        XCTAssertFalse(LiveRules.isActive(enabled: false, sourceInstalled: true))
        XCTAssertFalse(LiveRules.isActive(enabled: true, sourceInstalled: false))
    }

    func testTakesAFrameWhenIdleAndOnAHotDeviceAtMostOncePerSecond() {
        XCTAssertFalse(LiveRules.shouldTake(busy: true, hot: false, nowMs: 5000, lastStartMs: 0))
        XCTAssertTrue(LiveRules.shouldTake(busy: false, hot: false, nowMs: 10, lastStartMs: 0))
        XCTAssertTrue(LiveRules.shouldTake(busy: false, hot: true, nowMs: 10, lastStartMs: nil))
        XCTAssertFalse(LiveRules.shouldTake(busy: false, hot: true, nowMs: 999, lastStartMs: 0))
        XCTAssertTrue(LiveRules.shouldTake(busy: false, hot: true, nowMs: 1000, lastStartMs: 0))
    }

    func testBacksOffAfterFailuresUpTo8Seconds() {
        XCTAssertEqual([0, 1, 2, 3, 4, 5, 40].map(LiveRules.backoffMs), [0, 1000, 2000, 4000, 8000, 8000, 8000])
        XCTAssertFalse(LiveRules.shouldTake(busy: false, hot: false, nowMs: 1500, lastStartMs: 0, failures: 1, lastFailMs: 1000))
        XCTAssertTrue(LiveRules.shouldTake(busy: false, hot: false, nowMs: 2000, lastStartMs: 0, failures: 1, lastFailMs: 1000))
        XCTAssertFalse(LiveRules.shouldTake(busy: false, hot: false, nowMs: 4999, lastStartMs: 0, failures: 3, lastFailMs: 1000))
        XCTAssertTrue(LiveRules.shouldTake(busy: false, hot: false, nowMs: 5000, lastStartMs: 0, failures: 0, lastFailMs: 4999))
    }

    func testDownscalesTheLongSideTo1280OrLess() {
        XCTAssertEqual(LiveRules.downscaleFactor(width: 960, height: 720), 1)
        XCTAssertEqual(LiveRules.downscaleFactor(width: 1280, height: 960), 1)
        XCTAssertEqual(LiveRules.downscaleFactor(width: 1920, height: 1440), 2)
        XCTAssertEqual(LiveRules.downscaleFactor(width: 4032, height: 3024), 4)
    }

    func testSnapUsesTheLiveResultOnlyWithATranslation() {
        XCTAssertFalse(LiveRules.snapUsesLive(nil))
        XCTAssertFalse(LiveRules.snapUsesLive(result(nil)))
        XCTAssertFalse(LiveRules.snapUsesLive(result("t", error: "boom")))
        var empty = result("t")
        empty.regions = []
        XCTAssertFalse(LiveRules.snapUsesLive(empty))
        XCTAssertTrue(LiveRules.snapUsesLive(result("t")))
    }

    func testSkippedFollowsTheEngine() {
        XCTAssertFalse(LiveRules.skipped(result("t")))
        var skipped = result(nil)
        skipped.regions = []
        skipped.skipped = true
        XCTAssertTrue(LiveRules.skipped(skipped))
    }

    func testReadoutShowsTheRateAndStageTimes() {
        XCTAssertEqual(
            LiveRules.readout(intervalMs: 400, timings: StageTimings(detectMs: 120, recognizeMs: 85, translateMs: 40, precision: "fp16")),
            "fp16 · 2.5/s · det 120 · rec 85 · tr 40 ms"
        )
        XCTAssertEqual(LiveRules.readout(intervalMs: nil, timings: noTimings), "fp32 · –/s · det 0 · rec 0 · tr 0 ms")
    }

    func testFillCentersAndCropsLikeThePreview() {
        XCTAssertEqual(LiveRules.fill(imageWidth: 720, imageHeight: 960, viewWidth: 1080, viewHeight: 2400),
                       FillTransform(scale: 2.5, left: -360, top: 0))
        XCTAssertEqual(LiveRules.fill(imageWidth: 100, imageHeight: 100, viewWidth: 400, viewHeight: 200),
                       FillTransform(scale: 4, left: 0, top: -100))
    }

    /// A 2x2 frame with 4 bytes of row padding: pixel n is (n, 10+n, 20+n, 30+n).
    private func padded() -> [UInt8] {
        var bytes = [UInt8](repeating: 0, count: 24)
        for y in 0..<2 {
            for x in 0..<2 {
                for c in 0..<4 { bytes[y * 12 + x * 4 + c] = UInt8(c * 10 + y * 2 + x) }
            }
        }
        return bytes
    }

    private func pack(_ bytes: [UInt8], width: Int, height: Int, rowStride: Int, factor: Int, swap: Bool) -> [UInt8] {
        bytes.withUnsafeBytes {
            [UInt8](LiveRules.packRgba($0, width: width, height: height, rowStride: rowStride, factor: factor, swapRedBlue: swap))
        }
    }

    func testPacksRowsWithoutPadding() {
        XCTAssertEqual(pack(padded(), width: 2, height: 2, rowStride: 12, factor: 1, swap: false),
                       [0, 10, 20, 30, 1, 11, 21, 31, 2, 12, 22, 32, 3, 13, 23, 33])
    }

    func testSwapsRedAndBlueForBgraFrames() {
        XCTAssertEqual(pack(padded(), width: 2, height: 2, rowStride: 12, factor: 1, swap: true),
                       [20, 10, 0, 30, 21, 11, 1, 31, 22, 12, 2, 32, 23, 13, 3, 33])
    }

    func testKeepsEveryNthPixelOfEveryNthRow() {
        XCTAssertEqual(pack(padded(), width: 2, height: 2, rowStride: 12, factor: 2, swap: false), [0, 10, 20, 30])
    }

    func testATightlyPackedFrameIsCopiedAsIs() {
        let bytes = (0..<16).map(UInt8.init)
        XCTAssertEqual(pack(bytes, width: 2, height: 2, rowStride: 8, factor: 1, swap: false), bytes)
    }

    func testAFrameIsReadWhileTheLastIsTranslated() {
        var stages = LiveStages<String>()
        XCTAssertFalse(stages.readBusy)
        stages.startRead()
        XCTAssertTrue(stages.readBusy)
        XCTAssertEqual(stages.readDone("a"), "a")
        XCTAssertTrue(stages.translating)
        XCTAssertFalse(stages.readBusy)
        stages.startRead()
        XCTAssertNil(stages.readDone("b"))
        XCTAssertTrue(stages.readBusy, "b waits, so no new read")
        XCTAssertEqual(stages.translateDone(), "b")
        XCTAssertTrue(stages.translating)
        XCTAssertFalse(stages.readBusy)
        XCTAssertNil(stages.translateDone())
        XCTAssertFalse(stages.translating)
    }

    func testAResetDropsTheWaitingFrame() {
        var stages = LiveStages<String>()
        stages.startRead()
        _ = stages.readDone("a")
        stages.startRead()
        _ = stages.readDone("b")
        stages.reset()
        XCTAssertFalse(stages.readBusy)
        XCTAssertNil(stages.translateDone())
        stages.startRead()
        stages.readFailed()
        XCTAssertFalse(stages.readBusy)
    }

    private func corners(of frame: [UInt8], width: Int, rotation: Int) -> [UInt8] {
        let thumb = frame.withUnsafeBytes {
            LiveRules.thumbnail($0, width: width, height: frame.count / 4 / width, rowStride: width * 4, rotation: rotation)
        }
        let s = LiveRules.thumbSide
        XCTAssertEqual(thumb.count, s * s)
        return [thumb[0], thumb[s - 1], thumb[s * (s - 1)], thumb[s * s - 1]]
    }

    func testThumbnailsAreUprightGrayscaleSquares() {
        let frame: [UInt8] = [10, 10, 10, 0, 20, 20, 20, 0, 30, 30, 30, 0, 40, 40, 40, 0]
        XCTAssertEqual(corners(of: frame, width: 2, rotation: 0), [10, 20, 30, 40])
        XCTAssertEqual(corners(of: frame, width: 2, rotation: 90), [30, 10, 40, 20])
        XCTAssertEqual(corners(of: frame, width: 2, rotation: 180), [40, 30, 20, 10])
        XCTAssertEqual(corners(of: frame, width: 2, rotation: 270), [20, 40, 10, 30])
    }

    func testThumbnailsWeighGreenTwice() {
        XCTAssertEqual(corners(of: [40, 80, 120, 0], width: 1, rotation: 0), [80, 80, 80, 80])
    }

    func testANewBoxShowsItsTextAtOnceAndARetextFadesItIn() {
        var tracker = LiveOverlayTracker()
        let first = result("hello").regions
        let added = tracker.update(first, nowMs: 0)
        XCTAssertNil(LiveRules.textFadeInDelayMs(added[0]))
        var changed = first
        changed[0].translatedText = "goodbye"
        changed[0].sourceText = "t"
        let retexted = tracker.update(changed, nowMs: 100)
        XCTAssertEqual(retexted[0].id, added[0].id)
        XCTAssertEqual(LiveRules.textFadeInDelayMs(retexted[0]), LiveRules.moveMs / 2)
        XCTAssertNil(LiveRules.textFadeInDelayMs(tracker.update(changed, nowMs: 200)[0]))
    }
}

import CoreGraphics
import Foundation

/// Places an upright frame of image pixels over the preview: view = image * scale + (left, top).
struct FillTransform: Equatable {
    let scale: CGFloat
    let left: CGFloat
    let top: CGFloat
}

/// The two stages of live translation, the same as Android's `LiveStages`: reading a frame (OCR)
/// and translating what was read, each with at most one frame. A frame read while the translator
/// is busy waits for it; no new frame is read while one waits, so none goes stale.
struct LiveStages<T> {
    private(set) var reading = false
    private(set) var translating = false
    private var waiting: T?

    /// Whether a new frame can't be read yet (pass as `busy` to `LiveRules.shouldTake`).
    var readBusy: Bool { reading || waiting != nil }

    mutating func startRead() {
        reading = true
    }

    /// A frame was read: returns it to translate now, or nil when it waits for the translator.
    mutating func readDone(_ item: T) -> T? {
        reading = false
        if translating {
            waiting = item
            return nil
        }
        translating = true
        return item
    }

    mutating func readFailed() {
        reading = false
    }

    /// A translation finished or failed: returns the waiting frame to translate next, if any.
    mutating func translateDone() -> T? {
        let next = waiting
        waiting = nil
        translating = next != nil
        return next
    }

    /// Drops the waiting frame; frames in a stage finish and are ignored by their caller.
    mutating func reset() {
        waiting = nil
    }
}

/// Live translation rules, the same as Android's `LiveRules`; keep the two in step.
enum LiveRules {
    /// UserDefaults key of the "Live translation (experimental)" setting; on by default.
    static let storageKey = "liveTranslation"
    /// UserDefaults key of the debug "Live OCR precision" setting: fp32 reads live frames with the
    /// photo models instead of the fp16 ones.
    static let fp32StorageKey = "liveOcrFp32"
    /// Frames reach the engine at most this large on their long side.
    static let maxFrameSide = 1280
    /// Minimum time between frame starts while the device is hot.
    static let hotIntervalMs: Int64 = 1000
    /// Wait after the first failed frame; doubles with each further failure in a row.
    static let backoffBaseMs: Int64 = 1000
    /// The longest wait after failed frames.
    static let backoffMaxMs: Int64 = 8000
    /// How long a tracked box takes to slide to its new place.
    static let moveMs = 150
    /// How often stale boxes are pruned while updates stall.
    static let pruneIntervalMs: Int64 = 100
    /// Live translations shrink to fit their box down to this text size, then cut off.
    static let minTextSize: CGFloat = 9
    /// Motion thumbnails are this many pixels on each side, whatever the frame's aspect.
    static let thumbSide = 128

    static func isActive(enabled: Bool, sourceInstalled: Bool) -> Bool {
        enabled && sourceInstalled
    }

    /// The wait after `failures` frames failed in a row: none for 0, then `backoffBaseMs` doubling up to `backoffMaxMs`.
    static func backoffMs(_ failures: Int) -> Int64 {
        failures <= 0 ? 0 : min(backoffMaxMs, backoffBaseMs << Int64(min(failures - 1, 16)))
    }

    /// Start a frame when none is in flight and the backoff for `failures` has passed since `lastFailMs`;
    /// on a hot device, also wait a second after the last start.
    static func shouldTake(
        busy: Bool, hot: Bool, nowMs: Int64, lastStartMs: Int64?, failures: Int = 0, lastFailMs: Int64? = nil
    ) -> Bool {
        guard !busy else { return false }
        if let lastFailMs, nowMs - lastFailMs < backoffMs(failures) { return false }
        guard hot, let lastStartMs else { return true }
        return nowMs - lastStartMs >= hotIntervalMs
    }

    /// The pixel step that brings the long side within `maxFrameSide`.
    static func downscaleFactor(width: Int, height: Int) -> Int {
        max(1, (max(width, height) + maxFrameSide - 1) / maxFrameSide)
    }

    /// Whether the engine skipped `result`'s frame as too blurred to read: no update rather than an
    /// empty one. Always false until the engine reports skipped frames.
    static func skipped(_ result: TranslationResult) -> Bool {
        false
    }

    /// Snap freezes the live result only when it translated something; otherwise the photo pipeline runs.
    static func snapUsesLive(_ result: TranslationResult?) -> Bool {
        guard let result, result.translationError == nil else { return false }
        return result.regions.contains { $0.translatedText != nil }
    }

    /// Debug readout, e.g. "fp16 · 2.5/s · det 120 · rec 85 · tr 40 ms"; `intervalMs` is the time since the previous update.
    static func readout(intervalMs: Int64?, timings: StageTimings) -> String {
        let rate = intervalMs.flatMap { $0 > 0 ? String(format: "%.1f", 1000.0 / Double($0)) : nil } ?? "–"
        return "\(timings.precision) · \(rate)/s · det \(timings.detectMs) · rec \(timings.recognizeMs) · tr \(timings.translateMs) ms"
    }

    /// Scales an image to cover the view and centers it, cropping the overflow, as the camera preview does.
    static func fill(imageWidth: CGFloat, imageHeight: CGFloat, viewWidth: CGFloat, viewHeight: CGFloat) -> FillTransform {
        let scale = max(viewWidth / max(imageWidth, 1), viewHeight / max(imageHeight, 1))
        return FillTransform(scale: scale, left: (viewWidth - imageWidth * scale) / 2, top: (viewHeight - imageHeight * scale) / 2)
    }

    /// Copies a 4-byte-per-pixel frame whose rows are `rowStride` bytes apart into tightly packed
    /// RGBA, keeping every `factor`th pixel of every `factor`th row (output: width / factor by
    /// height / factor). `swapRedBlue` reads BGRA.
    static func packRgba(
        _ src: UnsafeRawBufferPointer, width: Int, height: Int, rowStride: Int, factor: Int, swapRedBlue: Bool
    ) -> Data {
        let w = width / factor, h = height / factor
        if factor == 1, !swapRedBlue, rowStride == width * 4 {
            return Data(src.prefix(w * h * 4))
        }
        let r = swapRedBlue ? 2 : 0, b = 2 - r
        var out = Data(count: w * h * 4)
        out.withUnsafeMutableBytes { dst in
            var o = 0
            for y in 0..<h {
                let row = y * factor * rowStride
                for x in 0..<w {
                    let i = row + x * factor * 4
                    dst[o] = src[i + r]
                    dst[o + 1] = src[i + 1]
                    dst[o + 2] = src[i + b]
                    dst[o + 3] = src[i + 3]
                    o += 4
                }
            }
        }
        return out
    }

    /// A `thumbSide`-square grayscale thumbnail of the upright frame, to follow camera motion: a
    /// `width` x `height` frame of 4 bytes per pixel (RGBA or BGRA) whose rows are `rowStride` bytes
    /// apart, turned clockwise by `rotation` degrees. Each pixel averages four samples of its cell.
    static func thumbnail(_ src: UnsafeRawBufferPointer, width: Int, height: Int, rowStride: Int, rotation: Int) -> Data {
        let side = thumbSide
        let turned = rotation % 180 != 0
        let uprightWidth = turned ? height : width
        let uprightHeight = turned ? width : height
        var out = Data(count: side * side)
        out.withUnsafeMutableBytes { dst in
            var o = 0
            for v in 0..<side {
                for u in 0..<side {
                    var sum = 0
                    for k in 0..<4 {
                        let ux = (u * 4 + 1 + 2 * (k & 1)) * uprightWidth / (side * 4)
                        let uy = (v * 4 + 1 + 2 * (k >> 1)) * uprightHeight / (side * 4)
                        let x: Int, y: Int
                        switch rotation {
                        case 90: (x, y) = (uy, height - 1 - ux)
                        case 180: (x, y) = (width - 1 - ux, height - 1 - uy)
                        case 270: (x, y) = (width - 1 - uy, ux)
                        default: (x, y) = (ux, uy)
                        }
                        let i = y * rowStride + x * 4
                        sum += Int(src[i]) + 2 * Int(src[i + 1]) + Int(src[i + 2])
                    }
                    dst[o] = UInt8(sum / 16)
                    o += 1
                }
            }
        }
        return out
    }
}

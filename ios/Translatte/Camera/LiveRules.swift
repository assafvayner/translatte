import CoreGraphics
import Foundation

/// Places an upright frame of image pixels over the preview: view = image * scale + (left, top).
struct FillTransform: Equatable {
    let scale: CGFloat
    let left: CGFloat
    let top: CGFloat
}

/// Live translation rules, the same as Android's `LiveRules`; keep the two in step.
enum LiveRules {
    /// UserDefaults key of the "Live translation (experimental)" setting; on by default.
    static let storageKey = "liveTranslation"
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

    /// Snap freezes the live result only when it translated something; otherwise the photo pipeline runs.
    static func snapUsesLive(_ result: TranslationResult?) -> Bool {
        guard let result, result.translationError == nil else { return false }
        return result.regions.contains { $0.translatedText != nil }
    }

    /// Debug readout, e.g. "2.5/s · det 120 · rec 85 · tr 40 ms"; `intervalMs` is the time since the previous update.
    static func readout(intervalMs: Int64?, timings: StageTimings) -> String {
        let rate = intervalMs.flatMap { $0 > 0 ? String(format: "%.1f", 1000.0 / Double($0)) : nil } ?? "–"
        return "\(rate)/s · det \(timings.detectMs) · rec \(timings.recognizeMs) · tr \(timings.translateMs) ms"
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
}

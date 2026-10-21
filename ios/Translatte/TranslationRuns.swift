import ImageIO
import UIKit

/// A successful translation, kept in memory for Home's last-result card.
struct LastResult {
    let image: UIImage
    let result: TranslationResult
}

/// Tracks which translation run is current, so a new photo supersedes a running one and its
/// stale results are dropped, and keeps the last successful result apart from failures.
struct TranslationRuns {
    private(set) var current = 0
    private(set) var lastSuccess: LastResult?

    /// Starts a run, superseding any earlier one.
    mutating func start() -> Int {
        current += 1
        return current
    }

    func isCurrent(_ run: Int) -> Bool {
        run == current
    }

    /// The phase `output` ends `run` in, or nil when the run was superseded. Only successes replace
    /// `lastSuccess`.
    mutating func finish(_ run: Int, image: UIImage, output: TranslationResult) -> TranslatorModel.Phase? {
        guard isCurrent(run) else { return nil }
        let phase = Self.phase(for: output)
        if phase == .done { lastSuccess = LastResult(image: image, result: output) }
        return phase
    }

    /// Supersedes any running translation with an already translated image (a live camera frame),
    /// which becomes the last success.
    mutating func show(image: UIImage, output: TranslationResult) {
        current += 1
        lastSuccess = LastResult(image: image, result: output)
    }

    static func phase(for output: TranslationResult) -> TranslatorModel.Phase {
        if let error = output.translationError {
            return .failed("Translation failed: \(error)")
        }
        if output.regions.isEmpty {
            return .failed(noTextMessage)
        }
        return .done
    }

    static let noTextMessage = "No text found. Try moving closer or reducing glare."
}

enum PhotoDecoder {
    /// Long-side cap for OCR, as on Android; recognition gains little above this and memory grows quadratically.
    static let maxOCRSide = 2048

    /// Decodes an encoded photo straight to at most `maxOCRSide` on its long side with its EXIF
    /// orientation applied, without decoding the full-size image first. Safe off the main thread.
    static func preparedForOCR(_ data: Data) -> UIImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxOCRSide,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        return UIImage(cgImage: image)
    }
}

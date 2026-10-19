import Foundation
import UIKit
import os

/// Owns the Rust engine and the app's translation state.
@MainActor
final class TranslatorModel: ObservableObject {
    enum Phase: Equatable {
        case idle
        case working(String)
        case done
        case failed(String)
    }

    @Published private(set) var sourceLanguages: [LanguageInfo] = []
    @Published private(set) var targetLanguages: [LanguageInfo] = []
    @Published private(set) var modelsReady = false
    @Published private(set) var setupError: String?
    @Published var sourceCode = UserDefaults.standard.string(forKey: "source") ?? "" {
        didSet {
            UserDefaults.standard.set(sourceCode, forKey: "source")
            if !settingLanguages { prepare() }
        }
    }
    @Published var targetCode = UserDefaults.standard.string(forKey: "target") ?? "en" {
        didSet {
            UserDefaults.standard.set(targetCode, forKey: "target")
            if !settingLanguages { prepare() }
        }
    }
    @Published private(set) var image: UIImage?
    @Published private(set) var result: TranslationResult?
    @Published private(set) var phase: Phase = .idle

    let modelsDirectory: URL
    private let usesHostModels: Bool
    private var engine: Engine?
    private let worker = DispatchQueue(label: "translatte.engine", qos: .userInitiated)
    private var settingLanguages = false
    /// Only touched on `worker`.
    private nonisolated(unsafe) var modelsLoaded = false

    /// Memory the loaded models need besides the memory-mapped weights, matching the Android app:
    /// ~840MB peak footprint translating a photo, plus margin. Below this, loading risks iOS killing
    /// the app mid-translation, so it refuses.
    private nonisolated static let minFreeMemory = 1_200_000_000

    /// Simulator runs can point at the host's models via `TRANSLATTE_MODELS_DIR`
    /// (`SIMCTL_CHILD_TRANSLATTE_MODELS_DIR` with `simctl launch`); otherwise models live in Documents.
    init() {
        if let dir = ProcessInfo.processInfo.environment["TRANSLATTE_MODELS_DIR"], !dir.isEmpty {
            modelsDirectory = URL(fileURLWithPath: dir)
            usesHostModels = true
        } else {
            let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            modelsDirectory = docs.appendingPathComponent("models", isDirectory: true)
            usesHostModels = false
        }
        reload()
    }

    func reload() {
        do {
            guard let url = Bundle.main.url(forResource: "manifest", withExtension: "json") else {
                throw FfiError.Failed(message: "manifest.json missing from app bundle")
            }
            try FileManager.default.createDirectory(at: modelsDirectory, withIntermediateDirectories: true)
            if !usesHostModels {
                var values = URLResourceValues()
                values.isExcludedFromBackup = true
                var dir = modelsDirectory
                try dir.setResourceValues(values)
            }
            let engine = try self.engine ?? Engine(modelsDir: modelsDirectory.path, manifestJson: String(contentsOf: url, encoding: .utf8))
            self.engine = engine
            sourceLanguages = try engine.sourceLanguages()
            targetLanguages = engine.targetLanguages()
            modelsReady = try engine.coreInstalled() && !sourceLanguages.isEmpty
            setupError = nil
            if sourceLanguages.contains(where: { $0.code == sourceCode }) {
                prepare()
            } else {
                sourceCode = sourceLanguages.first?.code ?? ""
            }
        } catch {
            setupError = errorMessage(error)
            modelsReady = false
        }
    }

    func translate(_ picked: UIImage) async {
        guard let engine else { return }
        result = nil
        phase = .working("Loading photo…")
        let (source, target) = (sourceCode, targetCode)
        let listener = StageListener { [weak self] text in
            Task { @MainActor in self?.phase = .working(text) }
        }
        do {
            let (prepared, jpeg) = try await onWorker {
                let prepared = picked.preparedForOCR()
                guard let jpeg = prepared.jpegData(compressionQuality: 0.92) else {
                    throw AppError(errorDescription: "Could not encode the photo")
                }
                return (prepared, jpeg)
            }
            image = prepared
            phase = .working("Starting…")
            let output = try await onWorker { [self] in
                try checkMemory()
                let output = try engine.translateImage(image: jpeg, source: source, target: target, translate: true, listener: listener)
                modelsLoaded = true
                return output
            }
            result = output
            if let error = output.translationError {
                phase = .failed("Translation failed: \(error)")
            } else if output.regions.isEmpty {
                phase = .failed("No text found. Try moving closer or reducing glare.")
            } else {
                phase = .done
            }
        } catch {
            phase = .failed(errorMessage(error))
        }
    }

    func photoFailedToLoad() {
        result = nil
        phase = .failed("Could not open the photo")
    }

    /// Loads the models and prefills the translator's prompt prefix for the picked languages on the
    /// worker queue, so a photo taken next skips that work.
    func prepare() {
        guard let engine, modelsReady, !sourceCode.isEmpty else { return }
        let (source, target) = (sourceCode, targetCode)
        worker.async { [self] in
            do {
                try checkMemory()
                try engine.prepare(source: source, target: target)
                modelsLoaded = true
            } catch let error as AppError {
                Task { @MainActor in
                    if !self.phase.isWorking { self.phase = .failed(error.localizedDescription) }
                }
            } catch {
                NSLog("translatte: prepare failed: %@", errorMessage(error))
            }
        }
    }

    func releaseModels() {
        guard let engine else { return }
        let task = UIApplication.shared.beginBackgroundTask(withName: "Unload models")
        worker.async { [self] in
            engine.unload()
            modelsLoaded = false
            DispatchQueue.main.async { UIApplication.shared.endBackgroundTask(task) }
        }
    }

    /// Refuses to load the models when iOS would likely kill the app for using them. Runs on `worker`.
    private nonisolated func checkMemory() throws {
        guard !modelsLoaded else { return }
        let available = os_proc_available_memory()
        // 0 means no limit applies, as in the simulator.
        guard available > 0, available < Self.minFreeMemory else { return }
        let free = ByteCountFormatter.string(fromByteCount: Int64(available), countStyle: .memory)
        let needed = ByteCountFormatter.string(fromByteCount: Int64(Self.minFreeMemory), countStyle: .memory)
        throw AppError(
            errorDescription: "Not enough free memory to load the translator (\(free) free, needs \(needed)). Close other apps and try again."
        )
    }

    private func onWorker<T>(_ work: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            worker.async { continuation.resume(with: Result(catching: work)) }
        }
    }

    private func setLanguages(source: String?, target: String?) {
        settingLanguages = true
        if let source { sourceCode = source }
        if let target { targetCode = target }
        settingLanguages = false
        prepare()
    }

    /// `-autotest-image <path> -autotest-from <code> -autotest-to <code>` runs one translation at
    /// launch and writes the result to Documents/autotest.json, for scripted simulator tests.
    func runAutotestIfRequested() async {
        let defaults = UserDefaults.standard
        guard let path = defaults.string(forKey: "autotest-image") else { return }
        setLanguages(source: defaults.string(forKey: "autotest-from"), target: defaults.string(forKey: "autotest-to"))
        let out = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("autotest.json")
        let start = Date()
        if let picked = UIImage(contentsOfFile: path) {
            await translate(picked)
        } else {
            photoFailedToLoad()
        }
        let summary: [String: Any] = [
            "phase": "\(phase)",
            "seconds": Date().timeIntervalSince(start),
            "regions": (result?.regions ?? []).map {
                ["source": $0.sourceText, "translated": $0.translatedText ?? NSNull()] as [String: Any]
            },
        ]
        try? JSONSerialization.data(withJSONObject: summary, options: [.prettyPrinted]).write(to: out)
    }
}

private struct AppError: LocalizedError {
    let errorDescription: String?
}

/// Shows UniFFI errors by their message rather than their debug description.
private func errorMessage(_ error: Error) -> String {
    if case let FfiError.Failed(message) = error { return message }
    return error.localizedDescription
}

private final class StageListener: ProgressListener, @unchecked Sendable {
    private let update: (String) -> Void

    init(update: @escaping (String) -> Void) {
        self.update = update
    }

    func onStage(stage: String, done: UInt32, total: UInt32) {
        let label: String
        switch stage {
        case "Detecting": label = "Finding text…"
        case "Recognizing": label = "Reading text \(done + 1)/\(total)…"
        case "Translating": label = total > 1 ? "Translating \(done + 1)/\(total)…" : "Translating…"
        default: label = stage
        }
        update(label)
    }
}

extension UIImage {
    /// Long-side cap for OCR, as on Android; recognition gains little above this and memory grows quadratically.
    private static let maxOCRSide: CGFloat = 2048

    /// Bakes EXIF orientation into the pixels so OCR coordinates match what's on screen, and caps the
    /// long side at `maxOCRSide`. Safe to call off the main thread.
    func preparedForOCR() -> UIImage {
        let scale = min(1, Self.maxOCRSide / max(size.width, size.height))
        guard imageOrientation != .up || scale < 1 else { return self }
        let target = CGSize(width: (size.width * scale).rounded(), height: (size.height * scale).rounded())
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: target, format: format).image { _ in
            draw(in: CGRect(origin: .zero, size: target))
        }
    }
}

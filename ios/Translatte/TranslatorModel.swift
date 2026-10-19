import Foundation
import UIKit

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
            prepare()
        }
    }
    @Published var targetCode = UserDefaults.standard.string(forKey: "target") ?? "en" {
        didSet {
            UserDefaults.standard.set(targetCode, forKey: "target")
            prepare()
        }
    }
    @Published private(set) var image: UIImage?
    @Published private(set) var result: TranslationResult?
    @Published private(set) var phase: Phase = .idle

    let modelsDirectory: URL
    private var engine: Engine?
    private let worker = DispatchQueue(label: "translatte.engine", qos: .userInitiated)

    init() {
        modelsDirectory = Self.resolveModelsDirectory()
        reload()
    }

    /// Simulator runs can point at the host's models via `TRANSLATTE_MODELS_DIR`
    /// (`SIMCTL_CHILD_TRANSLATTE_MODELS_DIR` with `simctl launch`); otherwise models live in Documents.
    private static func resolveModelsDirectory() -> URL {
        if let dir = ProcessInfo.processInfo.environment["TRANSLATTE_MODELS_DIR"], !dir.isEmpty {
            return URL(fileURLWithPath: dir)
        }
        let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        return docs.appendingPathComponent("models", isDirectory: true)
    }

    func reload() {
        do {
            guard let url = Bundle.main.url(forResource: "manifest", withExtension: "json") else {
                throw FfiError.Failed(message: "manifest.json missing from app bundle")
            }
            try FileManager.default.createDirectory(at: modelsDirectory, withIntermediateDirectories: true)
            let engine = try Engine(modelsDir: modelsDirectory.path, manifestJson: String(contentsOf: url, encoding: .utf8))
            self.engine = engine
            sourceLanguages = try engine.sourceLanguages()
            targetLanguages = engine.targetLanguages()
            modelsReady = try engine.coreInstalled() && !sourceLanguages.isEmpty
            if !sourceLanguages.contains(where: { $0.code == sourceCode }) {
                sourceCode = sourceLanguages.first?.code ?? ""
            }
            setupError = nil
            prepare()
        } catch {
            setupError = error.localizedDescription
            modelsReady = false
        }
    }

    func translate(_ picked: UIImage) async {
        guard let engine else { return }
        let normalized = picked.normalizedForOCR()
        image = normalized
        result = nil
        guard let jpeg = normalized.jpegData(compressionQuality: 0.92) else {
            phase = .failed("Could not encode the photo")
            return
        }
        phase = .working("Starting…")
        let (source, target) = (sourceCode, targetCode)
        let listener = StageListener { [weak self] text in
            Task { @MainActor in self?.phase = .working(text) }
        }
        do {
            let output = try await withCheckedThrowingContinuation { continuation in
                worker.async {
                    continuation.resume(with: Result {
                        try engine.translateImage(image: jpeg, source: source, target: target, translate: true, listener: listener)
                    })
                }
            }
            result = output
            phase = output.translationError.map { .failed("Translation failed: \($0)") } ?? .done
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    /// Loads the models and prefills the translator's prompt prefix for the picked languages on the
    /// worker queue, so a photo taken next skips that work.
    func prepare() {
        guard let engine, modelsReady, !sourceCode.isEmpty else { return }
        let (source, target) = (sourceCode, targetCode)
        worker.async {
            do {
                try engine.prepare(source: source, target: target)
            } catch {
                NSLog("translatte: prepare failed: %@", error.localizedDescription)
            }
        }
    }

    func releaseModels() {
        guard let engine else { return }
        worker.async { engine.unload() }
    }

    /// `-autotest-image <path> -autotest-from <code> -autotest-to <code>` runs one translation at
    /// launch and writes the result to Documents/autotest.json, for scripted simulator tests.
    func runAutotestIfRequested() async {
        let defaults = UserDefaults.standard
        guard let path = defaults.string(forKey: "autotest-image") else { return }
        if let from = defaults.string(forKey: "autotest-from") { sourceCode = from }
        if let to = defaults.string(forKey: "autotest-to") { targetCode = to }
        let out = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("autotest.json")
        guard let picked = UIImage(contentsOfFile: path) else {
            try? Data(#"{"error":"could not load image"}"#.utf8).write(to: out)
            return
        }
        let start = Date()
        await translate(picked)
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
    /// Bakes EXIF orientation into the pixels so OCR coordinates match what's on screen.
    func normalizedForOCR() -> UIImage {
        guard imageOrientation != .up else { return self }
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: size, format: format).image { _ in
            draw(in: CGRect(origin: .zero, size: size))
        }
    }
}

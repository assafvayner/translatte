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

    @Published private(set) var models: [ModelInfo] = []
    @Published private(set) var sourceLanguages: [LanguageInfo] = []
    @Published private(set) var packs: [LanguagePack] = []
    @Published private(set) var target: LanguageInfo?
    @Published private(set) var modelsReady = false
    @Published private(set) var setupError: String?
    /// Space left on the volume the models download to, as of the last `reload`.
    @Published private(set) var freeBytes: UInt64 = .max
    @Published var sourceCode = UserDefaults.standard.string(forKey: "source") ?? "" {
        didSet {
            UserDefaults.standard.set(sourceCode, forKey: "source")
            if !settingLanguages { prepare() }
        }
    }
    @Published private(set) var image: UIImage?
    @Published private(set) var result: TranslationResult?
    @Published private(set) var phase: Phase = .idle
    @Published private var runs = TranslationRuns()
    /// Shown on Home when there isn't enough free memory to load the models.
    @Published private(set) var notice: String?

    let modelsDirectory: URL
    let downloads = Downloads()
    private let usesHostModels: Bool
    private var engine: Engine?
    private let worker = DispatchQueue(label: "translatte.engine", qos: .userInitiated)
    private var settingLanguages = false
    /// Only touched on `worker`.
    private nonisolated(unsafe) var modelsLoaded = false

    /// Memory the loaded models need, with margin, matching the Android app: OCR plus a pair model
    /// peaked near 335MB (CJK) in the native benchmark. Below this, loading risks iOS killing the
    /// app mid-translation, so it refuses.
    private nonisolated static let minFreeMemory = 800_000_000

    /// Simulator runs can point at the host's models via `TRANSLATTE_MODELS_DIR`
    /// (`SIMCTL_CHILD_TRANSLATTE_MODELS_DIR` with `simctl launch`); otherwise models live in Documents.
    init() {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        configureDownloads(cacheDir: caches.appendingPathComponent("hf").path)
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
                throw FfiError.Failed(reason: "manifest.json missing from app bundle")
            }
            try FileManager.default.createDirectory(at: modelsDirectory, withIntermediateDirectories: true)
            if !usesHostModels {
                var values = URLResourceValues()
                values.isExcludedFromBackup = true
                var dir = modelsDirectory
                try dir.setResourceValues(values)
            }
            let engine = try self.engine ?? Engine(modelsDir: modelsDirectory.path, manifestJson: String(contentsOf: url, encoding: .utf8))
            if self.engine == nil {
                // Frees what an app update left behind (e.g. a replaced translator), off the main
                // thread; the engine makes a download wait for it. Host models are a developer's.
                if !usesHostModels {
                    worker.async { _ = try? engine.pruneModels() }
                }
                self.engine = engine
                downloads.engine = engine
            }
            models = try engine.models()
            packs = try engine.languagePacks()
            sourceLanguages = try engine.sourceLanguages()
            target = try engine.targetLanguage()
            modelsReady = try engine.coreInstalled() && !sourceLanguages.isEmpty
            setupError = nil
            setSource(LanguageDirection.fallback(source: sourceCode, sources: sourceLanguages))
        } catch {
            setupError = errorMessage(error)
            modelsReady = false
        }
        let values = try? modelsDirectory.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
        freeBytes = values?.volumeAvailableCapacityForImportantUsage.map { UInt64(max($0, 0)) } ?? .max
    }

    /// Bytes still to fetch for `ids`, counting partial downloads that will resume.
    func bytesToDownload(_ ids: [String]) -> UInt64 {
        models.filter { ids.contains($0.id) }.reduce(0) { $0 + $1.remainingBytes }
    }

    func deleteLanguage(_ code: String) {
        guard let engine else { return }
        Task {
            var failure: Error?
            do {
                try await onWorker { [self] in
                    // Deleting the script in use frees every loaded model.
                    if try engine.deleteLanguage(code: code) { modelsLoaded = false }
                }
            } catch {
                failure = error
            }
            reload()
            if let failure { setupError = errorMessage(failure) }
        }
    }

    /// The last successful translation; failures never replace it.
    var lastSuccess: LastResult? { runs.lastSuccess }

    /// Translates an encoded photo (a capture or a Photos pick), superseding any running translation.
    func translate(_ data: Data) async {
        let run = runs.start()
        image = nil
        result = nil
        phase = .working("Loading photo…")
        let prepared = try? await onWorker { PhotoDecoder.preparedForOCR(data) }
        guard runs.isCurrent(run) else { return }
        guard let prepared = prepared ?? nil else { return photoFailedToLoad() }
        await translate(prepared, run: run)
    }

    private func translate(_ photo: UIImage, run: Int) async {
        guard let engine else { return }
        image = photo
        result = nil
        phase = .working("Starting…")
        let source = sourceCode
        let listener = StageListener { [weak self] text in
            Task { @MainActor in
                guard let self, self.runs.isCurrent(run), self.phase.isWorking else { return }
                self.phase = .working(text)
            }
        }
        do {
            let output = try await onWorker { [self] in
                guard let jpeg = photo.jpegData(compressionQuality: 0.92) else {
                    throw AppError(errorDescription: "Could not encode the photo")
                }
                try checkMemory()
                #if DEBUG
                // For UI tests: `-mock-translation-error <message>` makes every translation fail.
                if let message = UserDefaults.standard.string(forKey: "mock-translation-error") {
                    throw AppError(errorDescription: message)
                }
                #endif
                let output = try engine.translateImage(image: jpeg, source: source, translate: true, listener: listener)
                modelsLoaded = true
                return output
            }
            notice = nil
            guard let phase = runs.finish(run, image: photo, output: output) else { return }
            result = output
            self.phase = phase
        } catch {
            guard runs.isCurrent(run) else { return }
            phase = .failed(errorMessage(error))
        }
    }

    func photoFailedToLoad() {
        _ = runs.start()
        image = nil
        result = nil
        phase = .failed("Could not open the photo")
    }

    /// Shows the last successful translation in the result viewer again.
    func showLastSuccess() {
        guard let last = runs.lastSuccess, image !== last.image else { return }
        _ = runs.start()
        image = last.image
        result = last.result
        phase = .done
    }

    /// Shows a live camera frame and its translation in the result viewer without running the pipeline again.
    func showLive(image: UIImage, result: TranslationResult) {
        runs.show(image: image, output: result)
        self.image = image
        self.result = result
        phase = .done
    }

    /// Translates one live camera frame from the current source language on the worker queue.
    func translateFrame(_ frame: LiveFrame) async throws -> TranslationResult {
        guard let engine else { throw AppError(errorDescription: "The engine isn't ready") }
        let source = sourceCode
        do {
            return try await onWorker { [self] in
                try checkMemory()
                let output = try engine.translateFrame(
                    rgba: frame.rgba, width: UInt32(frame.width), height: UInt32(frame.height),
                    rotationDegrees: UInt32(frame.rotation), source: source
                )
                modelsLoaded = true
                return output
            }
        } catch let error as AppError {
            if notice != error.localizedDescription { notice = error.localizedDescription }
            throw error
        }
    }

    var hasAutotestImage: Bool {
        UserDefaults.standard.string(forKey: "autotest-image") != nil
    }

    /// Loads the models for the picked source language on the worker queue and runs the translation
    /// model once, so a photo taken next skips that work.
    func prepare() {
        guard let engine, modelsReady, !sourceCode.isEmpty else { return }
        let source = sourceCode
        worker.async { [self] in
            do {
                try checkMemory()
                try engine.prepare(source: source)
                modelsLoaded = true
                Task { @MainActor in self.notice = nil }
            } catch let error as AppError {
                Task { @MainActor in self.notice = error.localizedDescription }
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
        let available = os_proc_available_memory()
        // 0 means no limit applies, as in the simulator.
        guard available > 0, available < Self.minFreeMemory, !modelsLoaded else { return }
        throw AppError(
            errorDescription: "Not enough free memory to load the models (\(formatSize(UInt64(available))) free, needs \(formatSize(UInt64(Self.minFreeMemory)))). Close other apps and try again."
        )
    }

    private func onWorker<T>(_ work: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            worker.async { continuation.resume(with: Result(catching: work)) }
        }
    }

    private func setSource(_ source: String?) {
        settingLanguages = true
        if let source { sourceCode = source }
        settingLanguages = false
        prepare()
    }

    /// For scripted simulator tests: `-autotest-download det,rec-latin,pair-fr-en` starts a download, and
    /// `-autotest-image <path> -autotest-from <code>` runs one translation at launch
    /// and writes the result to Documents/autotest.json.
    func runAutotestIfRequested() async {
        let defaults = UserDefaults.standard
        if let ids = defaults.string(forKey: "autotest-download") {
            downloads.start(ids.split(separator: ",").map(String.init), allowMetered: true)
        }
        guard let path = defaults.string(forKey: "autotest-image") else { return }
        setSource(defaults.string(forKey: "autotest-from"))
        let out = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("autotest.json")
        let start = Date()
        if let data = FileManager.default.contents(atPath: path) {
            await translate(data)
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

extension TranslatorModel.Phase {
    var isWorking: Bool {
        if case .working = self { return true }
        return false
    }
}

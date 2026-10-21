import XCTest
@testable import Translatte

/// Exercises the Rust core through the generated Swift bindings inside the simulator.
/// The simulator can read host paths, so tests use the repo checkout and a fetched models dir.
final class EngineTests: XCTestCase {
    private var repo: URL {
        get throws {
            let path = try XCTUnwrap(ProcessInfo.processInfo.environment["TRANSLATTE_REPO"], "TRANSLATTE_REPO not set")
            return URL(fileURLWithPath: path)
        }
    }

    private func manifest() throws -> String {
        try String(contentsOf: repo.appendingPathComponent("models/manifest.json"), encoding: .utf8)
    }

    func testEmptyModelsDirectoryOffersNoSourceLanguages() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let engine = try Engine(modelsDir: dir.path, manifestJson: try manifest())
        XCTAssertFalse(try engine.coreInstalled())
        XCTAssertTrue(try engine.sourceLanguages().isEmpty)
        XCTAssertEqual(try engine.targetLanguage().code, "en")
        XCTAssertEqual(try engine.languagePacks().count, 52)
    }

    func testRustErrorsSurfaceAsSwiftErrors() throws {
        let dir = FileManager.default.temporaryDirectory
        let engine = try Engine(modelsDir: dir.path, manifestJson: try manifest())
        XCTAssertThrowsError(try engine.translateImage(image: Data([1, 2, 3]), source: "fr", translate: false, listener: nil))
        XCTAssertThrowsError(try Engine(modelsDir: dir.path, manifestJson: "{}"))
    }

    func testCancelledDownloadThrowsCancelledAndDeleteStillWorks() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let engine = try Engine(modelsDir: dir.path, manifestJson: try manifest())
        let token = DownloadToken()
        token.cancel()
        XCTAssertThrowsError(try engine.download(modelIds: ["rec-latin"], listener: NoProgress(), token: token)) {
            guard case FfiError.Cancelled = $0 else { return XCTFail("\($0)") }
        }
        XCTAssertThrowsError(try engine.download(modelIds: ["nope"], listener: NoProgress(), token: DownloadToken()))
        XCTAssertFalse(try engine.deleteLanguage(code: "fr"))
        XCTAssertThrowsError(try engine.deleteLanguage(code: "xx"))
    }

    /// Fetches the smallest pack over HTTPS, checking TLS verification through the platform verifier.
    func testDownloadsAPackFromTheHub() throws {
        guard ProcessInfo.processInfo.environment["TRANSLATTE_DOWNLOAD_TESTS"] == "1" else {
            throw XCTSkip("TRANSLATTE_DOWNLOAD_TESTS not set")
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let engine = try Engine(modelsDir: dir.path, manifestJson: try manifest())
        let progress = RecordingProgress()
        try engine.download(modelIds: ["rec-latin"], listener: progress, token: DownloadToken())
        let pack = try XCTUnwrap(try engine.models().first { $0.id == "rec-latin" })
        XCTAssertEqual(pack.state, .installed)
        XCTAssertEqual(pack.downloadedBytes, pack.sizeBytes)
        XCTAssertEqual(progress.last, pack.sizeBytes)
        XCTAssertFalse(try engine.deleteLanguage(code: "fr"))
        XCTAssertEqual(try engine.models().first { $0.id == "rec-latin" }?.state, .notInstalled)
    }

    func testTranslatesBonjour() throws {
        guard let models = ProcessInfo.processInfo.environment["TRANSLATTE_MODELS"], !models.isEmpty else {
            throw XCTSkip("TRANSLATTE_MODELS not set")
        }
        let engine = try Engine(modelsDir: models, manifestJson: try manifest())
        let png = try Data(contentsOf: repo.appendingPathComponent("core/tests/data/bonjour.png"))
        try engine.prepare(source: "fr")
        let result = try engine.translateImage(image: png, source: "fr", translate: true, listener: nil)
        XCTAssertEqual(result.regions.count, 1)
        XCTAssertEqual(result.regions.first?.sourceText.lowercased(), "bonjour le monde")
        let translated = try XCTUnwrap(result.regions.first?.translatedText).lowercased()
        XCTAssertTrue(translated.contains("world") || translated.contains("hello"), translated)
        XCTAssertNil(result.translationError)
    }
}

private final class NoProgress: DownloadListener, @unchecked Sendable {
    func onProgress(modelId: String, done: UInt64, total: UInt64) {}
}

private final class RecordingProgress: DownloadListener, @unchecked Sendable {
    private let lock = NSLock()
    private var done: UInt64 = 0

    var last: UInt64 { lock.withLock { done } }

    func onProgress(modelId: String, done: UInt64, total: UInt64) {
        lock.withLock { self.done = done }
    }
}

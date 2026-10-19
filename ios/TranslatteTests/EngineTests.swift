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
        XCTAssertTrue(engine.targetLanguages().contains { $0.code == "ja" })
    }

    func testRustErrorsSurfaceAsSwiftErrors() throws {
        let dir = FileManager.default.temporaryDirectory
        let engine = try Engine(modelsDir: dir.path, manifestJson: try manifest())
        XCTAssertThrowsError(try engine.translateImage(image: Data([1, 2, 3]), source: "fr", target: "en", translate: false, listener: nil))
        XCTAssertThrowsError(try Engine(modelsDir: dir.path, manifestJson: "{}"))
    }

    func testTranslatesHelloWorld() throws {
        guard let models = ProcessInfo.processInfo.environment["TRANSLATTE_MODELS"], !models.isEmpty else {
            throw XCTSkip("TRANSLATTE_MODELS not set")
        }
        let engine = try Engine(modelsDir: models, manifestJson: try manifest())
        let png = try Data(contentsOf: repo.appendingPathComponent("core/tests/data/hello_world.png"))
        let result = try engine.translateImage(image: png, source: "en", target: "de", translate: true, listener: nil)
        XCTAssertEqual(result.regions.count, 1)
        XCTAssertEqual(result.regions.first?.sourceText, "HELLO WORLD")
        let translated = try XCTUnwrap(result.regions.first?.translatedText).lowercased()
        XCTAssertTrue(translated.contains("hallo"), translated)
        XCTAssertNil(result.translationError)
    }
}

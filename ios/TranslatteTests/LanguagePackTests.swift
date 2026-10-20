import XCTest
@testable import Translatte

final class LanguagePackTests: XCTestCase {
    private func model(_ id: String, _ state: ModelState) -> ModelInfo {
        ModelInfo(id: id, displayName: id, core: false, sizeBytes: 10, downloadedBytes: 0, state: state, languages: [])
    }

    func testMissingModelsSkipsInstalledOnesAndDuplicates() {
        let models = [model("det", .installed), model("rec-latin", .notInstalled), model("pair-fr-en", .incomplete)]
        XCTAssertEqual(
            missingModels(["det", "rec-latin", "pair-fr-en", "rec-latin"], in: models),
            ["rec-latin", "pair-fr-en"]
        )
    }

    func testPackStatusAndRemainingBytes() {
        let pack = LanguagePack(
            code: "fr", name: "French", modelIds: ["rec-latin", "pair-fr-en"], sizeBytes: 50_071_468,
            downloadedBytes: 8_044_639, state: .notInstalled, deletable: false
        )
        XCTAssertEqual(pack.remainingBytes, 42_026_829)
        XCTAssertEqual(pack.status, "50.1 MB")
        XCTAssertEqual(
            LanguagePack(code: "fr", name: "French", modelIds: [], sizeBytes: 50_071_468, downloadedBytes: 50_071_468,
                         state: .installed, deletable: true).status,
            "Installed · 50.1 MB"
        )
    }
}

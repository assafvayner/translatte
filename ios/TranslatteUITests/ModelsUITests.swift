import XCTest

final class ModelsUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testSetupScreenNeedsALanguageAndDownloadsSharedScriptsOnce() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let app = XCUIApplication()
        app.launchEnvironment["TRANSLATTE_MODELS_DIR"] = dir.path
        app.launch()

        XCTAssertTrue(app.staticTexts["Welcome to translatte"].waitForExistence(timeout: 10))
        let download = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Download'")).firstMatch
        XCTAssertTrue(app.staticTexts["Always included"].exists || app.staticTexts["ALWAYS INCLUDED"].exists)
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH 'Wi-Fi recommended · '")).firstMatch.exists)
        XCTAssertTrue(element(app, "model-det").label.contains("Required"))
        XCTAssertFalse(download.isEnabled, "a language is required")

        let french = app.buttons["pack-fr"]
        XCTAssertFalse(french.isSelected)
        french.tap()
        XCTAssertTrue(french.isSelected)
        XCTAssertTrue(download.isEnabled)
        XCTAssertEqual(download.label, "Download 50.1 MB")
        app.buttons["pack-de"].tap()
        XCTAssertEqual(download.label, "Download 87.4 MB", "German shares French's Latin script pack")
    }

    /// Sets up the app from scratch over the network (Japanese, ~76MB), then downloads and deletes
    /// Chinese, which shares Japanese's script pack, on the Languages screen. Uses the app's own
    /// container, so run it on a freshly erased simulator.
    func testDownloadAndManageLanguagePacks() throws {
        guard ProcessInfo.processInfo.environment["TRANSLATTE_E2E"] == "1" else {
            throw XCTSkip("TRANSLATTE_E2E not set")
        }
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.staticTexts["Welcome to translatte"].waitForExistence(timeout: 10))
        app.buttons["pack-ja"].tap()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Download '")).firstMatch.tap()
        let languages = app.buttons["Languages"]
        XCTAssertTrue(languages.waitForExistence(timeout: 1800), "setup didn't finish")

        languages.tap()
        XCTAssertTrue(app.navigationBars["Languages"].waitForExistence(timeout: 10))
        XCTAssertTrue(element(app, "pack-ja").label.contains("Installed"))
        XCTAssertTrue(app.buttons["delete-ja"].exists)
        XCTAssertFalse(element(app, "pack-zh").isSelected, "only installed packs are checked here")
        app.buttons["download-zh"].tap()
        XCTAssertTrue(app.buttons["delete-zh"].waitForExistence(timeout: 600))
        XCTAssertTrue(element(app, "pack-zh").label.contains("Installed"))

        app.buttons["delete-zh"].tap()
        XCTAssertTrue(app.buttons["download-zh"].waitForExistence(timeout: 30))
        XCTAssertFalse(app.buttons["delete-zh"].exists)
        XCTAssertTrue(element(app, "pack-ja").label.contains("Installed"), "Japanese keeps the shared script pack")
    }

    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }
}

import XCTest

final class ModelsUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testSetupScreenPreselectsLatinAndNeedsAPack() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let app = XCUIApplication()
        app.launchEnvironment["TRANSLATTE_MODELS_DIR"] = dir.path
        app.launch()

        XCTAssertTrue(app.navigationBars["Set up Translatte"].waitForExistence(timeout: 10))
        let download = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Download ('")).firstMatch
        XCTAssertEqual(download.label, "Download (2.5 GB)")
        XCTAssertTrue(download.isEnabled)
        XCTAssertTrue(element(app, "model-translator").label.contains("Required"))
        let latin = app.buttons["model-rec-latin"]
        XCTAssertTrue(latin.isSelected)

        latin.tap()
        XCTAssertFalse(latin.isSelected)
        XCTAssertFalse(download.isEnabled, "a script pack is required")
        app.buttons["model-rec-en"].tap()
        XCTAssertTrue(download.isEnabled)
    }

    /// Sets up the app from scratch over the network (~2.5GB), cancelling and resuming on the way,
    /// then downloads and deletes a pack on the Languages screen. Uses the app's own container, so
    /// run it on a freshly erased simulator.
    func testDownloadCancelResumeAndManage() throws {
        guard ProcessInfo.processInfo.environment["TRANSLATTE_E2E"] == "1" else {
            throw XCTSkip("TRANSLATTE_E2E not set")
        }
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.navigationBars["Set up Translatte"].waitForExistence(timeout: 10))
        app.buttons["model-rec-latin"].tap()
        app.buttons["model-rec-en"].tap()
        app.buttons["Download (2.5 GB)"].tap()

        let receiving = app.staticTexts.matching(NSPredicate(format: "label MATCHES '[0-9]{3}(\\.[0-9])? MB of 2.49 GB'")).firstMatch
        XCTAssertTrue(receiving.waitForExistence(timeout: 300))
        app.buttons["Cancel download"].tap()
        let resume = app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Download ('")).firstMatch
        XCTAssertTrue(resume.waitForExistence(timeout: 60))
        XCTAssertNotEqual(resume.label, "Download (2.5 GB)", "only the rest is left to download")
        XCTAssertFalse(app.buttons["Cancel download"].exists)

        resume.tap()
        XCTAssertTrue(app.buttons["Cancel download"].waitForExistence(timeout: 10))
        let languages = app.buttons["Languages"]
        XCTAssertTrue(languages.waitForExistence(timeout: 3600), "setup didn't finish")

        languages.tap()
        XCTAssertTrue(app.navigationBars["Languages"].waitForExistence(timeout: 10))
        XCTAssertTrue(element(app, "model-rec-en").label.contains("Installed"))
        XCTAssertTrue(element(app, "model-rec-en").isSelected)
        XCTAssertTrue(app.buttons["delete-rec-en"].exists)
        XCTAssertFalse(element(app, "model-rec-latin").isSelected, "only installed packs are checked here")
        app.buttons["download-rec-latin"].tap()
        XCTAssertTrue(app.buttons["delete-rec-latin"].waitForExistence(timeout: 300))
        XCTAssertTrue(element(app, "model-rec-latin").label.contains("Installed"))
        XCTAssertFalse(app.buttons["download-rec-latin"].exists)

        app.buttons["delete-rec-latin"].tap()
        XCTAssertTrue(app.buttons["download-rec-latin"].waitForExistence(timeout: 30))
        XCTAssertFalse(app.buttons["delete-rec-latin"].exists)
        XCTAssertFalse(element(app, "model-rec-latin").label.contains("Installed"))
    }

    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }
}

import XCTest

/// Saves screenshots of every screen in both themes to `TRANSLATTE_SCREENSHOTS` (a host directory).
final class ScreenshotUITests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        continueAfterFailure = false
        guard let path = TestEnv.value("TRANSLATTE_SCREENSHOTS") else { throw XCTSkip("TRANSLATTE_SCREENSHOTS not set") }
        dir = URL(fileURLWithPath: path)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    func testCaptureEveryScreenInBothThemes() throws {
        for theme in ["translatte", "americano"] {
            let empty = try XCUIApplication.withModels(["-theme", theme, "-source", "en", "-target", "de"])
            XCTAssertTrue(empty.element("home").waitForExistence(timeout: 30))
            save("home-empty-\(theme)")
            empty.buttons["from-picker"].tap()
            XCTAssertTrue(empty.staticTexts["TRANSLATE FROM"].waitForExistence(timeout: 5))
            sleep(1)
            save("picker-from-\(theme)")
            empty.buttons["language-en"].tap()
            XCTAssertTrue(empty.waitUntil(empty.staticTexts["TRANSLATE FROM"], "exists == false"))
            empty.buttons["to-picker"].tap()
            XCTAssertTrue(empty.staticTexts["TRANSLATE TO"].waitForExistence(timeout: 5))
            sleep(1)
            save("picker-to-\(theme)")
            empty.buttons["language-de"].tap()
            XCTAssertTrue(empty.waitUntil(empty.staticTexts["TRANSLATE TO"], "exists == false"))
            empty.terminate()

            let app = try XCUIApplication.withModels([
                "-theme", theme, "-mock-camera", TestEnv.helloWorld, "-source", "en", "-target", "de",
                "-resultHintSeen", "NO",
            ])
            XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
            save("camera-\(theme)")
            app.buttons["camera-language-chip"].tap()
            XCTAssertTrue(app.element("from-picker").waitForExistence(timeout: 5))
            sleep(1)
            save("camera-languages-\(theme)")
            app.buttons["to-picker"].tap()
            XCTAssertTrue(app.staticTexts["TRANSLATE TO"].waitForExistence(timeout: 5))
            sleep(1)
            save("camera-dropdown-\(theme)")
            app.buttons["language-de"].tap()
            XCTAssertTrue(app.waitUntil(app.staticTexts["TRANSLATE TO"], "exists == false"))
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()
            XCTAssertTrue(app.waitUntil(app.element("from-picker"), "exists == false"))
            app.buttons["camera-shutter"].tap()
            let region = app.element("region-0")
            XCTAssertTrue(region.waitForExistence(timeout: 300))
            XCTAssertTrue(app.staticTexts["result-hint"].exists)
            save("result-\(theme)")
            app.buttons["result-mode-original"].tap()
            XCTAssertTrue(app.waitUntil(region, "exists == false"))
            sleep(1)
            save("result-original-\(theme)")
            app.buttons["result-mode-translated"].tap()
            app.buttons["result-back"].tap()
            app.buttons["camera-close"].tap()
            XCTAssertTrue(app.element("last-result").waitForExistence(timeout: 5))
            save("home-last-result-\(theme)")
            app.buttons["Languages"].tap()
            XCTAssertTrue(app.navigationBars["Languages"].waitForExistence(timeout: 5))
            sleep(1)
            save("languages-\(theme)")
            app.navigationBars.buttons.firstMatch.tap()
            app.buttons["settings-button"].tap()
            XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
            sleep(1)
            save("settings-\(theme)")
            app.element("credit-font").tap()
            XCTAssertTrue(app.element("license-dialog").waitForExistence(timeout: 5))
            sleep(1)
            save("dialog-\(theme)")
            app.terminate()

            let setup = XCUIApplication()
            setup.launchEnvironment["TRANSLATTE_MODELS_DIR"] = FileManager.default.temporaryDirectory
                .appendingPathComponent(UUID().uuidString).path
            setup.launchArguments = ["-theme", theme]
            setup.launch()
            XCTAssertTrue(setup.staticTexts["Welcome to translatte"].waitForExistence(timeout: 10))
            save("setup-\(theme)")
            setup.terminate()
        }
    }

    private func save(_ name: String) {
        let data = XCUIScreen.main.screenshot().pngRepresentation
        XCTAssertNoThrow(try data.write(to: dir.appendingPathComponent("\(name).png")))
    }
}

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
            let empty = try XCUIApplication.withModels(["-theme", theme, "-resultHintSeen", "NO"])
            XCTAssertTrue(empty.element("home").waitForExistence(timeout: 30))
            save("home-empty-\(theme)")
            empty.terminate()

            let app = try XCUIApplication.withModels([
                "-theme", theme, "-mock-camera", TestEnv.helloWorld, "-source", "en", "-target", "de",
            ])
            XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
            save("camera-\(theme)")
            app.buttons["camera-shutter"].tap()
            if app.element("result-progress").waitForExistence(timeout: 5) {
                save("result-progress-\(theme)")
            }
            let region = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'region-'")).firstMatch
            XCTAssertTrue(region.waitForExistence(timeout: 300))
            save("result-\(theme)")
            app.element("result-image").doubleTap()
            XCTAssertTrue(app.staticTexts["result-zoom"].waitForExistence(timeout: 5))
            save("result-zoomed-\(theme)")
            app.buttons["result-back"].tap()
            app.buttons["camera-close"].tap()
            XCTAssertTrue(app.element("last-result").waitForExistence(timeout: 5))
            save("home-last-result-\(theme)")
            app.buttons["Languages"].tap()
            XCTAssertTrue(app.navigationBars["Languages"].waitForExistence(timeout: 5))
            save("languages-\(theme)")
            app.navigationBars.buttons.firstMatch.tap()
            app.buttons["settings-button"].tap()
            XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
            save("settings-\(theme)")
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

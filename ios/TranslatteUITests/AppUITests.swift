import XCTest

final class AppUITests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    func testLandsOnHomeWithoutACamera() throws {
        let app = try XCUIApplication.withModels()
        XCTAssertTrue(app.element("home").waitForExistence(timeout: 30))
        XCTAssertFalse(app.element("camera-button").exists, "the simulator has no camera")
        XCTAssertTrue(app.element("photos-button").exists)
        XCTAssertTrue(app.element("empty-state").exists)
        XCTAssertTrue(app.buttons["Languages"].exists)
        XCTAssertTrue(app.buttons["settings-button"].exists)
    }

    func testThemeChoicePersistsAcrossLaunches() throws {
        var app = try XCUIApplication.withModels()
        app.buttons["settings-button"].tap()
        var picker = app.segmentedControls["theme-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        picker.buttons["System"].tap()
        picker.buttons["americano"].tap()
        XCTAssertTrue(picker.buttons["americano"].isSelected)
        app.terminate()

        app = try XCUIApplication.withModels()
        app.buttons["settings-button"].tap()
        picker = app.segmentedControls["theme-picker"]
        XCTAssertTrue(picker.waitForExistence(timeout: 10))
        XCTAssertTrue(picker.buttons["americano"].isSelected)
        picker.buttons["System"].tap()
        XCTAssertTrue(app.staticTexts["About"].exists || app.staticTexts["ABOUT"].exists)
    }

    func testCameraShutterOpensTheResultAndBackReturnsToWhereItCameFrom() throws {
        let app = try XCUIApplication.withModels(["-mock-camera", TestEnv.helloWorld, "-source", "en", "-target", "de"])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30), "with a camera the app opens on it")
        XCTAssertEqual(app.buttons["camera-language-chip"].label, "English → German")

        app.buttons["camera-shutter"].tap()
        XCTAssertTrue(app.element("result-screen").waitForExistence(timeout: 10))
        let region = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'region-'")).firstMatch
        XCTAssertTrue(region.waitForExistence(timeout: 300), "translation didn't finish")
        XCTAssertFalse((region.value as? String ?? "").isEmpty, "each box exposes its original text")
        XCTAssertFalse(region.isSelected)

        region.tap()
        XCTAssertTrue(app.waitUntil(region, "isSelected == true"), "tapping a box shows its original")
        region.tap()
        XCTAssertTrue(app.waitUntil(region, "isSelected == false"))

        app.element("result-image").doubleTap()
        let zoom = app.staticTexts["result-zoom"]
        XCTAssertTrue(zoom.waitForExistence(timeout: 5))
        XCTAssertEqual(zoom.label, "250%")
        XCTAssertFalse(app.element("result-hint").exists, "the hint hides after the first gesture")
        app.element("result-image").doubleTap()
        XCTAssertTrue(app.waitUntil(zoom, "exists == false"))

        app.buttons["result-back"].tap()
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 5), "back returns to the camera")

        app.buttons["camera-close"].tap()
        XCTAssertTrue(app.element("home").waitForExistence(timeout: 5))
        XCTAssertTrue(app.element("camera-button").exists)
        app.element("last-result").tap()
        XCTAssertTrue(app.element("result-screen").waitForExistence(timeout: 5))
        app.buttons["result-back"].tap()
        XCTAssertTrue(app.element("home").waitForExistence(timeout: 5), "back returns to Home when opened from Home")

        app.buttons["camera-button"].tap()
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 5))
    }

    func testCameraLanguageChipOpensThePickers() throws {
        let app = try XCUIApplication.withModels(["-mock-camera", TestEnv.helloWorld])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
        app.buttons["camera-language-chip"].tap()
        XCTAssertTrue(app.element("from-picker").waitForExistence(timeout: 5))
        XCTAssertTrue(app.element("to-picker").exists)
        XCTAssertTrue(app.element("swap-languages").exists)
    }
}

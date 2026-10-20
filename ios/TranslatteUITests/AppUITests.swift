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
        let app = try XCUIApplication.withModels([
            "-mock-camera", TestEnv.helloWorld, "-source", "en", "-target", "de", "-resultHintSeen", "NO",
        ])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30), "with a camera the app opens on it")
        XCTAssertEqual(app.buttons["camera-language-chip"].label, "English → German")

        app.buttons["camera-shutter"].tap()
        XCTAssertTrue(app.element("result-screen").waitForExistence(timeout: 10))
        let region = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'region-'")).firstMatch
        XCTAssertTrue(region.waitForExistence(timeout: 300), "translation didn't finish")
        XCTAssertFalse((region.value as? String ?? "").isEmpty, "each box exposes its original text")
        XCTAssertFalse(region.isSelected)
        let hint = app.staticTexts["result-hint"]
        XCTAssertTrue(hint.exists, "the hint shows until the first gesture")
        XCTAssertEqual(hint.label, "Pinch to zoom · tap a box to see the original")

        region.tap()
        XCTAssertTrue(app.waitUntil(hint, "exists == false"), "the hint hides after the first gesture")
        XCTAssertTrue(app.waitUntil(region, "isSelected == true"), "tapping a box shows its original")
        region.tap()
        XCTAssertTrue(app.waitUntil(region, "isSelected == false"))

        app.element("result-image").doubleTap()
        let zoom = app.staticTexts["result-zoom"]
        XCTAssertTrue(zoom.waitForExistence(timeout: 5))
        XCTAssertEqual(zoom.label, "250%")
        app.element("result-image").doubleTap()
        XCTAssertTrue(app.waitUntil(zoom, "exists == false"))

        app.buttons["result-back"].tap()
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 5), "back returns to the camera")
        XCTAssertEqual(app.buttons["camera-close"].label, "Back to Home")

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

    func testOriginalModeShowsThePhotoTextAndTranslatedBringsTheBoxesBack() throws {
        let app = try XCUIApplication.withModels(["-mock-camera", TestEnv.helloWorld, "-source", "en", "-target", "de"])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
        app.buttons["camera-shutter"].tap()
        let region = app.element("region-0")
        XCTAssertTrue(region.waitForExistence(timeout: 300))

        let mode = app.segmentedControls["result-mode"]
        mode.buttons["Original"].tap()
        XCTAssertTrue(app.waitUntil(region, "exists == false"))
        let original = app.element("original-0")
        XCTAssertTrue(original.exists, "the original text stays readable by VoiceOver")
        XCTAssertFalse(original.label.isEmpty)

        mode.buttons["Translated"].tap()
        XCTAssertTrue(region.waitForExistence(timeout: 5))
        XCTAssertFalse(app.element("original-0").exists)
    }

    func testFailedTranslationOffersRetry() throws {
        let failing = try XCUIApplication.withModels(["-mock-camera", TestEnv.helloWorld, "-source", "en", "-target", "xx"])
        XCTAssertTrue(failing.element("camera-screen").waitForExistence(timeout: 30))
        failing.buttons["camera-shutter"].tap()
        let retry = failing.buttons["result-retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 300))
        XCTAssertTrue(failing.element("result-error").exists)
        retry.tap()
        XCTAssertTrue(failing.element("result-error").waitForExistence(timeout: 300), "retry runs the same photo again")
        XCTAssertTrue(failing.buttons["result-retry"].waitForExistence(timeout: 300))
        failing.buttons["result-back"].tap()
        failing.buttons["camera-close"].tap()
        XCTAssertTrue(failing.element("empty-state").waitForExistence(timeout: 5), "failures don't become the last result")
    }

    func testNoTextCardHasNoRetry() throws {
        let app = try XCUIApplication.withModels(["-mock-camera", try TestEnv.blankImage(), "-source", "en", "-target", "de"])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
        app.buttons["camera-shutter"].tap()
        let card = app.element("result-error")
        XCTAssertTrue(card.waitForExistence(timeout: 300))
        XCTAssertTrue(app.staticTexts["No text found. Try moving closer or reducing glare."].exists)
        XCTAssertFalse(app.buttons["result-retry"].exists)
        card.buttons["Back"].tap()
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 5))
    }

    func testSettingsShowsTheVersionAndCredits() throws {
        let app = try XCUIApplication.withModels()
        app.buttons["settings-button"].tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS '0.1.0'")).firstMatch.exists)
        XCTAssertTrue(app.staticTexts["System uses translatte in light mode and americano in dark mode."].exists)
        XCTAssertTrue(app.element("credit-ocr").label.contains("PP-OCRv5 by PaddlePaddle · Apache License 2.0"))
        XCTAssertTrue(app.element("credit-translation").label.contains("TranslateGemma 4B by Google · Gemma Terms of Use"))
        let font = app.element("credit-font")
        XCTAssertTrue(font.label.contains("Fraunces · SIL Open Font License 1.1"), font.label)
        font.tap()
        XCTAssertTrue(app.navigationBars["License"].waitForExistence(timeout: 5))
    }
}

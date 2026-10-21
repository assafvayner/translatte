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
        XCTAssertTrue(app.buttons["theme-system"].waitForExistence(timeout: 10))
        app.buttons["theme-system"].tap()
        XCTAssertTrue(app.buttons["theme-system"].isSelected)
        app.buttons["theme-americano"].tap()
        XCTAssertTrue(app.waitUntil(app.buttons["theme-americano"], "isSelected == true"))
        XCTAssertFalse(app.buttons["theme-system"].isSelected, "only the chosen row is checked")
        app.terminate()

        app = try XCUIApplication.withModels()
        app.buttons["settings-button"].tap()
        XCTAssertTrue(app.buttons["theme-americano"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["theme-americano"].isSelected)
        app.buttons["theme-system"].tap()
        XCTAssertTrue(app.staticTexts["ABOUT"].exists)
    }

    func testCameraShutterOpensTheResultAndBackReturnsToWhereItCameFrom() throws {
        let app = try XCUIApplication.withModels([
            "-mock-camera", TestEnv.bonjour, "-source", "fr", "-resultHintSeen", "NO",
        ])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30), "with a camera the app opens on it")
        XCTAssertEqual(app.buttons["camera-language-chip"].label, "French → English")

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
        let app = try XCUIApplication.withModels(["-mock-camera", TestEnv.bonjour])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
        app.buttons["camera-language-chip"].tap()
        XCTAssertTrue(app.element("from-picker").waitForExistence(timeout: 5))
        XCTAssertTrue(app.element("to-picker").exists)
        XCTAssertFalse(app.element("swap-languages").exists)
        XCTAssertFalse(app.buttons["to-picker"].isEnabled, "English is the only target")
    }

    func testTheFromPickerIsADropdownAndToIsFixed() throws {
        let app = try XCUIApplication.withModels(["-source", "fr"])
        XCTAssertTrue(app.element("home").waitForExistence(timeout: 30))
        app.buttons["from-picker"].tap()
        XCTAssertTrue(app.staticTexts["TRANSLATE FROM"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["language-fr"].isSelected, "the current source is checked")
        XCTAssertFalse(app.buttons["language-th"].exists, "From lists installed sources only (no Thai pack in the test models)")
        app.buttons["language-fr"].tap()
        XCTAssertTrue(app.waitUntil(app.staticTexts["TRANSLATE FROM"], "exists == false"), "choosing closes the dropdown")

        let to = app.buttons["to-picker"]
        XCTAssertEqual(to.value as? String, "English")
        XCTAssertFalse(to.isEnabled, "English is the only target")
        XCTAssertFalse(app.element("swap-languages").exists)
    }

    func testOriginalModeShowsThePhotoTextAndTranslatedBringsTheBoxesBack() throws {
        let app = try XCUIApplication.withModels(["-mock-camera", TestEnv.bonjour, "-source", "fr"])
        XCTAssertTrue(app.element("camera-screen").waitForExistence(timeout: 30))
        app.buttons["camera-shutter"].tap()
        let region = app.element("region-0")
        XCTAssertTrue(region.waitForExistence(timeout: 300))

        app.buttons["result-mode-original"].tap()
        XCTAssertTrue(app.buttons["result-mode-original"].isSelected)
        XCTAssertTrue(app.waitUntil(region, "exists == false"))
        let original = app.element("original-0")
        XCTAssertTrue(original.exists, "the original text stays readable by VoiceOver")
        XCTAssertFalse(original.label.isEmpty)

        app.buttons["result-mode-translated"].tap()
        XCTAssertTrue(region.waitForExistence(timeout: 5))
        XCTAssertFalse(app.element("original-0").exists)
    }

    func testAFailedTranslationShowsTheErrorWithoutRetry() throws {
        let failing = try XCUIApplication.withModels(["-mock-camera", TestEnv.bonjour, "-source", "fr", "-mock-translation-error", "Translation failed"])
        XCTAssertTrue(failing.element("camera-screen").waitForExistence(timeout: 30))
        failing.buttons["camera-shutter"].tap()
        XCTAssertTrue(failing.element("result-error").waitForExistence(timeout: 300))
        XCTAssertFalse(failing.buttons["result-retry"].exists)
        failing.buttons["result-back"].tap()
        failing.buttons["camera-close"].tap()
        XCTAssertTrue(failing.element("empty-state").waitForExistence(timeout: 5), "failures don't become the last result")
    }

    func testNoTextCardHasNoRetry() throws {
        let app = try XCUIApplication.withModels(["-mock-camera", try TestEnv.blankImage(), "-source", "fr"])
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
        XCTAssertTrue(app.staticTexts["THEME"].exists)
        XCTAssertTrue(app.element("theme-system").label.contains("translatte in light mode, americano in dark mode"))
        XCTAssertTrue(app.element("credit-ocr").label.contains("PP-OCRv5 by PaddlePaddle · Apache License 2.0"))
        XCTAssertTrue(app.element("credit-translation").label.contains("Mozilla translations models and engine · Mozilla Public License 2.0"))
        let font = app.element("credit-font")
        XCTAssertTrue(font.label.contains("Fraunces · SIL Open Font License 1.1"), font.label)
        XCTAssertTrue(font.label.contains("Font"), font.label)
        font.tap()
        let dialog = app.element("license-dialog")
        XCTAssertTrue(dialog.waitForExistence(timeout: 5), "the license opens in the app's own dialog")
        XCTAssertTrue(app.staticTexts["SIL Open Font License 1.1"].exists)
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'SIL OPEN FONT LICENSE Version 1.1'")).firstMatch.exists)
        app.buttons["Close"].tap()
        XCTAssertTrue(app.waitUntil(dialog, "exists == false"))
    }
}

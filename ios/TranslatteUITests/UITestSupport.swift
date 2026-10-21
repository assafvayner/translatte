import UIKit
import XCTest

enum TestEnv {
    static func value(_ key: String) -> String? {
        ProcessInfo.processInfo.environment[key].flatMap { $0.isEmpty ? nil : $0 }
    }

    static var bonjour: String {
        "\(value("TRANSLATTE_REPO") ?? "")/core/tests/data/bonjour.png"
    }

    /// A plain white photo with no text in it.
    static func blankImage() throws -> String {
        let size = CGSize(width: 800, height: 600)
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        let png = UIGraphicsImageRenderer(size: size, format: format).pngData { ctx in
            UIColor.white.setFill()
            ctx.fill(CGRect(origin: .zero, size: size))
        }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("blank.png")
        try png.write(to: url)
        return url.path
    }
}

extension XCUIApplication {
    /// Launches against the host's models (`TRANSLATTE_MODELS`); skips the test when they aren't set.
    static func withModels(_ arguments: [String] = []) throws -> XCUIApplication {
        guard let models = TestEnv.value("TRANSLATTE_MODELS") else { throw XCTSkip("TRANSLATTE_MODELS not set") }
        let app = XCUIApplication()
        app.launchEnvironment["TRANSLATTE_MODELS_DIR"] = models
        app.launchArguments = arguments
        app.launch()
        return app
    }

    func element(_ id: String) -> XCUIElement {
        descendants(matching: .any)[id].firstMatch
    }

    func waitUntil(_ element: XCUIElement, _ format: String, timeout: TimeInterval = 10) -> Bool {
        let expectation = XCTNSPredicateExpectation(predicate: NSPredicate(format: format), object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}

import XCTest

enum TestEnv {
    static func value(_ key: String) -> String? {
        ProcessInfo.processInfo.environment[key].flatMap { $0.isEmpty ? nil : $0 }
    }

    static var helloWorld: String {
        "\(value("TRANSLATTE_REPO") ?? "")/core/tests/data/hello_world.png"
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

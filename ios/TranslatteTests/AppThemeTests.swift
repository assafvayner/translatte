import SwiftUI
import XCTest
@testable import Translatte

final class AppThemeTests: XCTestCase {
    func testSystemFollowsTheDeviceAndTheOthersForceAnAppearance() {
        XCTAssertNil(AppTheme.system.colorScheme)
        XCTAssertEqual(AppTheme.translatte.colorScheme, .light)
        XCTAssertEqual(AppTheme.americano.colorScheme, .dark)
        XCTAssertEqual(AppTheme.system.interfaceStyle, .unspecified)
        XCTAssertEqual(AppTheme.translatte.interfaceStyle, .light)
        XCTAssertEqual(AppTheme.americano.interfaceStyle, .dark)
    }

    func testStoredValuesAndTitles() {
        XCTAssertEqual(AppTheme.allCases.map(\.rawValue), ["system", "translatte", "americano"])
        XCTAssertEqual(AppTheme.allCases.map(\.title), ["System", "translatte", "americano"])
        XCTAssertNil(AppTheme(rawValue: "sepia"))
        XCTAssertEqual(AppTheme.storageKey, "theme")
    }

    func testAppStoragePersistsTheChoice() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "AppThemeTests-\(UUID().uuidString)"))
        let stored = AppStorage(wrappedValue: AppTheme.system, AppTheme.storageKey, store: defaults)
        XCTAssertEqual(stored.wrappedValue, .system)
        stored.wrappedValue = .americano
        XCTAssertEqual(defaults.string(forKey: "theme"), "americano")
        XCTAssertEqual(AppStorage(wrappedValue: AppTheme.system, AppTheme.storageKey, store: defaults).wrappedValue, .americano)
    }
}

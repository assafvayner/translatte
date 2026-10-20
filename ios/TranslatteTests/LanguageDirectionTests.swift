import XCTest
@testable import Translatte

final class LanguageDirectionTests: XCTestCase {
    private let ja = LanguageInfo(code: "ja", name: "Japanese", rtl: false)
    private let en = LanguageInfo(code: "en", name: "English", rtl: false)
    private let ar = LanguageInfo(code: "ar", name: "Arabic", rtl: true)

    func testLabelUsesLanguageNames() {
        XCTAssertEqual(
            LanguageDirection.label(source: "ja", target: "en", sources: [ja, en], targets: [ja, en, ar]),
            "Japanese → English"
        )
    }

    func testLabelFallsBackToCodes() {
        XCTAssertEqual(LanguageDirection.label(source: "xx", target: "yy", sources: [], targets: []), "xx → yy")
    }

    func testSwapNeedsTheTargetToBeReadable() {
        XCTAssertTrue(LanguageDirection.canSwap(source: "ja", target: "en", sources: [ja, en]))
        XCTAssertFalse(LanguageDirection.canSwap(source: "ja", target: "ar", sources: [ja, en]), "no Arabic pack installed")
        XCTAssertFalse(LanguageDirection.canSwap(source: "en", target: "en", sources: [ja, en]))
    }
}

import XCTest
@testable import Translatte

final class LanguageDirectionTests: XCTestCase {
    private let ja = LanguageInfo(code: "ja", name: "Japanese", rtl: false)
    private let fr = LanguageInfo(code: "fr", name: "French", rtl: false)
    private let en = LanguageInfo(code: "en", name: "English", rtl: false)

    func testLabelUsesLanguageNames() {
        XCTAssertEqual(LanguageDirection.label(source: "ja", sources: [fr, ja], target: en), "Japanese → English")
    }

    func testLabelFallsBackToTheCode() {
        XCTAssertEqual(LanguageDirection.label(source: "xx", sources: [], target: nil), "xx → English")
    }

    func testARemovedSourceFallsBackToTheFirstOffered() {
        XCTAssertEqual(LanguageDirection.fallback(source: "ja", sources: [fr, ja]), "ja")
        XCTAssertEqual(LanguageDirection.fallback(source: "en", sources: [fr, ja]), "fr")
        XCTAssertEqual(LanguageDirection.fallback(source: "fr", sources: []), "")
    }
}

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

    func testRemovedLanguagesFallBackWithoutPairingALanguageWithItself() {
        let fr = LanguageInfo(code: "fr", name: "French", rtl: false)
        let sources = [fr, en]
        let targets = [ar, en, fr]
        func check(_ source: String, _ target: String, _ want: (String, String), in sources: [LanguageInfo],
                   line: UInt = #line) {
            let got = LanguageDirection.fallback(source: source, target: target, sources: sources, targets: targets)
            XCTAssertEqual(got.source, want.0, line: line)
            XCTAssertEqual(got.target, want.1, line: line)
        }
        check("fr", "en", ("fr", "en"), in: sources)
        check("sv", "fr", ("en", "fr"), in: sources)
        check("sv", "en", ("fr", "en"), in: sources)
        check("en", "sv", ("en", "ar"), in: sources)
        check("sv", "no", ("en", "ar"), in: sources)
        check("sv", "en", ("en", "ar"), in: [en])
        check("sv", "en", ("", "en"), in: [])
    }

    func testSwapNeedsTheTargetToBeReadable() {
        XCTAssertTrue(LanguageDirection.canSwap(source: "ja", target: "en", sources: [ja, en]))
        XCTAssertFalse(LanguageDirection.canSwap(source: "ja", target: "ar", sources: [ja, en]), "no Arabic pack installed")
        XCTAssertFalse(LanguageDirection.canSwap(source: "en", target: "en", sources: [ja, en]))
    }
}

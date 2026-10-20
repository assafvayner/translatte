import XCTest
@testable import Translatte

final class FormatSizeTests: XCTestCase {
    func testDecimalUnitsWithDigitsByMagnitude() {
        XCTAssertEqual(formatSize(4_800_000), "4.8 MB")
        XCTAssertEqual(formatSize(8_000_000), "8.0 MB")
        XCTAssertEqual(formatSize(16_640_000), "16.6 MB")
        XCTAssertEqual(formatSize(512_000_000), "512 MB")
        XCTAssertEqual(formatSize(2_490_000_000), "2.49 GB")
        XCTAssertEqual(formatSize(20_700_000_000), "20.7 GB")
        XCTAssertEqual(formatSize(207_000_000_000), "207 GB")
    }

    func testSwitchesToGigabytesAtAThousandMegabytes() {
        XCTAssertEqual(formatSize(999_000_000), "999 MB")
        XCTAssertEqual(formatSize(999_700_000), "1.00 GB")
        XCTAssertEqual(formatSize(1_000_000_000), "1.00 GB")
    }
}

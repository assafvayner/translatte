import UIKit
import XCTest
@testable import Translatte

/// The asset catalog's color sets must match the spec's hex values in both appearances.
final class PaletteTests: XCTestCase {
    private let expected: [(name: String, light: UInt32, dark: UInt32)] = [
        ("Background", 0xFBF6EE, 0x16110E),
        ("Surface", 0xF3E8D9, 0x201914),
        ("SurfaceVariant", 0xE9D8C3, 0x2C231C),
        ("Sheet", 0xFFFCF7, 0x241C17),
        ("Outline", 0xCDB79F, 0x4A3B30),
        ("Primary", 0xA4501A, 0xE09A5F),
        ("OnPrimary", 0xFFF8F0, 0x2A1406),
        ("PrimaryContainer", 0xF1D5B8, 0x5A3216),
        ("OnPrimaryContainer", 0x3E1F08, 0xF6DCC2),
        ("Text", 0x2B1D14, 0xF1E6D8),
        ("TextMuted", 0x6B5444, 0xBBA894),
        ("Error", 0xB3261E, 0xF2B8B5),
        ("Glass", 0xFBF6EE, 0x16110E),
        ("AccentColor", 0xA4501A, 0xE09A5F),
    ]

    func testColorSetsMatchTheSpecInBothAppearances() throws {
        for color in expected {
            XCTAssertEqual(try rgba(color.name, .light).rgb, color.light, "\(color.name) light")
            XCTAssertEqual(try rgba(color.name, .dark).rgb, color.dark, "\(color.name) dark")
        }
    }

    func testGlassIsTranslucent() throws {
        XCTAssertEqual(try rgba("Glass", .light).alpha, 0.86, accuracy: 0.005)
        XCTAssertEqual(try rgba("Glass", .dark).alpha, 0.78, accuracy: 0.005)
    }

    func testLogoAndIconAreBundled() {
        XCTAssertNotNil(UIImage(named: "LogoCup"))
    }

    func testEveryFrauncesFaceIsRegistered() {
        for name in [Typography.displayFace, Typography.textRegular, Typography.textSemiBold] {
            XCTAssertNotNil(UIFont(name: name, size: 16), "\(name) missing from UIAppFonts")
        }
    }

    @MainActor
    func testNavigationBarTitlesUseFraunces() {
        Typography.applyNavigationBarAppearance()
        let appearance = UINavigationBar.appearance().standardAppearance
        let title = appearance.titleTextAttributes[.font] as? UIFont
        XCTAssertEqual(title?.fontName, Typography.displayFace)
        let large = appearance.largeTitleTextAttributes[.font] as? UIFont
        XCTAssertEqual(large?.fontName, Typography.displayFace)
        let back = appearance.backButtonAppearance.normal.titleTextAttributes[.font] as? UIFont
        XCTAssertEqual(back?.fontName, Typography.textRegular)
    }

    private func rgba(_ name: String, _ style: UIUserInterfaceStyle) throws -> (rgb: UInt32, alpha: CGFloat) {
        let color = try XCTUnwrap(UIColor(named: name, in: .main, compatibleWith: nil), "missing color set \(name)")
        let resolved = color.resolvedColor(with: UITraitCollection(userInterfaceStyle: style))
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        resolved.getRed(&r, green: &g, blue: &b, alpha: &a)
        let byte = { (v: CGFloat) in UInt32((v * 255).rounded()) }
        return (byte(r) << 16 | byte(g) << 8 | byte(b), a)
    }
}

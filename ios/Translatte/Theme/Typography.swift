import SwiftUI
import UIKit

/// The shared type tokens (design/DESIGN.md); Android has the same names in `AppType`. Each scales
/// with Dynamic Type relative to a system text style.
enum Typography {
    static let displayFace = "Fraunces72pt-SemiBold"
    static let textRegular = "Fraunces9pt-Regular"
    static let textSemiBold = "Fraunces9pt-SemiBold"

    static let wordmark = Font.custom(displayFace, size: 26, relativeTo: .title)
    static let wordmarkTracking: CGFloat = -0.3
    static let display = Font.custom(displayFace, size: 24, relativeTo: .title)
    static let title = Font.custom(displayFace, size: 20, relativeTo: .title2)
    static let headline = Font.custom(textSemiBold, size: 17, relativeTo: .headline)
    static let body = Font.custom(textRegular, size: 16, relativeTo: .body)
    /// Extra leading that brings body to its ~22pt line height.
    static let bodyLineSpacing: CGFloat = 3
    static let button = Font.custom(textSemiBold, size: 16, relativeTo: .body)
    static let label = Font.custom(textSemiBold, size: 14, relativeTo: .subheadline)
    static let caption = Font.custom(textRegular, size: 13, relativeTo: .caption)
    /// Use through `Overline`, which uppercases and tracks it.
    static let overline = Font.custom(textSemiBold, size: 11, relativeTo: .caption2)
    static let overlineTracking: CGFloat = 0.8

    /// Navigation bar titles in the title token and bar buttons in the body token, scaled like the
    /// SwiftUI tokens.
    @MainActor
    static func applyNavigationBarAppearance() {
        func font(_ name: String, _ size: CGFloat, _ style: UIFont.TextStyle) -> UIFont {
            UIFontMetrics(forTextStyle: style).scaledFont(for: UIFont(name: name, size: size) ?? .systemFont(ofSize: size))
        }
        let text = UIColor(named: "Text") ?? .label
        let appearance = UINavigationBarAppearance()
        appearance.configureWithOpaqueBackground()
        appearance.backgroundColor = UIColor(named: "Background")
        appearance.shadowColor = .clear
        appearance.titleTextAttributes = [.font: font(displayFace, 20, .title2), .foregroundColor: text]
        appearance.largeTitleTextAttributes = [.font: font(displayFace, 32, .largeTitle), .foregroundColor: text]
        let buttons = UIBarButtonItemAppearance()
        buttons.normal.titleTextAttributes = [.font: font(textRegular, 16, .body)]
        appearance.buttonAppearance = buttons
        appearance.backButtonAppearance = buttons
        let bar = UINavigationBar.appearance()
        bar.standardAppearance = appearance
        bar.scrollEdgeAppearance = appearance
        bar.compactAppearance = appearance
    }
}

/// The overline token: uppercase, tracked and muted (FROM / TO, card section headers).
struct Overline: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text.uppercased())
            .font(Typography.overline)
            .tracking(Typography.overlineTracking)
            .foregroundStyle(Palette.textMuted)
    }
}

/// Shared radii and sizes (design/DESIGN.md); Android has the same values in `Dimens`.
enum Metrics {
    static let cardRadius: CGFloat = 20
    static let innerRadius: CGFloat = 14
    static let sheetRadius: CGFloat = 28
    static let dialogRadius: CGFloat = 28
    static let buttonHeight: CGFloat = 56
    static let smallButtonHeight: CGFloat = 36
    static let chipHeight: CGFloat = 44
    static let rowMinHeight: CGFloat = 52
    static let gutter: CGFloat = 16
    static let spacing: CGFloat = 16
}

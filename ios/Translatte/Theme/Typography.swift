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
    /// Extra leading over Fraunces' natural 1.23 line height, matching Android's token line heights
    /// (headline 22, body 22, caption 18).
    static let headlineLineSpacing: CGFloat = 1
    static let bodyLineSpacing: CGFloat = 2.3
    static let captionLineSpacing: CGFloat = 2
    static let button = Font.custom(textSemiBold, size: 16, relativeTo: .body)
    static let label = Font.custom(textSemiBold, size: 14, relativeTo: .subheadline)
    static let caption = Font.custom(textRegular, size: 13, relativeTo: .caption)
    /// Use through `Overline`, which uppercases and tracks it.
    static let overline = Font.custom(textSemiBold, size: 11, relativeTo: .caption2)
    static let overlineTracking: CGFloat = 0.8

    /// Navigation bar titles in the title token and bar buttons in the body token, scaled for the
    /// current content size. Call again when it changes: this also restyles bars already on screen.
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
        func style(_ bar: UINavigationBar) {
            bar.standardAppearance = appearance
            bar.scrollEdgeAppearance = appearance
            bar.compactAppearance = appearance
        }
        style(UINavigationBar.appearance())
        func restyle(_ view: UIView) {
            if let bar = view as? UINavigationBar { style(bar) }
            view.subviews.forEach(restyle)
        }
        for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
            scene.windows.forEach(restyle)
        }
    }

    /// The height of the app's key window, for sizing popovers.
    @MainActor
    static var windowHeight: CGFloat {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.first?.keyWindow?.bounds.height ?? scenes.first?.screen.bounds.height ?? 800
    }
}

/// SF Symbol sizing that follows Dynamic Type, relative to a text style.
private struct IconFont: ViewModifier {
    @ScaledMetric private var size: CGFloat
    private let weight: Font.Weight

    init(size: CGFloat, weight: Font.Weight, relativeTo style: Font.TextStyle) {
        _size = ScaledMetric(wrappedValue: size, relativeTo: style)
        self.weight = weight
    }

    func body(content: Content) -> some View {
        content.font(.system(size: size, weight: weight))
    }
}

extension View {
    func iconFont(_ size: CGFloat, weight: Font.Weight = .regular, relativeTo style: Font.TextStyle = .body) -> some View {
        modifier(IconFont(size: size, weight: weight, relativeTo: style))
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

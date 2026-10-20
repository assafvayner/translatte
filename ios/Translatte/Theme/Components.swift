import SwiftUI

struct Wordmark: View {
    var size: CGFloat = 26

    var body: some View {
        Text("translatte")
            .font(.display(size))
            .tracking(-0.3)
            .foregroundStyle(Palette.text)
            .accessibilityAddTraits(.isHeader)
    }
}

struct LogoCup: View {
    let size: CGFloat

    var body: some View {
        Image("LogoCup")
            .resizable()
            .scaledToFit()
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// The 56pt pill buttons: primary (filled) and tonal (primary container).
struct BigButtonStyle: ButtonStyle {
    enum Kind { case primary, tonal }

    let kind: Kind
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .labelStyle(.titleAndIcon)
            .frame(maxWidth: .infinity, minHeight: 56)
            .foregroundStyle(kind == .primary ? Palette.onPrimary : Palette.onPrimaryContainer)
            .background(kind == .primary ? Palette.primary : Palette.primaryContainer, in: Capsule())
            .opacity(isEnabled ? (configuration.isPressed ? 0.85 : 1) : 0.45)
    }
}

/// A surface-colored rounded card with a small uppercase heading.
struct Card<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title.uppercased())
                .font(.caption.weight(.semibold))
                .tracking(0.4)
                .foregroundStyle(Palette.textMuted)
                .accessibilityAddTraits(.isHeader)
            content
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
    }
}

/// An error-tinted banner with an icon, e.g. the low-memory notice.
struct NoticeBanner: View {
    let message: String

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(Palette.error)
            Text(message)
                .font(.callout)
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.leading)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.error.opacity(0.14), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

extension View {
    /// The translucent card floated over photos (progress, errors).
    func glassCard() -> some View {
        padding(20)
            .foregroundStyle(Palette.text)
            .background(Palette.glass, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
    }
}

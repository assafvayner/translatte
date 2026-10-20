import SwiftUI

struct Wordmark: View {
    var body: some View {
        Text("translatte")
            .font(Typography.wordmark)
            .tracking(Typography.wordmarkTracking)
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

enum ButtonKind { case primary, tonal, outlined }

private extension ButtonKind {
    var foreground: Color {
        switch self {
        case .primary: Palette.onPrimary
        case .tonal: Palette.onPrimaryContainer
        case .outlined: Palette.text
        }
    }

    var fill: Color {
        switch self {
        case .primary: Palette.primary
        case .tonal: Palette.primaryContainer
        case .outlined: .clear
        }
    }
}

private struct CapsuleButton: ViewModifier {
    let kind: ButtonKind
    let pressed: Bool
    let enabled: Bool

    func body(content: Content) -> some View {
        content
            .foregroundStyle(kind.foreground)
            .background(kind.fill, in: Capsule())
            .overlay { if kind == .outlined { Capsule().strokeBorder(Palette.outline) } }
            .contentShape(Capsule())
            .opacity(enabled ? (pressed ? 0.85 : 1) : 0.45)
    }
}

/// Full-width 56pt buttons: filled primary, tonal (primary container) or outlined.
struct BigButtonStyle: ButtonStyle {
    let kind: ButtonKind
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Typography.button)
            .labelStyle(.titleAndIcon)
            .frame(maxWidth: .infinity, minHeight: Metrics.buttonHeight)
            .modifier(CapsuleButton(kind: kind, pressed: configuration.isPressed, enabled: isEnabled))
    }
}

/// 36pt buttons in rows and cards (Download, Delete, Languages).
struct SmallButtonStyle: ButtonStyle {
    let kind: ButtonKind
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Typography.label)
            .labelStyle(.titleAndIcon)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 16)
            .frame(height: Metrics.smallButtonHeight)
            .modifier(CapsuleButton(kind: kind, pressed: configuration.isPressed, enabled: isEnabled))
    }
}

/// Text buttons in the primary color: dialog actions and the result card's Back.
struct TextButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Typography.button)
            .foregroundStyle(Palette.primary)
            .padding(.horizontal, 12)
            .frame(minHeight: 40)
            .contentShape(Capsule())
            .background(configuration.isPressed ? Palette.primary.opacity(0.12) : .clear, in: Capsule())
            .opacity(isEnabled ? 1 : 0.45)
    }
}

/// A surface card with an overline heading; rows inside pad themselves (see `ListRow`).
struct Card<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Overline(title)
                .padding(.horizontal, 16)
                .padding(.bottom, 4)
                .accessibilityAddTraits(.isHeader)
            content
        }
        .padding(.top, 14)
        .padding(.bottom, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
    }
}

/// A 52pt row: body title, optional caption, optional trailing content.
struct ListRow<Trailing: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 0) {
                Text(title)
                    .font(Typography.body)
                    .foregroundStyle(Palette.text)
                if let subtitle {
                    Text(subtitle)
                        .font(Typography.caption)
                        .lineSpacing(Typography.captionLineSpacing)
                        .foregroundStyle(Palette.textMuted)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            trailing
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(minHeight: Metrics.rowMinHeight)
        .contentShape(Rectangle())
    }
}

extension ListRow where Trailing == EmptyView {
    init(title: String, subtitle: String? = nil) {
        self.init(title: title, subtitle: subtitle) { EmptyView() }
    }
}

/// A selectable row with a checkmark on the selected one; used by pickers and the theme choice.
struct CheckRow: View {
    let title: String
    var subtitle: String?
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ListRow(title: title, subtitle: subtitle) {
                Image(systemName: "checkmark")
                    .iconFont(17, weight: .semibold)
                    .foregroundStyle(Palette.primary)
                    .frame(width: 24, height: 24)
                    .opacity(selected ? 1 : 0)
            }
        }
        .buttonStyle(RowButtonStyle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }
}

/// Plain rows that highlight while pressed.
struct RowButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? Palette.text.opacity(0.08) : .clear)
    }
}

/// The compact list a FROM / TO chip opens as a popover: overline title, body rows at least 48pt tall,
/// a checkmark on the current option, opened with it as the third row. Choosing one closes it.
struct PickerDropdown<Option: Hashable>: View {
    let title: String
    let options: [Option]
    let selected: Option?
    let label: (Option) -> String
    let id: (Option) -> String
    let anchorWidth: CGFloat
    let onSelect: (Option) -> Void
    @Environment(\.dismiss) private var dismiss
    @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 48
    @ScaledMetric(relativeTo: .caption2) private var headerHeight: CGFloat = 36

    var body: some View {
        let width = min(max(anchorWidth, 220), 280)
        let height = min(CGFloat(options.count) * rowHeight + headerHeight + 8, Typography.windowHeight * 0.6)
        ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(options, id: \.self) { option in
                        row(option).id(option)
                    }
                }
                .padding(.bottom, 8)
            }
            .safeAreaInset(edge: .top, spacing: 0) {
                Overline(title)
                    .padding(.horizontal, 16)
                    .padding(.top, 14)
                    .padding(.bottom, 6)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Palette.sheet)
                    .accessibilityAddTraits(.isHeader)
            }
            .onAppear {
                guard let selected, let index = options.firstIndex(of: selected) else { return }
                proxy.scrollTo(options[max(index - 2, 0)], anchor: .top)
            }
        }
        .frame(width: width, height: height, alignment: .top)
        .presentationCompactAdaptation(.popover)
        .presentationBackground(Palette.sheet)
        .accessibilityIdentifier("picker-dropdown")
    }

    private func row(_ option: Option) -> some View {
        let on = option == selected
        return Button {
            onSelect(option)
            dismiss()
        } label: {
            HStack(spacing: 12) {
                Text(label(option))
                    .font(Typography.body)
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Image(systemName: "checkmark")
                    .iconFont(15, weight: .semibold)
                    .foregroundStyle(Palette.primary)
                    .opacity(on ? 1 : 0)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .frame(minHeight: rowHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(RowButtonStyle())
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier(id(option))
    }
}

/// An error-tinted notice, e.g. the low-memory warning on Home or a download error.
struct NoticeBanner: View {
    let message: String

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(Palette.error)
            Text(message)
                .font(Typography.body)
                .lineSpacing(Typography.bodyLineSpacing)
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.leading)
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.error.opacity(0.14), in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/// A glass chip over the photo: label text in a 44pt capsule.
struct GlassChip: View {
    let text: String

    var body: some View {
        Text(text)
            .font(Typography.label)
            .lineLimit(1)
            .padding(.horizontal, 16)
            .frame(height: Metrics.chipHeight)
            .background(Palette.glass, in: Capsule())
    }
}

/// Two-option capsule toggle (Translated / Original), the same look as on Android.
struct SegmentedToggle: View {
    let labels: [String]
    @Binding var selection: Int
    let id: String

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(labels.enumerated()), id: \.offset) { index, label in
                let on = index == selection
                Button { selection = index } label: {
                    Text(label)
                        .font(Typography.label)
                        .foregroundStyle(on ? Palette.onPrimary : Palette.text)
                        .padding(.horizontal, 16)
                        .frame(minWidth: 116, minHeight: Metrics.smallButtonHeight)
                        .background(on ? Palette.primary : .clear, in: Capsule())
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
                .accessibilityIdentifier("\(id)-\(label.lowercased())")
            }
        }
        .padding(4)
        .background(Palette.glass, in: Capsule())
    }
}

extension View {
    /// The translucent card floated over photos (progress, errors).
    func glassCard() -> some View {
        padding(20)
            .frame(maxWidth: 320)
            .foregroundStyle(Palette.text)
            .background(Palette.glass, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
    }
}

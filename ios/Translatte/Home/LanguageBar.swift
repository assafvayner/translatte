import SwiftUI

/// From / To. From lists installed source languages in a dropdown popover anchored to its chip, as
/// on Android; To is the fixed target language.
struct LanguageBar: View {
    @EnvironmentObject private var model: TranslatorModel

    var body: some View {
        HStack(spacing: 8) {
            LanguageTile(
                title: "From",
                dropdownTitle: "Translate from",
                id: "from-picker",
                languages: model.sourceLanguages,
                selection: $model.sourceCode
            )
            LanguageTile(
                title: "To",
                dropdownTitle: "Translate to",
                id: "to-picker",
                languages: model.target.map { [$0] } ?? [],
                selection: .constant(model.target?.code ?? ""),
                enabled: false
            )
        }
        .padding(8)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
        .disabled(model.phase.isWorking)
    }
}

/// A FROM or TO chip and, when enabled, the dropdown popover it opens.
private struct LanguageTile: View {
    let title: String
    let dropdownTitle: String
    let id: String
    let languages: [LanguageInfo]
    @Binding var selection: String
    var enabled = true
    @State private var picking = false
    @State private var width: CGFloat = 0
    /// Low in its window (the camera's language sheet) the dropdown opens above the chip.
    @State private var opensUp = false

    var body: some View {
        let name = languages.first { $0.code == selection }?.name ?? "—"
        Button { picking = true } label: {
            VStack(alignment: .leading, spacing: 0) {
                Overline(title)
                HStack(spacing: 4) {
                    Text(name).font(Typography.headline).lineLimit(1)
                    if enabled {
                        Image(systemName: "chevron.down").iconFont(12, weight: .bold)
                    }
                }
                .foregroundStyle(Palette.text)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.background, in: RoundedRectangle(cornerRadius: Metrics.innerRadius, style: .continuous))
            .background(GeometryReader { geo in
                Color.clear
                    .onAppear { measure(geo) }
                    .onChange(of: geo.frame(in: .global)) { measure(geo) }
            })
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityLabel(title)
        .accessibilityValue(name)
        .accessibilityIdentifier(id)
        .popover(isPresented: $picking, attachmentAnchor: .rect(.bounds), arrowEdge: opensUp ? .bottom : .top) {
            PickerDropdown(
                title: dropdownTitle,
                options: languages.map(\.code),
                selected: selection,
                label: { code in languages.first { $0.code == code }?.name ?? code },
                id: { "language-\($0)" },
                anchorWidth: width,
                onSelect: { selection = $0 }
            )
        }
    }

    /// `.global` is the tile's own window, so this also holds inside a sheet.
    private func measure(_ geo: GeometryProxy) {
        let frame = geo.frame(in: .global)
        width = frame.width
        let window = Typography.windowHeight
        opensUp = window - frame.maxY < frame.minY
    }
}

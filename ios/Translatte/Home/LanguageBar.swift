import SwiftUI

/// From / swap / To. From lists installed source packs only; To lists every language. Each opens a
/// dropdown popover anchored to its chip, as on Android.
struct LanguageBar: View {
    @EnvironmentObject private var model: TranslatorModel
    @State private var pickingFrom = false
    @State private var pickingTo = false
    @State private var tileWidth: CGFloat = 0
    /// Low on screen (the camera's language sheet) the dropdowns open above their chips.
    @State private var opensUp = false

    var body: some View {
        HStack(spacing: 8) {
            tile("From", id: "from-picker", code: model.sourceCode, languages: model.sourceLanguages) { pickingFrom = true }
                .popover(isPresented: $pickingFrom, attachmentAnchor: .rect(.bounds), arrowEdge: opensUp ? .bottom : .top) {
                    dropdown("Translate from", languages: model.sourceLanguages, selected: model.sourceCode) { model.sourceCode = $0 }
                }
            Button(action: model.swapLanguages) {
                Image(systemName: "arrow.left.arrow.right")
                    .font(.system(size: 17, weight: .semibold))
                    .frame(width: 44, height: 44)
                    .foregroundStyle(Palette.onPrimaryContainer)
                    .background(Palette.primaryContainer, in: Circle())
            }
            .buttonStyle(.plain)
            .disabled(!model.canSwapLanguages)
            .opacity(model.canSwapLanguages ? 1 : 0.45)
            .accessibilityLabel("Swap languages")
            .accessibilityIdentifier("swap-languages")
            tile("To", id: "to-picker", code: model.targetCode, languages: model.targetLanguages) { pickingTo = true }
                .popover(isPresented: $pickingTo, attachmentAnchor: .rect(.bounds), arrowEdge: opensUp ? .bottom : .top) {
                    dropdown("Translate to", languages: model.targetLanguages, selected: model.targetCode) { model.targetCode = $0 }
                }
        }
        .padding(8)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
        .disabled(model.phase.isWorking)
    }

    private func dropdown(
        _ title: String,
        languages: [LanguageInfo],
        selected: String,
        onSelect: @escaping (String) -> Void
    ) -> some View {
        PickerDropdown(
            title: title,
            options: languages.map(\.code),
            selected: selected,
            label: { code in languages.first { $0.code == code }?.name ?? code },
            id: { "language-\($0)" },
            anchorWidth: tileWidth,
            onSelect: onSelect
        )
    }

    private func tile(
        _ title: String,
        id: String,
        code: String,
        languages: [LanguageInfo],
        action: @escaping () -> Void
    ) -> some View {
        let name = languages.first { $0.code == code }?.name ?? "—"
        return Button(action: action) {
            VStack(alignment: .leading, spacing: 0) {
                Overline(title)
                HStack(spacing: 4) {
                    Text(name).font(Typography.headline).lineLimit(1)
                    Image(systemName: "chevron.down").font(.system(size: 12, weight: .bold))
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
        .accessibilityLabel(title)
        .accessibilityValue(name)
        .accessibilityIdentifier(id)
    }

    private func measure(_ geo: GeometryProxy) {
        tileWidth = geo.size.width
        opensUp = geo.frame(in: .global).maxY > UIScreen.main.bounds.height * 0.55
    }
}

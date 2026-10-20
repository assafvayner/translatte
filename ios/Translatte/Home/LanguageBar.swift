import SwiftUI

/// From / swap / To. From lists installed source packs only; To lists every language.
struct LanguageBar: View {
    @EnvironmentObject private var model: TranslatorModel

    var body: some View {
        HStack(spacing: 8) {
            picker("From", id: "from-picker", selection: $model.sourceCode, languages: model.sourceLanguages)
            Button(action: model.swapLanguages) {
                Image(systemName: "arrow.left.arrow.right")
                    .font(.body.weight(.semibold))
                    .frame(width: 44, height: 44)
                    .foregroundStyle(Palette.onPrimaryContainer)
                    .background(Palette.primaryContainer, in: Circle())
            }
            .buttonStyle(.plain)
            .disabled(!model.canSwapLanguages)
            .opacity(model.canSwapLanguages ? 1 : 0.45)
            .accessibilityLabel("Swap languages")
            .accessibilityIdentifier("swap-languages")
            picker("To", id: "to-picker", selection: $model.targetCode, languages: model.targetLanguages)
        }
        .padding(8)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        .disabled(model.phase.isWorking)
    }

    private func picker(_ title: String, id: String, selection: Binding<String>, languages: [LanguageInfo]) -> some View {
        let name = languages.first { $0.code == selection.wrappedValue }?.name ?? "—"
        return Menu {
            Picker(title, selection: selection) {
                ForEach(languages, id: \.code) { Text($0.name).tag($0.code) }
            }
        } label: {
            VStack(alignment: .leading, spacing: 2) {
                Text(title.uppercased())
                    .font(.caption2)
                    .tracking(0.4)
                    .foregroundStyle(Palette.textMuted)
                HStack(spacing: 4) {
                    Text(name).font(.body.weight(.semibold)).lineLimit(1)
                    Image(systemName: "chevron.down").font(.caption2.weight(.bold))
                }
                .foregroundStyle(Palette.text)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.background, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .accessibilityLabel(title)
        .accessibilityValue(name)
        .accessibilityIdentifier(id)
    }
}

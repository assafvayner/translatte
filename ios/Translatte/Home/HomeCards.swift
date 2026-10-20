import SwiftUI

struct EmptyStateCard: View {
    var body: some View {
        VStack(spacing: 14) {
            LogoCup(size: 132)
            Text("Point at a sign, menu or label")
                .font(.display(22))
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.center)
            Text("Take a photo and the translation appears right on top of the text. Works fully offline.")
                .font(.callout)
                .foregroundStyle(Palette.textMuted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 260)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            RoundedRectangle(cornerRadius: 28, style: .continuous)
                .strokeBorder(Palette.outline, style: StrokeStyle(lineWidth: 2, dash: [7, 6]))
        )
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("empty-state")
    }
}

/// The last translated photo; tapping it reopens the result viewer.
struct LastResultCard: View {
    @EnvironmentObject private var model: TranslatorModel
    let last: LastResult
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 10) {
                TranslatedImage(image: last.image, result: last.result, rtl: model.targetIsRTL)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Palette.surfaceVariant)
                    .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
                HStack {
                    Text("Last translation").font(.headline)
                    Spacer()
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                }
                .foregroundStyle(Palette.text)
            }
            .padding(12)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 28, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Last translation")
        .accessibilityIdentifier("last-result")
    }
}

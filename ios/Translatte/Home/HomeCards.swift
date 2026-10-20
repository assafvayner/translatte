import SwiftUI

struct EmptyStateCard: View {
    var body: some View {
        VStack(spacing: 12) {
            LogoCup(size: 120)
            Text("Point at a sign, menu or label")
                .font(Typography.display)
                .foregroundStyle(Palette.text)
                .multilineTextAlignment(.center)
            Text("Take a photo and the translation appears right on top of the text. Works fully offline.")
                .font(Typography.body)
                .lineSpacing(Typography.bodyLineSpacing)
                .foregroundStyle(Palette.textMuted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(
            RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous)
                .strokeBorder(Palette.outline, style: StrokeStyle(lineWidth: 1.5, dash: [8, 6]))
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
                    .clipShape(RoundedRectangle(cornerRadius: Metrics.innerRadius, style: .continuous))
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
                HStack {
                    Text("Last translation").font(Typography.headline)
                    Spacer()
                    Image(systemName: "arrow.up.left.and.arrow.down.right")
                        .iconFont(17)
                }
                .foregroundStyle(Palette.text)
                .padding(.leading, 4)
            }
            .padding(12)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Metrics.cardRadius, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Last translation")
        .accessibilityIdentifier("last-result")
    }
}

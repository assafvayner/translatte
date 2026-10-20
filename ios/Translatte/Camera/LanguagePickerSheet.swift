import SwiftUI

struct LanguagePickerSheet: View {
    @ScaledMetric(relativeTo: .body) private var height: CGFloat = 200

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Languages")
                .font(Typography.title)
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)
            LanguageBar()
        }
        .padding(.horizontal, Metrics.gutter)
        .padding(.top, 24)
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.height(height), .large])
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(Metrics.sheetRadius)
        .presentationBackground(Palette.sheet)
    }
}

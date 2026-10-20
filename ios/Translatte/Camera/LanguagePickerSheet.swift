import SwiftUI

struct LanguagePickerSheet: View {
    @ScaledMetric(relativeTo: .body) private var height: CGFloat = 190

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Languages")
                .font(.headline)
                .foregroundStyle(Palette.text)
            LanguageBar()
        }
        .padding(20)
        .presentationDetents([.height(height), .large])
        .presentationBackground(Palette.sheet)
    }
}

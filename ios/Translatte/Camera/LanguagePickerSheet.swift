import SwiftUI

struct LanguagePickerSheet: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Languages")
                .font(.headline)
                .foregroundStyle(Palette.text)
            LanguageBar()
        }
        .padding(20)
        .presentationDetents([.height(190)])
        .presentationBackground(Palette.sheet)
    }
}

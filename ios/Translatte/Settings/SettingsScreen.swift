import SwiftUI

struct SettingsScreen: View {
    @AppStorage(AppTheme.storageKey) private var theme = AppTheme.system
    @EnvironmentObject private var dialogs: DialogCenter
    @Environment(\.openURL) private var openURL

    var body: some View {
        ScrollView {
            VStack(spacing: Metrics.spacing) {
                Card(title: "Theme") {
                    ForEach(AppTheme.allCases) { option in
                        CheckRow(title: option.title, subtitle: option.description, selected: option == theme) {
                            theme = option
                        }
                        .accessibilityIdentifier("theme-\(option.rawValue)")
                    }
                }
                Card(title: "About") {
                    ListRow(title: "Version", subtitle: Bundle.main.appVersion)
                        .accessibilityElement(children: .combine)
                    AboutRow(title: "Text detection and recognition", subtitle: "PP-OCRv5 by PaddlePaddle · Apache License 2.0") {
                        openURL(URL(string: "https://www.apache.org/licenses/LICENSE-2.0")!)
                    }
                    .accessibilityIdentifier("credit-ocr")
                    AboutRow(title: "Translation", subtitle: "TranslateGemma 4B by Google · Gemma Terms of Use") {
                        openURL(URL(string: "https://ai.google.dev/gemma/terms")!)
                    }
                    .accessibilityIdentifier("credit-translation")
                    AboutRow(title: "Font", subtitle: "Fraunces · SIL Open Font License 1.1", external: false) {
                        dialogs.show(AppDialog(
                            title: "SIL Open Font License 1.1",
                            message: Self.license("OFL"),
                            scrolls: true,
                            actions: [.init(title: "Close")],
                            identifier: "license-dialog"
                        ))
                    }
                    .accessibilityIdentifier("credit-font")
                }
            }
            .padding(Metrics.gutter)
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarRole(.editor)
    }

    private static func license(_ resource: String) -> String {
        Bundle.main.url(forResource: resource, withExtension: "txt")
            .flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? ""
    }
}

private struct AboutRow: View {
    let title: String
    let subtitle: String
    var external = true
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ListRow(title: title, subtitle: subtitle) {
                Image(systemName: external ? "arrow.up.right.square" : "chevron.right")
                    .iconFont(17)
                    .foregroundStyle(Palette.textMuted)
                    .frame(width: 24, height: 24)
                    .accessibilityHidden(true)
            }
        }
        .buttonStyle(RowButtonStyle())
        .accessibilityElement(children: .combine)
    }
}

extension Bundle {
    var appVersion: String {
        object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }
}

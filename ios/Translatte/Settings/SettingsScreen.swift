import SwiftUI

struct SettingsScreen: View {
    @AppStorage(AppTheme.storageKey) private var theme = AppTheme.system

    var body: some View {
        Form {
            Section {
                Picker("Theme", selection: $theme) {
                    ForEach(AppTheme.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("theme-picker")
            } header: {
                Text("Theme")
            } footer: {
                Text("System uses translatte in light mode and americano in dark mode.")
                    .foregroundStyle(Palette.textMuted)
            }
            .listRowBackground(Palette.surface)
            Section("About") {
                LabeledContent("Version", value: Bundle.main.appVersion)
                Link(destination: URL(string: "https://www.apache.org/licenses/LICENSE-2.0")!) {
                    AboutRow(title: "Text detection and recognition", subtitle: "PP-OCRv5 by PaddlePaddle · Apache License 2.0")
                }
                .accessibilityIdentifier("credit-ocr")
                Link(destination: URL(string: "https://ai.google.dev/gemma/terms")!) {
                    AboutRow(title: "Translation", subtitle: "TranslateGemma 4B by Google · Gemma Terms of Use")
                }
                .accessibilityIdentifier("credit-translation")
                NavigationLink {
                    LicenseText(resource: "OFL")
                } label: {
                    AboutRow(title: "Wordmark font", subtitle: "Fraunces · SIL Open Font License 1.1", external: false)
                }
                .accessibilityIdentifier("credit-font")
            }
            .listRowBackground(Palette.surface)
        }
        .scrollContentBackground(.hidden)
        .background(Palette.background.ignoresSafeArea())
        .foregroundStyle(Palette.text)
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
    }
}

private struct AboutRow: View {
    let title: String
    let subtitle: String
    var external = true

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).foregroundStyle(Palette.text)
                Text(subtitle)
                    .font(.footnote)
                    .foregroundStyle(Palette.textMuted)
            }
            if external {
                Spacer()
                Image(systemName: "arrow.up.right.square")
                    .foregroundStyle(Palette.textMuted)
                    .accessibilityHidden(true)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

private struct LicenseText: View {
    let resource: String

    var body: some View {
        ScrollView {
            Text(text)
                .font(.footnote.monospaced())
                .foregroundStyle(Palette.text)
                .textSelection(.enabled)
                .padding()
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle("License")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var text: String {
        Bundle.main.url(forResource: resource, withExtension: "txt")
            .flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? ""
    }
}

extension Bundle {
    var appVersion: String {
        object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    }
}

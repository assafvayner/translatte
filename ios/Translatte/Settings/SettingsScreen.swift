import SwiftUI

struct SettingsScreen: View {
    @AppStorage(AppTheme.storageKey) private var theme = AppTheme.system

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Theme", selection: $theme) {
                    ForEach(AppTheme.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("theme-picker")
            }
            .listRowBackground(Palette.surface)
            Section("About") {
                LabeledContent("Version", value: Bundle.main.appVersion)
                Link(destination: URL(string: "https://huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det_onnx")!) {
                    LabeledContent("Text reading", value: "PP-OCRv5 · Apache 2.0")
                }
                Link(destination: URL(string: "https://huggingface.co/google/translategemma-4b-it")!) {
                    LabeledContent("Translation", value: "TranslateGemma · Gemma Terms")
                }
                Link("Apache License 2.0", destination: URL(string: "https://www.apache.org/licenses/LICENSE-2.0")!)
                Link("Gemma Terms of Use", destination: URL(string: "https://ai.google.dev/gemma/terms")!)
                NavigationLink("Fraunces font · SIL Open Font License") { LicenseText(resource: "OFL") }
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
        let version = object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
        let build = object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "?"
        return "\(version) (\(build))"
    }
}

import SwiftUI

@main
struct TranslatteApp: App {
    @StateObject private var model = TranslatorModel()
    @AppStorage(AppTheme.storageKey) private var theme = AppTheme.system
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
                .environmentObject(model.downloads)
                .preferredColorScheme(theme.colorScheme)
                .onChange(of: theme, initial: true) { _, theme in theme.applyToWindows() }
                .task { await model.runAutotestIfRequested() }
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background: model.releaseModels()
            case .active:
                model.reload()
                model.downloads.pump()
            default: break
            }
        }
    }
}

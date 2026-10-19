import SwiftUI

@main
struct TranslatteApp: App {
    @StateObject private var model = TranslatorModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
                .task { await model.runAutotestIfRequested() }
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background: model.releaseModels()
            case .active: model.prepare()
            default: break
            }
        }
    }
}

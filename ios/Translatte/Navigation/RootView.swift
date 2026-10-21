import SwiftUI

struct RootView: View {
    let camera: CameraController
    let live: LiveTranslator
    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var downloads: Downloads
    @EnvironmentObject private var navigator: AppNavigator

    var body: some View {
        Group {
            switch navigator.screen {
            case .setup: ModelsView(setup: true)
            case .camera: CameraScreen(camera: camera, live: live)
            case .home: HomeScreen()
            case .result: ResultScreen()
            }
        }
        .tint(Palette.primary)
        .onChange(of: model.modelsReady) { _, ready in navigator.modelsChanged(ready: ready) }
        .onChange(of: downloads.finished) { model.reload() }
    }
}

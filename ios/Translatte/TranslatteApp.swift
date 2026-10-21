import SwiftUI

@main
struct TranslatteApp: App {
    @StateObject private var model: TranslatorModel
    @StateObject private var camera: CameraController
    /// Held without observing: only LiveOverlay and LiveReadout redraw on live updates.
    @State private var live: LiveTranslator
    @StateObject private var navigator: AppNavigator
    @StateObject private var dialogs = DialogCenter()
    @AppStorage(AppTheme.storageKey) private var theme = AppTheme.system
    @Environment(\.scenePhase) private var scenePhase

    init() {
        Typography.applyNavigationBarAppearance()
        let model = TranslatorModel()
        let camera = CameraController(mockImagePath: UserDefaults.standard.string(forKey: "mock-camera"))
        _model = StateObject(wrappedValue: model)
        _camera = StateObject(wrappedValue: camera)
        let live = LiveTranslator { [model] frame in try await model.translateFrame(frame) }
        camera.onFrame { [weak live] buffer in live?.offer(buffer) }
        _live = State(wrappedValue: live)
        _navigator = StateObject(wrappedValue: AppNavigator(
            modelsReady: model.modelsReady,
            cameraAccess: camera.isMock ? .authorized : .current()
        ))
    }

    var body: some Scene {
        WindowGroup {
            RootView(camera: camera, live: live)
                .font(Typography.body)
                .accessibilityHidden(dialogs.current != nil)
                .overlay { DialogHost() }
                .onReceive(NotificationCenter.default.publisher(for: UIContentSizeCategory.didChangeNotification)) { _ in
                    Typography.applyNavigationBarAppearance()
                }
                .environmentObject(model)
                .environmentObject(model.downloads)
                .environmentObject(navigator)
                .environmentObject(dialogs)
                .preferredColorScheme(theme.colorScheme)
                .onChange(of: theme, initial: true) { _, theme in theme.applyToWindows() }
                .task {
                    if model.hasAutotestImage { navigator.showResult(from: .home) }
                    await model.runAutotestIfRequested()
                }
        }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .background: model.releaseModels()
            case .active:
                model.reload()
                model.downloads.pump()
                if !camera.isMock { navigator.cameraAccessChanged(.current()) }
            default: break
            }
        }
    }
}

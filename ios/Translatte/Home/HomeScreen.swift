import PhotosUI
import SwiftUI

/// The only route to Photos, Languages and Settings; Camera returns to the in-app camera.
struct HomeScreen: View {
    enum Route: Hashable { case languages, settings }

    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var navigator: AppNavigator
    @State private var path: [Route] = []
    @State private var pickerItem: PhotosPickerItem?
    @EnvironmentObject private var dialogs: DialogCenter

    var body: some View {
        NavigationStack(path: $path) {
            VStack(spacing: 16) {
                header
                LanguageBar()
                if let last = model.lastSuccess {
                    LastResultCard(last: last) {
                        model.showLastSuccess()
                        navigator.showResult(from: .home)
                    }
                } else {
                    EmptyStateCard()
                }
                if let notice = model.notice {
                    NoticeBanner(message: notice)
                        .accessibilityIdentifier("low-memory-notice")
                }
                actions
            }
            .padding(.horizontal, Metrics.gutter)
            .padding(.top, 8)
            .padding(.bottom, 16)
            .background(Palette.background.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { route in
                switch route {
                case .languages: ModelsView(setup: false).toolbarRole(.editor)
                case .settings: SettingsScreen()
                }
            }
        }
        .tint(Palette.primary)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("home")
        .onChange(of: pickerItem) { _, item in
            guard let item else { return }
            pickerItem = nil
            Task {
                let data = try? await item.loadTransferable(type: Data.self)
                guard navigator.screen == .home else { return }
                navigator.showResult(from: .home)
                if let data {
                    await model.translate(data)
                } else {
                    model.photoFailedToLoad()
                }
            }
        }
    }

    private func explainCamera() {
        dialogs.show(AppDialog(
            title: "Camera access is off",
            message: "translatte needs the camera to photograph text. Allow it in Settings, or choose a photo instead.",
            actions: [
                .init(title: "Not now"),
                .init(title: "Open Settings") {
                    if let url = URL(string: UIApplication.openSettingsURLString) {
                        UIApplication.shared.open(url)
                    }
                },
            ],
            identifier: "camera-access-dialog"
        ))
    }

    private var header: some View {
        HStack(spacing: 8) {
            LogoCup(size: 34)
            Wordmark()
            Spacer()
            Button { path.append(.languages) } label: {
                Label("Languages", systemImage: "globe")
            }
            .buttonStyle(SmallButtonStyle(kind: .outlined))
            .disabled(model.phase.isWorking)
            Button { path.append(.settings) } label: {
                Image(systemName: "gearshape")
                    .font(.system(size: 20))
                    .frame(width: 44, height: 44)
                    .foregroundStyle(Palette.text)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Settings")
            .accessibilityIdentifier("settings-button")
        }
    }

    private var actions: some View {
        let hasCamera = navigator.cameraAccess != .unavailable && navigator.cameraAccess != .restricted
        return HStack(spacing: 12) {
            if hasCamera {
                Button {
                    if navigator.cameraAccess == .denied {
                        explainCamera()
                    } else {
                        navigator.openCamera()
                    }
                } label: {
                    Label("Camera", systemImage: "camera")
                }
                .buttonStyle(BigButtonStyle(kind: .primary))
                .accessibilityIdentifier("camera-button")
            }
            PhotosPicker(selection: $pickerItem, matching: .images) {
                Label("Photos", systemImage: "photo.on.rectangle")
            }
            .buttonStyle(BigButtonStyle(kind: hasCamera ? .tonal : .primary))
            .accessibilityIdentifier("photos-button")
        }
    }
}

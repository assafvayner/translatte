import PhotosUI
import SwiftUI

/// The only route to Photos, Languages and Settings; Camera returns to the in-app camera.
struct HomeScreen: View {
    enum Route: Hashable { case languages, settings }

    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var navigator: AppNavigator
    @State private var path: [Route] = []
    @State private var pickerItem: PhotosPickerItem?
    @State private var explainingCamera = false

    var body: some View {
        NavigationStack(path: $path) {
            VStack(spacing: 16) {
                header
                LanguageBar()
                if model.image != nil {
                    LastResultCard { navigator.showResult(from: .home) }
                } else {
                    EmptyStateCard()
                }
                if let notice = model.notice {
                    NoticeBanner(message: notice)
                        .accessibilityIdentifier("low-memory-notice")
                }
                actions
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)
            .padding(.bottom, 16)
            .background(Palette.background.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar)
            .navigationDestination(for: Route.self) { route in
                switch route {
                case .languages: ModelsView(setup: false)
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
                navigator.showResult(from: .home)
                if let data, let picked = UIImage(data: data) {
                    await model.translate(picked)
                } else {
                    model.photoFailedToLoad()
                }
            }
        }
        .alert("Camera access is off", isPresented: $explainingCamera) {
            Button("Open Settings") {
                if let url = URL(string: UIApplication.openSettingsURLString) {
                    UIApplication.shared.open(url)
                }
            }
            Button("Not now", role: .cancel) {}
        } message: {
            Text("Translatte uses the camera to photograph the text you want translated. You can turn access on in Settings.")
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            LogoCup(size: 34)
            Wordmark(size: 26)
            Spacer()
            Button { path.append(.languages) } label: {
                Label("Languages", systemImage: "globe")
                    .font(.subheadline.weight(.medium))
                    .padding(.horizontal, 14)
                    .frame(height: 44)
                    .foregroundStyle(Palette.text)
                    .overlay(Capsule().strokeBorder(Palette.outline))
            }
            .buttonStyle(.plain)
            .disabled(model.phase.isWorking)
            Button { path.append(.settings) } label: {
                Image(systemName: "gearshape")
                    .font(.title3)
                    .frame(width: 44, height: 44)
                    .foregroundStyle(Palette.text)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Settings")
            .accessibilityIdentifier("settings-button")
        }
    }

    private var actions: some View {
        let hasCamera = navigator.cameraAccess != .unavailable
        return HStack(spacing: 12) {
            if hasCamera {
                Button {
                    if navigator.cameraAccess == .denied {
                        explainingCamera = true
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
        .disabled(model.phase.isWorking)
    }
}

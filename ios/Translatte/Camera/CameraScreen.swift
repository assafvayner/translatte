import AVFoundation
import SwiftUI

/// The full-screen camera the app opens to once it's set up.
struct CameraScreen: View {
    @ObservedObject var camera: CameraController
    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var navigator: AppNavigator
    @Environment(\.scenePhase) private var scenePhase
    @State private var pickingLanguages = false
    @State private var capturing = false

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            viewfinder
            VStack(spacing: 0) {
                topBar
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                Spacer()
                bottomBar
            }
        }
        .foregroundStyle(CameraStyle.foreground)
        .buttonStyle(.plain)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("camera-screen")
        .statusBarHidden(true)
        .onAppear(perform: camera.reset)
        .task {
            if navigator.cameraAccess == .notDetermined {
                let granted = await AVCaptureDevice.requestAccess(for: .video)
                navigator.cameraAccessChanged(granted ? .authorized : .current())
            }
            if navigator.cameraAccess == .authorized { camera.start() }
        }
        .onDisappear { camera.stop() }
        .onChange(of: scenePhase) { _, phase in
            guard navigator.cameraAccess == .authorized else { return }
            if phase == .active {
                camera.start()
            } else if phase == .background {
                camera.stop()
            }
        }
        .sheet(isPresented: $pickingLanguages) {
            LanguagePickerSheet().environmentObject(model)
        }
    }

    @ViewBuilder private var viewfinder: some View {
        if let mock = camera.mockImage {
            Color.clear
                .overlay { Image(uiImage: mock).resizable().scaledToFill() }
                .clipped()
                .ignoresSafeArea()
                .accessibilityHidden(true)
        } else {
            CameraPreview(session: camera.session)
                .ignoresSafeArea()
                .accessibilityHidden(true)
        }
    }

    private var topBar: some View {
        HStack(spacing: 10) {
            ScrimButton(systemImage: "chevron.left", label: "Back to Home", action: navigator.closeCamera)
                .accessibilityIdentifier("camera-close")
            Button { pickingLanguages = true } label: {
                Text(model.directionLabel)
                    .font(.subheadline.weight(.semibold))
                    .padding(.horizontal, 16)
                    .frame(height: 44)
                    .background(CameraStyle.control, in: Capsule())
            }
            .accessibilityIdentifier("camera-language-chip")
            Spacer()
            ScrimButton(
                systemImage: camera.flashOn ? "bolt.fill" : "bolt.slash",
                label: camera.flashOn ? "Flash on" : "Flash off",
                action: camera.toggleFlash
            )
            .accessibilityIdentifier("camera-flash")
        }
    }

    private var bottomBar: some View {
        HStack {
            Color.clear.frame(width: 52, height: 52)
            Spacer()
            Button(action: shoot) {
                Circle()
                    .fill(Palette.primary)
                    .overlay(Circle().strokeBorder(CameraStyle.foreground, lineWidth: 5))
                    .frame(width: 80, height: 80)
            }
            .accessibilityLabel("Take photo")
            .accessibilityIdentifier("camera-shutter")
            Spacer()
            Button(action: camera.toggleZoom) {
                Text(camera.zoomed ? "2×" : "1×")
                    .font(.subheadline.weight(.semibold))
                    .frame(width: 52, height: 52)
                    .background(CameraStyle.control, in: Circle())
            }
            .accessibilityLabel("Zoom")
            .accessibilityValue(camera.zoomed ? "2×" : "1×")
            .accessibilityIdentifier("camera-zoom")
        }
        .padding(.horizontal, 28)
        .frame(maxWidth: .infinity)
        .frame(height: 170)
        .background(CameraStyle.scrim.ignoresSafeArea(edges: .bottom))
    }

    private func shoot() {
        guard !capturing else { return }
        capturing = true
        Task {
            let photo = await camera.capture()
            capturing = false
            guard let photo, navigator.screen == .camera else { return }
            navigator.showResult(from: .camera)
            await model.translate(photo)
        }
    }
}

private enum CameraStyle {
    static let foreground = Color(red: 0xF1 / 255, green: 0xE6 / 255, blue: 0xD8 / 255)
    static let control = Color(red: 0x16 / 255, green: 0x11 / 255, blue: 0x0E / 255).opacity(0.6)
    static let scrim = Color(red: 0x16 / 255, green: 0x11 / 255, blue: 0x0E / 255).opacity(0.55)
}

private struct ScrimButton: View {
    let systemImage: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .font(.body.weight(.semibold))
                .frame(width: 44, height: 44)
                .background(CameraStyle.control, in: Circle())
        }
        .accessibilityLabel(label)
    }
}

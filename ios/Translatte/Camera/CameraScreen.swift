import AVFoundation
import SwiftUI

/// The full-screen camera the app opens to once it's set up.
struct CameraScreen: View {
    @ObservedObject var camera: CameraController
    let live: LiveTranslator
    @AppStorage(LiveRules.storageKey) private var liveSetting = true
    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var navigator: AppNavigator
    @Environment(\.scenePhase) private var scenePhase
    @State private var pickingLanguages = false
    @State private var capturing = false

    private var liveActive: Bool {
        LiveRules.isActive(enabled: liveSetting, sourceInstalled: model.sourceLanguages.contains { $0.code == model.sourceCode })
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            viewfinder
            if liveActive {
                LiveOverlay(live: live, rtl: model.targetIsRTL)
                    .ignoresSafeArea()
                    .allowsHitTesting(false)
            }
            VStack(spacing: 0) {
                LinearGradient(colors: [CameraStyle.edge, .clear], startPoint: .top, endPoint: .bottom)
                    .frame(height: 96)
                    .ignoresSafeArea(edges: .top)
                Spacer()
            }
            .allowsHitTesting(false)
            VStack(spacing: 0) {
                topBar
                    .padding(.horizontal, Metrics.gutter)
                    .padding(.top, 8)
                LiveReadout(live: live)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, Metrics.gutter)
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
        .onDisappear {
            camera.stop()
            live.enabled = false
            live.reset()
        }
        .onChange(of: scenePhase) { _, phase in
            live.enabled = phase == .active && liveActive
            guard navigator.cameraAccess == .authorized else { return }
            if phase == .active {
                camera.start()
            } else if phase == .background {
                camera.stop()
            }
        }
        .onChange(of: liveActive, initial: true) { _, active in
            live.enabled = active && scenePhase == .active
            camera.setFramesEnabled(active)
            if !active { live.reset() }
        }
        .onChange(of: model.sourceCode) { live.reset() }
        .task(id: liveActive) {
            guard liveActive else { return }
            if let replay = camera.replay {
                await live.runReplay(replay)
            } else if let mock = camera.mockImage {
                await live.runMock(mock)
            }
        }
        .sheet(isPresented: $pickingLanguages) {
            LanguagePickerSheet().environmentObject(model)
        }
    }

    @ViewBuilder private var viewfinder: some View {
        if let replay = camera.replay {
            ReplayPreview(player: replay.player)
                .ignoresSafeArea()
                .accessibilityHidden(true)
        } else if let mock = camera.mockImage {
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
        HStack(spacing: 8) {
            ScrimButton(systemImage: "chevron.left", label: "Back to Home", action: navigator.closeCamera)
                .accessibilityIdentifier("camera-close")
            Button { pickingLanguages = true } label: {
                Text(model.directionLabel)
                    .font(Typography.label)
                    .lineLimit(1)
                    .padding(.horizontal, 16)
                    .frame(height: Metrics.chipHeight)
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
                    .padding(11)
                    .overlay(Circle().strokeBorder(CameraStyle.foreground, lineWidth: 4))
                    .frame(width: 80, height: 80)
            }
            .accessibilityLabel("Take photo")
            .accessibilityIdentifier("camera-shutter")
            Spacer()
            Button(action: camera.toggleZoom) {
                Text(camera.zoomed ? "2×" : "1×")
                    .font(Typography.label)
                    .frame(width: 52, height: 52)
                    .background(CameraStyle.control, in: Circle())
            }
            .accessibilityLabel("Zoom")
            .accessibilityValue(camera.zoomed ? "2×" : "1×")
            .accessibilityIdentifier("camera-zoom")
        }
        .padding(.horizontal, 28)
        .frame(maxWidth: .infinity)
        .padding(.bottom, 24)
        .background(alignment: .bottom) {
            LinearGradient(colors: [.clear, CameraStyle.edge], startPoint: .top, endPoint: .bottom)
                .frame(height: 160)
                .ignoresSafeArea(edges: .bottom)
                .allowsHitTesting(false)
        }
    }

    private func shoot() {
        guard !capturing else { return }
        capturing = true
        Task {
            if let frozen = await live.snapshot() {
                capturing = false
                guard navigator.screen == .camera else { return }
                model.showLive(image: frozen.image, result: frozen.result)
                navigator.showResult(from: .camera)
                return
            }
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
    static let edge = Color(red: 0x16 / 255, green: 0x11 / 255, blue: 0x0E / 255).opacity(0.55)
}

private struct ScrimButton: View {
    let systemImage: String
    let label: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemImage)
                .iconFont(17, weight: .semibold)
                .frame(width: 44, height: 44)
                .background(CameraStyle.control, in: Circle())
        }
        .accessibilityLabel(label)
    }
}

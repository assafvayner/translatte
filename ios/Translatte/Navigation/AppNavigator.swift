import AVFoundation
import SwiftUI

enum CameraAccess: Equatable {
    case authorized, notDetermined, denied, unavailable

    static func current(
        hasCamera: Bool = AVCaptureDevice.default(for: .video) != nil,
        status: AVAuthorizationStatus = AVCaptureDevice.authorizationStatus(for: .video)
    ) -> CameraAccess {
        guard hasCamera else { return .unavailable }
        switch status {
        case .authorized: return .authorized
        case .notDetermined: return .notDetermined
        default: return .denied
        }
    }
}

enum AppScreen: Equatable {
    enum Origin: Equatable { case camera, home }

    case setup, camera, home
    case result(from: Origin)
}

/// Which root screen is showing. Setup while required models are missing; then the camera, or Home
/// when there's no camera to use; the result viewer returns to wherever the photo came from.
@MainActor
final class AppNavigator: ObservableObject {
    @Published private(set) var screen: AppScreen
    @Published private(set) var cameraAccess: CameraAccess

    init(modelsReady: Bool, cameraAccess: CameraAccess) {
        self.cameraAccess = cameraAccess
        screen = Self.landing(modelsReady: modelsReady, camera: cameraAccess)
    }

    static func landing(modelsReady: Bool, camera: CameraAccess) -> AppScreen {
        guard modelsReady else { return .setup }
        switch camera {
        case .authorized, .notDetermined: return .camera
        case .denied, .unavailable: return .home
        }
    }

    func modelsChanged(ready: Bool) {
        if !ready {
            screen = .setup
        } else if screen == .setup {
            screen = Self.landing(modelsReady: true, camera: cameraAccess)
        }
    }

    func cameraAccessChanged(_ access: CameraAccess) {
        cameraAccess = access
        if screen == .camera, access == .denied || access == .unavailable {
            screen = .home
        }
    }

    func openCamera() {
        if cameraAccess == .authorized || cameraAccess == .notDetermined {
            screen = .camera
        }
    }

    func closeCamera() {
        screen = .home
    }

    func showResult(from origin: AppScreen.Origin) {
        screen = .result(from: origin)
    }

    func closeResult() {
        guard case let .result(origin) = screen else { return }
        screen = origin == .camera ? .camera : .home
    }
}

import AVFoundation
import UIKit

/// The in-app camera's capture session. `-mock-camera <path>` swaps in a still image so the simulator,
/// which has no camera, can run the camera screen and its UI tests.
final class CameraController: NSObject, ObservableObject, @unchecked Sendable {
    @Published private(set) var flashOn = false
    @Published private(set) var zoomed = false
    let session = AVCaptureSession()
    let mockImage: UIImage?
    var isMock: Bool { mockImage != nil }

    /// Owns `session`, `output`, `device` and `captures`.
    private let queue = DispatchQueue(label: "translatte.camera")
    private let output = AVCapturePhotoOutput()
    private var device: AVCaptureDevice?
    private var configured = false
    private var captures: [Int64: PhotoCapture] = [:]

    init(mockImagePath: String?) {
        mockImage = mockImagePath.flatMap { UIImage(contentsOfFile: $0) }
        super.init()
    }

    func start() {
        guard !isMock else { return }
        queue.async { [self] in
            if !configured { configure() }
            if !session.isRunning { session.startRunning() }
        }
    }

    func stop() {
        guard !isMock else { return }
        queue.async { [self] in
            if session.isRunning { session.stopRunning() }
        }
    }

    func toggleFlash() {
        flashOn.toggle()
    }

    func toggleZoom() {
        zoomed.toggle()
        let factor: CGFloat = zoomed ? 2 : 1
        queue.async { [self] in
            guard let device, (try? device.lockForConfiguration()) != nil else { return }
            device.videoZoomFactor = min(factor, device.activeFormat.videoMaxZoomFactor)
            device.unlockForConfiguration()
        }
    }

    /// A full-resolution still, or nil when the session isn't running or the capture failed.
    func capture() async -> UIImage? {
        if let mockImage { return mockImage }
        let flash = flashOn
        return await withCheckedContinuation { continuation in
            queue.async { [self] in
                guard session.isRunning else { return continuation.resume(returning: nil) }
                let settings = AVCapturePhotoSettings()
                settings.maxPhotoDimensions = output.maxPhotoDimensions
                if output.supportedFlashModes.contains(.on) {
                    settings.flashMode = flash ? .on : .off
                }
                let id = settings.uniqueID
                let capture = PhotoCapture { [weak self] image in
                    self?.queue.async { self?.captures[id] = nil }
                    continuation.resume(returning: image)
                }
                captures[id] = capture
                output.capturePhoto(with: settings, delegate: capture)
            }
        }
    }

    private func configure() {
        configured = true
        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
              let input = try? AVCaptureDeviceInput(device: device) else { return }
        session.beginConfiguration()
        session.sessionPreset = .photo
        if session.canAddInput(input) { session.addInput(input) }
        if session.canAddOutput(output) { session.addOutput(output) }
        let largest = device.activeFormat.supportedMaxPhotoDimensions.max {
            Int($0.width) * Int($0.height) < Int($1.width) * Int($1.height)
        }
        if let largest { output.maxPhotoDimensions = largest }
        session.commitConfiguration()
        self.device = device
    }
}

/// AVCapturePhotoOutput doesn't keep its delegate alive, so `CameraController.captures` does.
private final class PhotoCapture: NSObject, AVCapturePhotoCaptureDelegate {
    private let done: (UIImage?) -> Void

    init(done: @escaping (UIImage?) -> Void) {
        self.done = done
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishProcessingPhoto photo: AVCapturePhoto, error: Error?) {
        done(error == nil ? photo.fileDataRepresentation().flatMap(UIImage.init(data:)) : nil)
    }
}

import AVFoundation
import UIKit

/// The in-app camera's capture session, plus live frames while live translation is on. `-mock-camera <path>` swaps in a still image so the simulator,
/// which has no camera, can run the camera screen and its UI tests; in debug builds, `-mock-video <path>` swaps in a video (see `LiveReplay`).
final class CameraController: NSObject, ObservableObject, @unchecked Sendable {
    /// Captures are capped near 12 MP: OCR downsamples to 2048px anyway, and 48 MP stills cost
    /// hundreds of MB to hold and decode.
    static let maxCapturePixels = 4032 * 3024

    @MainActor @Published private(set) var flashOn = false
    @MainActor @Published private(set) var zoomed = false
    let session = AVCaptureSession()
    let mockImage: UIImage?
    private let mockData: Data?
    let replay: LiveReplay?
    var isMock: Bool { mockImage != nil || replay != nil }

    /// Owns `session`, `output`, `device`, `rotation` and `captures`.
    private let queue = DispatchQueue(label: "translatte.camera")
    private let output = AVCapturePhotoOutput()
    private var device: AVCaptureDevice?
    private var rotation: AVCaptureDevice.RotationCoordinator?
    private var configured = false
    private var captures: [Int64: PhotoCapture] = [:]
    private let frameQueue = DispatchQueue(label: "translatte.camera.frames")
    private let videoOutput = AVCaptureVideoDataOutput()
    /// Owned by `queue`.
    private var framesWanted = false
    /// Owned by `frameQueue`.
    private var loggedFrameSize = false
    private let handlerLock = NSLock()
    private var frameHandler: (@Sendable (CVPixelBuffer) -> Void)?

    @MainActor init(mockImagePath: String?, mockVideoPath: String? = nil, loopVideo: Bool = false) {
        mockData = mockImagePath.flatMap { FileManager.default.contents(atPath: $0) }
        mockImage = mockData.flatMap(UIImage.init(data:))
        replay = mockVideoPath.map { LiveReplay(path: $0, loop: loopVideo) }
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

    /// Receives live frames on a background queue.
    func onFrame(_ handler: @escaping @Sendable (CVPixelBuffer) -> Void) {
        handlerLock.withLock { frameHandler = handler }
    }

    /// Adds or removes the live frame output; without it the camera is snap-only, as before.
    func setFramesEnabled(_ on: Bool) {
        guard !isMock else { return }
        queue.async { [self] in
            framesWanted = on
            if configured { applyFrames() }
        }
    }

    private func applyFrames() {
        let attached = session.outputs.contains(videoOutput)
        guard attached != framesWanted else { return }
        session.beginConfiguration()
        if framesWanted {
            if session.canAddOutput(videoOutput) { session.addOutput(videoOutput) }
        } else {
            session.removeOutput(videoOutput)
        }
        session.commitConfiguration()
    }

    /// Flash off and 1×, as each visit to the camera starts.
    @MainActor func reset() {
        flashOn = false
        if zoomed { toggleZoom() }
    }

    @MainActor func toggleFlash() {
        flashOn.toggle()
    }

    @MainActor func toggleZoom() {
        zoomed.toggle()
        let factor: CGFloat = zoomed ? 2 : 1
        queue.async { [self] in
            guard let device, (try? device.lockForConfiguration()) != nil else { return }
            device.videoZoomFactor = min(factor, device.activeFormat.videoMaxZoomFactor)
            device.unlockForConfiguration()
        }
    }

    /// The encoded still, or nil when the session isn't running or the capture failed.
    @MainActor func capture() async -> Data? {
        if let mockData { return mockData }
        let flash = flashOn
        return await withCheckedContinuation { continuation in
            queue.async { [self] in
                guard session.isRunning else { return continuation.resume(returning: nil) }
                let settings = AVCapturePhotoSettings()
                settings.maxPhotoDimensions = output.maxPhotoDimensions
                if output.supportedFlashModes.contains(.on) {
                    settings.flashMode = flash ? .on : .off
                }
                if let rotation, let connection = output.connection(with: .video) {
                    let angle = rotation.videoRotationAngleForHorizonLevelCapture
                    if connection.isVideoRotationAngleSupported(angle) { connection.videoRotationAngle = angle }
                }
                let id = settings.uniqueID
                let capture = PhotoCapture { [weak self] data in
                    self?.queue.async { self?.captures[id] = nil }
                    continuation.resume(returning: data)
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
        if let dimensions = Self.captureDimensions(device.activeFormat.supportedMaxPhotoDimensions) {
            output.maxPhotoDimensions = dimensions
        }
        videoOutput.videoSettings = [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA]
        videoOutput.alwaysDiscardsLateVideoFrames = true
        videoOutput.automaticallyConfiguresOutputBufferDimensions = false
        videoOutput.deliversPreviewSizedOutputBuffers = true
        videoOutput.setSampleBufferDelegate(self, queue: frameQueue)
        session.commitConfiguration()
        self.device = device
        rotation = AVCaptureDevice.RotationCoordinator(device: device, previewLayer: nil)
        applyFrames()
    }

    /// The largest supported size within `maxCapturePixels`, or the smallest when none fits.
    static func captureDimensions(_ supported: [CMVideoDimensions]) -> CMVideoDimensions? {
        let pixels = { (d: CMVideoDimensions) in Int(d.width) * Int(d.height) }
        let sorted = supported.sorted { pixels($0) < pixels($1) }
        return sorted.last { pixels($0) <= maxCapturePixels } ?? sorted.first
    }
}

extension CameraController: AVCaptureVideoDataOutputSampleBufferDelegate {
    func captureOutput(_ output: AVCaptureOutput, didOutput sampleBuffer: CMSampleBuffer, from connection: AVCaptureConnection) {
        guard let buffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        #if DEBUG
        if !loggedFrameSize {
            loggedFrameSize = true
            print("live frames: \(CVPixelBufferGetWidth(buffer))x\(CVPixelBufferGetHeight(buffer))")
        }
        #endif
        handlerLock.withLock { frameHandler }?(buffer)
    }
}

/// AVCapturePhotoOutput doesn't keep its delegate alive, so `CameraController.captures` does.
/// Calls `done` exactly once: with the photo, or with nil if the capture ends without one.
private final class PhotoCapture: NSObject, AVCapturePhotoCaptureDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private var done: ((Data?) -> Void)?

    init(done: @escaping (Data?) -> Void) {
        self.done = done
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishProcessingPhoto photo: AVCapturePhoto, error: Error?) {
        finish(error == nil ? photo.fileDataRepresentation() : nil)
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishCaptureFor resolvedSettings: AVCaptureResolvedPhotoSettings, error: Error?) {
        finish(nil)
    }

    private func finish(_ data: Data?) {
        let done = lock.withLock {
            defer { self.done = nil }
            return self.done
        }
        done?(data)
    }
}

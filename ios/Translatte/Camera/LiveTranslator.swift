import CoreVideo
import UIKit

/// One camera frame as the engine takes it: tightly packed RGBA rows, before rotating upright.
struct LiveFrame {
    let rgba: Data
    let width: Int
    let height: Int
    /// Clockwise degrees that turn the frame upright.
    let rotation: Int
    /// A replayed video frame's position.
    var ptsMs: Int64?

    init(rgba: Data, width: Int, height: Int, rotation: Int) {
        self.rgba = rgba
        self.width = width
        self.height = height
        self.rotation = rotation
    }

    /// A BGRA camera buffer, scaled down to `LiveRules.maxFrameSide`.
    init?(pixelBuffer: CVPixelBuffer, rotation: Int) {
        guard CVPixelBufferGetPixelFormatType(pixelBuffer) == kCVPixelFormatType_32BGRA else { return nil }
        CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(pixelBuffer) else { return nil }
        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let stride = CVPixelBufferGetBytesPerRow(pixelBuffer)
        let factor = LiveRules.downscaleFactor(width: width, height: height)
        let rgba = LiveRules.packRgba(
            UnsafeRawBufferPointer(start: base, count: stride * height),
            width: width, height: height, rowStride: stride, factor: factor, swapRedBlue: true
        )
        self.init(rgba: rgba, width: width / factor, height: height / factor, rotation: rotation)
    }

    /// A still image (the simulator's mock camera), already upright.
    init?(image: UIImage) {
        guard let cg = image.cgImage else { return nil }
        let width = cg.width, height = cg.height
        var bytes = Data(count: width * height * 4)
        let drawn = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            ) else { return false }
            context.draw(cg, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return nil }
        self.init(rgba: bytes, width: width, height: height, rotation: 0)
    }

    /// The frame as an upright photo for the result viewer.
    func image() -> UIImage? {
        guard let provider = CGDataProvider(data: rgba as CFData),
              let cg = CGImage(
                width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                provider: provider, decode: nil, shouldInterpolate: true, intent: .defaultIntent
              ) else { return nil }
        let orientation: UIImage.Orientation
        switch rotation {
        case 90: orientation = .right
        case 180: orientation = .down
        case 270: orientation = .left
        default: orientation = .up
        }
        return UIImage(cgImage: cg, scale: 1, orientation: orientation)
    }
}

/// Live translation of camera frames, mirroring Android's `LiveTranslator`: at most one frame in
/// flight (the newest frame after it finishes is next), paced by `LiveRules`, with results steadied
/// by `LiveOverlayTracker`.
@MainActor
final class LiveTranslator: ObservableObject {
    @Published private(set) var overlay: [TrackedRegion] = []
    @Published private(set) var imageSize: CGSize = .zero
    /// Debug builds: updates/s and stage times of the latest update.
    @Published private(set) var readout: String?

    private let translate: (LiveFrame) async throws -> TranslationResult
    private let gate = LiveGate()
    private var tracker = LiveOverlayTracker()
    private var latest: (frame: LiveFrame, result: TranslationResult)?
    private var lastDoneMs: Int64?
    private var generation = 0
    private var pruner: Task<Void, Never>?
    private let events: LiveEventLog?

    init(translate: @escaping (LiveFrame) async throws -> TranslationResult) {
        self.translate = translate
        #if DEBUG
        events = LiveEventLog(emit: LiveEventSink.write)
        #else
        events = nil
        #endif
    }

    var enabled: Bool {
        get { gate.enabled }
        set { gate.enabled = newValue }
    }

    /// Clears the overlay and drops results still in flight (source change, live turned off, leaving the camera).
    func reset() {
        generation += 1
        pruner?.cancel()
        pruner = nil
        gate.clearFailures()
        events?.reset(nowMs: Self.nowMs(), prev: overlay)
        tracker.reset()
        overlay = []
        latest = nil
        lastDoneMs = nil
        readout = nil
    }

    /// The latest live frame, upright, and its result when snap should show them rather than take a photo.
    func snapshot() async -> (image: UIImage, result: TranslationResult)? {
        guard let (frame, result) = latest, LiveRules.snapUsesLive(result) else { return nil }
        guard let image = await Task.detached(priority: .userInitiated, operation: { frame.image() }).value else { return nil }
        return (image, result)
    }

    /// Runs on the camera's frame queue for every frame; converts and translates only those `LiveRules` lets through.
    /// The UI is portrait-only, so back-camera buffers need a clockwise quarter turn.
    nonisolated func offer(_ pixelBuffer: CVPixelBuffer, rotation: Int = 90, ptsMs: Int64? = nil) {
        guard gate.tryTake(nowMs: Self.nowMs(), hot: ThermalMonitor.shared.hot) else { return }
        guard var frame = LiveFrame(pixelBuffer: pixelBuffer, rotation: rotation) else { return gate.finish(failed: true, nowMs: Self.nowMs()) }
        frame.ptsMs = ptsMs
        Task { @MainActor in await self.run(frame) }
    }

    /// Debug builds: plays `replay` in place of the camera, offering its frames, until cancelled.
    /// Event times count from the start of the video.
    func runReplay(_ replay: LiveReplay) async {
        await replay.play(
            started: { [weak self] positionMs in self?.events?.start(nowMs: Self.nowMs() - positionMs) },
            frame: { [weak self] buffer, rotation, ptsMs in self?.offer(buffer, rotation: rotation, ptsMs: ptsMs) }
        )
    }

    /// The simulator has no camera: feeds the mock still as frames until cancelled.
    func runMock(_ image: UIImage) async {
        guard let frame = LiveFrame(image: image) else { return }
        while !Task.isCancelled {
            if gate.tryTake(nowMs: Self.nowMs(), hot: ThermalMonitor.shared.hot) { await run(frame) }
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
    }

    private func run(_ frame: LiveFrame) async {
        let run = generation
        do {
            let result = try await translate(frame)
            gate.finish(failed: false, nowMs: Self.nowMs())
            guard run == generation else { return }
            publish(frame, result)
        } catch {
            gate.finish(failed: true, nowMs: Self.nowMs())
            NSLog("translatte: live frame failed: %@", "\(error)")
        }
    }

    private func publish(_ frame: LiveFrame, _ result: TranslationResult) {
        let now = Self.nowMs()
        let prev = overlay
        overlay = tracker.update(result.regions, nowMs: now)
        if !overlay.isEmpty, pruner == nil { pruner = Task { await self.prune() } }
        imageSize = CGSize(width: Int(result.imageWidth), height: Int(result.imageHeight))
        latest = (frame, result)
        #if DEBUG
        let line = LiveRules.readout(intervalMs: lastDoneMs.map { now - $0 }, timings: result.timings)
        readout = line
        NSLog("translatte: live %@ regions %d", line, result.regions.count)
        events?.update(nowMs: now, prev: prev, next: overlay, result: result, intervalMs: lastDoneMs.map { now - $0 }, ptsMs: frame.ptsMs)
        #endif
        lastDoneMs = now
    }

    private func prune() async {
        while !Task.isCancelled, !overlay.isEmpty {
            try? await Task.sleep(nanoseconds: UInt64(LiveRules.pruneIntervalMs) * 1_000_000)
            guard !Task.isCancelled else { return }
            let now = Self.nowMs()
            let kept = tracker.prune(nowMs: now)
            if kept.count != overlay.count {
                events?.prune(nowMs: now, prev: overlay, next: kept)
                overlay = kept
            }
        }
        pruner = nil
    }

    private nonisolated static func nowMs() -> Int64 {
        Int64(ProcessInfo.processInfo.systemUptime * 1000)
    }
}

/// Pacing state shared by the camera's frame queue and the main actor.
private final class LiveGate: @unchecked Sendable {
    private let lock = NSLock()
    private var isEnabled = false
    private var busy = false
    private var lastStartMs: Int64?
    private var failures = 0
    private var lastFailMs: Int64?

    var enabled: Bool {
        get { lock.withLock { isEnabled } }
        set { lock.withLock { isEnabled = newValue } }
    }

    func tryTake(nowMs: Int64, hot: Bool) -> Bool {
        lock.withLock {
            guard isEnabled, LiveRules.shouldTake(
                busy: busy, hot: hot, nowMs: nowMs, lastStartMs: lastStartMs, failures: failures, lastFailMs: lastFailMs
            ) else {
                return false
            }
            busy = true
            lastStartMs = nowMs
            return true
        }
    }

    func finish(failed: Bool, nowMs: Int64) {
        lock.withLock {
            busy = false
            if failed {
                failures += 1
                lastFailMs = nowMs
            } else {
                failures = 0
            }
        }
    }

    func clearFailures() {
        lock.withLock {
            failures = 0
            lastFailMs = nil
        }
    }
}

/// The thermal state, kept current by notification so frames needn't query it.
private final class ThermalMonitor: @unchecked Sendable {
    static let shared = ThermalMonitor()
    private let lock = NSLock()
    private var isHot = ThermalMonitor.read()

    private init() {
        NotificationCenter.default.addObserver(
            forName: ProcessInfo.thermalStateDidChangeNotification, object: nil, queue: nil
        ) { [weak self] _ in
            guard let self else { return }
            let hot = Self.read()
            lock.withLock { self.isHot = hot }
        }
    }

    var hot: Bool { lock.withLock { isHot } }

    private static func read() -> Bool {
        ProcessInfo.processInfo.thermalState.rawValue >= ProcessInfo.ThermalState.serious.rawValue
    }
}

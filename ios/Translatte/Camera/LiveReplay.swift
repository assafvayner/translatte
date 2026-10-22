import AVFoundation
import SwiftUI

/// Debug builds: a video played at real time in place of the camera (`-mock-video <path>`, plus
/// `-mock-video-loop YES` to loop it rather than stop at the end).
@MainActor
final class LiveReplay {
    let player: AVPlayer
    private let item: AVPlayerItem
    private let output = AVPlayerItemVideoOutput(pixelBufferAttributes: [
        kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
    ])
    private let loop: Bool
    private var finished = false

    init(path: String, loop: Bool) {
        item = AVPlayerItem(url: URL(fileURLWithPath: path))
        item.add(output)
        player = AVPlayer(playerItem: item)
        player.isMuted = true
        player.actionAtItemEnd = loop ? .none : .pause
        self.loop = loop
    }

    /// Plays until cancelled, handing each new frame, its clockwise turn upright and its position
    /// in ms to `frame` on a background task; `started` gets the first frame's position. Logs
    /// "live replay done" at the end unless looping.
    func play(
        started: @escaping @MainActor (Int64) -> Void,
        frame: @escaping @Sendable (CVPixelBuffer, Int, Int64) -> Void
    ) async {
        let rotation = await Self.rotation(of: item.asset)
        guard !finished, !Task.isCancelled else { return }
        player.play()
        let output = output
        let poll = Task.detached(priority: .userInitiated) {
            var first = true
            while !Task.isCancelled {
                let time = output.itemTime(forHostTime: CACurrentMediaTime())
                if output.hasNewPixelBuffer(forItemTime: time),
                   let buffer = output.copyPixelBuffer(forItemTime: time, itemTimeForDisplay: nil) {
                    let ms = Int64(time.seconds * 1000)
                    if first {
                        first = false
                        await started(ms)
                    }
                    frame(buffer, rotation, ms)
                }
                try? await Task.sleep(nanoseconds: 8_000_000)
            }
        }
        defer {
            poll.cancel()
            player.pause()
        }
        while !Task.isCancelled, !finished {
            try? await Task.sleep(nanoseconds: 50_000_000)
            guard item.duration.isNumeric, item.currentTime() >= item.duration else { continue }
            if loop {
                await player.seek(to: .zero)
                player.play()
            } else {
                finished = true
                NSLog("translatte: live replay done")
            }
        }
        while !Task.isCancelled {
            try? await Task.sleep(nanoseconds: 100_000_000)
        }
    }

    /// Clockwise degrees that turn the video track's frames upright, from its preferred transform.
    private static func rotation(of asset: AVAsset) async -> Int {
        guard let track = try? await asset.loadTracks(withMediaType: .video).first,
              let t = try? await track.load(.preferredTransform) else { return 0 }
        let degrees = Int((atan2(t.b, t.a) * 180 / .pi).rounded())
        return (degrees + 360) % 360
    }
}

/// The replay's video, aspect-filled like the camera preview.
struct ReplayPreview: UIViewRepresentable {
    let player: AVPlayer

    func makeUIView(context: Context) -> PlayerView {
        let view = PlayerView()
        view.playerLayer.player = player
        view.playerLayer.videoGravity = .resizeAspectFill
        return view
    }

    func updateUIView(_ uiView: PlayerView, context: Context) {}

    final class PlayerView: UIView {
        override class var layerClass: AnyClass { AVPlayerLayer.self }
        var playerLayer: AVPlayerLayer { layer as! AVPlayerLayer }
    }
}

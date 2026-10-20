import CoreGraphics

/// The result viewer's zoom: `scale` around the container's center, then `offset`.
struct ZoomTransform: Equatable {
    static let minScale: CGFloat = 1
    static let maxScale: CGFloat = 6
    static let doubleTapScale: CGFloat = 2.5

    var scale: CGFloat = 1
    var offset: CGSize = .zero

    var isZoomed: Bool { scale > 1.01 }
    var percent: Int { Int((scale * 100).rounded()) }

    /// Keeps the scale in range and the zoomed photo covering the container wherever it's larger than it.
    mutating func clamp(content: CGSize, container: CGSize) {
        scale = min(max(scale, Self.minScale), Self.maxScale)
        let maxX = max(0, (content.width * scale - container.width) / 2)
        let maxY = max(0, (content.height * scale - container.height) / 2)
        offset = CGSize(
            width: min(max(offset.width, -maxX), maxX),
            height: min(max(offset.height, -maxY), maxY)
        )
    }

    /// `base` scaled by `magnification` around `anchor`, keeping the point under the pinch in place.
    mutating func pinch(from base: ZoomTransform, magnification: CGFloat, anchor: CGPoint, content: CGSize, container: CGSize) {
        let s = min(max(base.scale * magnification, Self.minScale), Self.maxScale)
        let ratio = s / base.scale
        let x = anchor.x - container.width / 2
        let y = anchor.y - container.height / 2
        scale = s
        offset = CGSize(width: x - (x - base.offset.width) * ratio, height: y - (y - base.offset.height) * ratio)
        clamp(content: content, container: container)
    }

    /// Back to fit when zoomed; otherwise `doubleTapScale`, keeping `point` under the finger.
    mutating func toggleDoubleTap(at point: CGPoint, content: CGSize, container: CGSize) {
        guard !isZoomed else {
            self = ZoomTransform()
            return
        }
        let s = Self.doubleTapScale
        scale = s
        offset = CGSize(
            width: (point.x - container.width / 2) * (1 - s),
            height: (point.y - container.height / 2) * (1 - s)
        )
        clamp(content: content, container: container)
    }

    /// Maps a point in the container to where it lies in the unzoomed layout.
    func unzoomed(_ point: CGPoint, container: CGSize) -> CGPoint {
        CGPoint(
            x: (point.x - container.width / 2 - offset.width) / scale + container.width / 2,
            y: (point.y - container.height / 2 - offset.height) / scale + container.height / 2
        )
    }
}

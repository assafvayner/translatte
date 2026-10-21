import Foundation

/// A live region whose identity survives updates; `from` is where its box was shown before this update.
struct TrackedRegion: Equatable {
    let id: Int
    let region: Region
    let from: [Point]
    let missed: Int
}

/// Keeps the live overlay steady between updates, with the same rules as Android's `LiveOverlayTracker`.
/// Each translated new region takes the not-yet-taken previous region it overlaps most (IoU at least
/// `minIoU`). With the same source text, it keeps that region's identity and translation and slides
/// from the old box. With different text, it replaces it. Previous regions nothing took stay for
/// `maxMissed` updates.
struct LiveOverlayTracker {
    static let minIoU: Float = 0.3
    static let maxMissed = 2

    private(set) var regions: [TrackedRegion] = []
    private var nextId = 0

    mutating func update(_ incoming: [Region]) -> [TrackedRegion] {
        var taken = Array(repeating: false, count: regions.count)
        var out: [TrackedRegion] = []
        for r in incoming where r.translatedText != nil {
            var best = -1
            var bestIoU: Float = 0
            for (i, old) in regions.enumerated() {
                let v = Self.iou(old.region.corners, r.corners)
                if !taken[i], v >= Self.minIoU, v > bestIoU {
                    best = i
                    bestIoU = v
                }
            }
            if best >= 0 { taken[best] = true }
            if best >= 0, regions[best].region.sourceText == r.sourceText {
                let old = regions[best]
                var kept = r
                kept.translatedText = old.region.translatedText
                out.append(TrackedRegion(id: old.id, region: kept, from: old.region.corners, missed: 0))
            } else {
                out.append(TrackedRegion(id: nextId, region: r, from: r.corners, missed: 0))
                nextId += 1
            }
        }
        for (i, old) in regions.enumerated() where !taken[i] && old.missed < Self.maxMissed {
            out.append(TrackedRegion(id: old.id, region: old.region, from: old.region.corners, missed: old.missed + 1))
        }
        regions = out
        return out
    }

    mutating func reset() {
        regions = []
    }

    /// Intersection over union of the two quads' axis-aligned bounds.
    static func iou(_ a: [Point], _ b: [Point]) -> Float {
        guard let ax0 = a.map(\.x).min(), let ax1 = a.map(\.x).max(),
              let ay0 = a.map(\.y).min(), let ay1 = a.map(\.y).max(),
              let bx0 = b.map(\.x).min(), let bx1 = b.map(\.x).max(),
              let by0 = b.map(\.y).min(), let by1 = b.map(\.y).max() else { return 0 }
        let inter = max(0, min(ax1, bx1) - max(ax0, bx0)) * max(0, min(ay1, by1) - max(ay0, by0))
        let union = (ax1 - ax0) * (ay1 - ay0) + (bx1 - bx0) * (by1 - by0) - inter
        return union <= 0 ? 0 : inter / union
    }
}

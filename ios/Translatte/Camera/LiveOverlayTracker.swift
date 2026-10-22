import Foundation

/// A live region whose identity survives updates; `from` is where its box was shown before this update.
struct TrackedRegion: Equatable {
    let id: Int
    let region: Region
    let from: [Point]
    let missedSinceMs: Int64?
}

/// Keeps the live overlay steady between updates, with the same rules as Android's `LiveOverlayTracker`.
/// Each new region takes the not-yet-taken previous region it overlaps most (IoU at least `minIoU`).
/// With the same source text, it keeps that region's identity and translation and slides from the old
/// box, even when its own translation is missing; with different text, a translated one replaces it
/// and an untranslated one is skipped. Previous regions nothing took stay, however long updates
/// take, until `maxUnseenMs` after the first update that missed them, unless a new region overlaps them. An update with no regions
/// clears everything. Previous regions first move by the camera's motion since the last update.
struct LiveOverlayTracker {
    static let minIoU: Float = 0.3
    static let maxUnseenMs: Int64 = 500

    private(set) var regions: [TrackedRegion] = []
    private var nextId = 0

    /// `shift` is how far the image moved from the last update's frame to this one's, if known.
    mutating func update(_ incoming: [Region], nowMs: Int64, shift: Point? = nil) -> [TrackedRegion] {
        guard !incoming.isEmpty else {
            regions = []
            return regions
        }
        if let shift {
            regions = regions.map { t in
                var r = t.region
                r.corners = Self.moved(r.corners, by: shift)
                return TrackedRegion(id: t.id, region: r, from: t.from, missedSinceMs: t.missedSinceMs)
            }
        }
        var taken = Array(repeating: false, count: regions.count)
        var out: [TrackedRegion] = []
        for r in incoming {
            var best = -1
            var bestIoU: Float = 0
            for (i, old) in regions.enumerated() {
                let v = Self.iou(old.region.corners, r.corners)
                if !taken[i], v >= Self.minIoU, v > bestIoU {
                    best = i
                    bestIoU = v
                }
            }
            if best >= 0, regions[best].region.sourceText == r.sourceText {
                taken[best] = true
                let old = regions[best]
                var kept = r
                kept.translatedText = old.region.translatedText
                out.append(TrackedRegion(id: old.id, region: kept, from: old.region.corners, missedSinceMs: nil))
            } else if r.translatedText != nil {
                if best >= 0 { taken[best] = true }
                out.append(TrackedRegion(id: nextId, region: r, from: r.corners, missedSinceMs: nil))
                nextId += 1
            }
        }
        for (i, old) in regions.enumerated()
        where !taken[i] && !incoming.contains(where: { Self.intersects(old.region.corners, $0.corners) }) {
            let missedSince = old.missedSinceMs ?? nowMs
            if nowMs - missedSince < Self.maxUnseenMs {
                out.append(TrackedRegion(id: old.id, region: old.region, from: old.region.corners, missedSinceMs: missedSince))
            }
        }
        regions = out
        return out
    }

    /// Drops missed regions `maxUnseenMs` after the update that missed them; never drops on a lack of updates.
    mutating func prune(nowMs: Int64) -> [TrackedRegion] {
        let kept = regions.filter { $0.missedSinceMs.map { nowMs - $0 < Self.maxUnseenMs } ?? true }
        if kept.count != regions.count { regions = kept }
        return regions
    }

    mutating func reset() {
        regions = []
    }

    static func moved(_ corners: [Point], by d: Point) -> [Point] {
        corners.map { Point(x: $0.x + d.x, y: $0.y + d.y) }
    }

    /// Whether the two quads' axis-aligned bounds overlap by a positive area.
    static func intersects(_ a: [Point], _ b: [Point]) -> Bool {
        guard let ax0 = a.map(\.x).min(), let ax1 = a.map(\.x).max(),
              let ay0 = a.map(\.y).min(), let ay1 = a.map(\.y).max(),
              let bx0 = b.map(\.x).min(), let bx1 = b.map(\.x).max(),
              let by0 = b.map(\.y).min(), let by1 = b.map(\.y).max() else { return false }
        return min(ax1, bx1) > max(ax0, bx0) && min(ay1, by1) > max(ay0, by0)
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

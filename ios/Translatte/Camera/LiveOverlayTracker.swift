import Foundation

/// A live region whose identity survives updates; `from` is where its box was shown before this
/// update, `fadeFrom` the translation it showed before this update when that changed, and `drift`
/// how far the camera moved since an update last saw it.
struct TrackedRegion: Equatable {
    let id: Int
    let region: Region
    let from: [Point]
    let missedSinceMs: Int64?
    var fadeFrom: String?
    var drift = LiveMotion.zero
}

/// Keeps the live overlay steady between updates, with the same rules as Android's `LiveOverlayTracker`.
/// Previous regions first move by the camera's motion since the last update. Then new and previous
/// regions pair up one to one, best overlap first (IoU at least `minIoU`): a new region keeps its
/// pair's identity and slides from its box. When the source text is `similar`, the old translation
/// stays unless the new one isn't similar to it (or is missing); otherwise the new translation
/// replaces it, and an untranslated new region can't pair. Unpaired new regions with a translation
/// are added. Unpaired previous regions stay until `maxUnseenMs` after the first update that
/// missed them, however long updates take, unless a new region overlaps them or the camera moved
/// more than `maxDriftLines` of their lines since they were seen. Regions come out in drawing
/// order: missed ones first, then from the smallest box to the largest.
struct LiveOverlayTracker {
    static let minIoU: Float = 0.3
    static let maxUnseenMs: Int64 = 500
    /// Texts at least this similar (`similarity`) are the same text read again.
    static let similarText: Float = 0.6
    /// Missed regions go once the camera moved this many of their lines: they'd be out of place.
    static let maxDriftLines: Float = 1.5

    private(set) var regions: [TrackedRegion] = []
    private var nextId = 0

    /// `shift` is how far the image moved from the last update's frame to this one's, if known.
    mutating func update(_ incoming: [Region], nowMs: Int64, shift: Point? = nil) -> [TrackedRegion] {
        let d = shift ?? LiveMotion.zero
        let old = regions.map { t -> TrackedRegion in
            var r = t.region
            r.corners = Self.moved(r.corners, by: d)
            return TrackedRegion(id: t.id, region: r, from: t.from, missedSinceMs: t.missedSinceMs, fadeFrom: t.fadeFrom, drift: t.drift)
        }
        var pairs: [(old: Int, new: Int, iou: Float)] = []
        for (i, o) in old.enumerated() {
            for (j, r) in incoming.enumerated() {
                let v = Self.iou(o.region.corners, r.corners)
                if v >= Self.minIoU, r.translatedText != nil || Self.similar(o.region.sourceText, r.sourceText) {
                    pairs.append((i, j, v))
                }
            }
        }
        pairs.sort { $0.iou > $1.iou }
        var taken = Array(repeating: false, count: old.count)
        var match: [Int?] = Array(repeating: nil, count: incoming.count)
        for p in pairs where !taken[p.old] && match[p.new] == nil {
            taken[p.old] = true
            match[p.new] = p.old
        }
        var out: [TrackedRegion] = []
        for (j, r) in incoming.enumerated() {
            if let i = match[j] {
                let o = old[i]
                var kept = r
                kept.translatedText = Self.translation(old: o.region, new: r)
                let fade = o.region.translatedText == kept.translatedText ? nil : o.region.translatedText
                out.append(TrackedRegion(id: o.id, region: kept, from: o.region.corners, missedSinceMs: nil, fadeFrom: fade))
            } else if r.translatedText != nil {
                out.append(TrackedRegion(id: nextId, region: r, from: r.corners, missedSinceMs: nil))
                nextId += 1
            }
        }
        for (i, o) in old.enumerated() where !taken[i] && !incoming.contains(where: { Self.intersects(o.region.corners, $0.corners) }) {
            let drift = Point(x: o.drift.x + d.x, y: o.drift.y + d.y)
            let missedSince = o.missedSinceMs ?? nowMs
            if nowMs - missedSince < Self.maxUnseenMs, hypot(drift.x, drift.y) <= Self.maxDriftLines * Self.lineHeight(o.region) {
                out.append(TrackedRegion(id: o.id, region: o.region, from: o.region.corners, missedSinceMs: missedSince, drift: drift))
            }
        }
        regions = out.enumerated().sorted { a, b in
            let (am, bm) = (a.element.missedSinceMs != nil, b.element.missedSinceMs != nil)
            if am != bm { return am }
            let (aa, ba) = (Self.area(a.element.region.corners), Self.area(b.element.region.corners))
            return aa != ba ? aa < ba : a.offset < b.offset
        }.map(\.element)
        return regions
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

    /// The translation a region that paired with `old` shows.
    static func translation(old: Region, new: Region) -> String? {
        guard let was = old.translatedText else { return new.translatedText }
        guard let now = new.translatedText else { return was }
        return similar(old.sourceText, new.sourceText) && similar(was, now) ? was : now
    }

    static func similar(_ a: String, _ b: String) -> Bool {
        similarity(a, b) >= similarText
    }

    /// 1 minus the edit distance over the longer length, ignoring case and whitespace.
    static func similarity(_ a: String, _ b: String) -> Float {
        let x = Array(a.lowercased().filter { !$0.isWhitespace })
        let y = Array(b.lowercased().filter { !$0.isWhitespace })
        if x.isEmpty && y.isEmpty { return 1 }
        var prev = Array(0...y.count)
        var cur = Array(repeating: 0, count: y.count + 1)
        for i in stride(from: 1, through: x.count, by: 1) {
            cur[0] = i
            for j in stride(from: 1, through: y.count, by: 1) {
                cur[j] = min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x[i - 1] == y[j - 1] ? 0 : 1))
            }
            swap(&prev, &cur)
        }
        return 1 - Float(prev[y.count]) / Float(max(x.count, y.count))
    }

    /// The region's line height, or its box's short side when unknown.
    static func lineHeight(_ r: Region) -> Float {
        if r.lineHeight > 0 { return r.lineHeight }
        let c = r.corners
        return min(hypot(c[1].x - c[0].x, c[1].y - c[0].y), hypot(c[3].x - c[0].x, c[3].y - c[0].y))
    }

    static func moved(_ corners: [Point], by d: Point) -> [Point] {
        corners.map { Point(x: $0.x + d.x, y: $0.y + d.y) }
    }

    /// The area of the quad's axis-aligned bounds.
    static func area(_ c: [Point]) -> Float {
        guard let x0 = c.map(\.x).min(), let x1 = c.map(\.x).max(),
              let y0 = c.map(\.y).min(), let y1 = c.map(\.y).max() else { return 0 }
        return (x1 - x0) * (y1 - y0)
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

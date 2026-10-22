import Foundation

/// A frame's `LiveRules.thumbnail` and the upright size, in frame pixels, of the frame it shrinks.
struct Thumb {
    let pixels: Data
    let width: Int
    let height: Int
}

/// Follows camera motion since the frame the overlay's boxes were read from (the reference), as a
/// global translation in that frame's pixels, the same as Android's `LiveMotion`. `frame` takes
/// each preview frame and `rebase` makes a published result's frame the reference. Thread-safe.
///
/// Each frame is measured against the reference and against the frame before it. Recent frames
/// keep their position along that frame-to-frame chain, which breaks where two frames can't be
/// matched; when a frame is too far from the reference or too blurred to measure directly, its
/// motion along the chain from the last frame that was (or from the reference itself) adds on. A
/// frame that can't be measured either way leaves `shift` as it was.
final class LiveMotion: @unchecked Sendable {
    static let zero = Point(x: 0, y: 0)
    /// About 3 s of preview frames: results are rarely older.
    static let recentFrames = 90

    /// Makes a reference of a thumbnail: how far a later thumbnail moved from it, in thumbnail
    /// pixels, or nil when unsure; nil for a thumbnail too flat to match.
    typealias Measure = (Data) -> ((Data) -> Point?)?

    /// A recent frame and its position along chain `segment`, in thumbnail pixels.
    private struct Entry {
        let thumb: Data
        let pos: Point
        let segment: Int
    }

    private let measure: Measure
    private let lock = NSLock()
    private var recent: [Entry] = []
    private var segment = 0
    private var reference: Thumb?
    private var estimate: ((Data) -> Point?)?
    /// The last frame whose motion from the reference is known, and that motion.
    private var known: Entry?
    private var knownShift = LiveMotion.zero
    private var current = LiveMotion.zero

    init(measure: @escaping Measure = LiveMotion.ffiMeasure) {
        self.measure = measure
    }

    /// The motion from the reference to the latest frame, in reference frame pixels.
    var shift: Point { lock.withLock { current } }

    /// A new preview frame's thumbnail; returns whether `shift` changed.
    func frame(_ thumb: Data) -> Bool {
        lock.withLock {
            let entry: Entry
            if let prev = recent.last, let step = measure(prev.thumb)?(thumb) {
                entry = Entry(thumb: thumb, pos: Point(x: prev.pos.x + step.x, y: prev.pos.y + step.y), segment: prev.segment)
            } else {
                segment += 1
                entry = Entry(thumb: thumb, pos: Self.zero, segment: segment)
            }
            recent.append(entry)
            if recent.count > Self.recentFrames { recent.removeFirst() }
            if let direct = estimate?(thumb).map(toFrame) {
                known = entry
                knownShift = direct
                current = direct
            } else if let chained = chained(entry) {
                current = chained
            } else {
                return false
            }
            return true
        }
    }

    /// Makes `source` the reference, with `shift` measured from it to the latest frame; returns the
    /// motion from the previous reference to `source`, or nil when unknown.
    func rebase(_ source: Thumb?) -> Point? {
        lock.withLock {
            let sourceEntry = source.flatMap { s in recent.last { $0.thumb == s.pixels } }
            let between = source.flatMap { estimate?($0.pixels) }.map(toFrame) ?? sourceEntry.flatMap(chained)
            let before = current
            reference = source
            estimate = source.flatMap { measure($0.pixels) }
            known = sourceEntry
            knownShift = Self.zero
            let latest = recent.last
            if let latest, let direct = estimate?(latest.thumb).map(toFrame) {
                known = latest
                knownShift = direct
                current = direct
            } else if let chained = latest.flatMap(chained) {
                current = chained
            } else if let between {
                current = Point(x: before.x - between.x, y: before.y - between.y)
            } else {
                current = Self.zero
            }
            return between
        }
    }

    func reset() {
        lock.withLock {
            recent = []
            reference = nil
            estimate = nil
            known = nil
            current = Self.zero
        }
    }

    /// The motion from the reference to `e` along the chain from the last known frame.
    private func chained(_ e: Entry) -> Point? {
        guard let k = known, k.segment == e.segment else { return nil }
        let d = toFrame(Point(x: e.pos.x - k.pos.x, y: e.pos.y - k.pos.y))
        return Point(x: knownShift.x + d.x, y: knownShift.y + d.y)
    }

    private func toFrame(_ p: Point) -> Point {
        guard let r = reference else { return Self.zero }
        let side = Float(LiveRules.thumbSide)
        return Point(x: p.x * Float(r.width) / side, y: p.y * Float(r.height) / side)
    }

    private static func ffiMeasure(_ thumb: Data) -> ((Data) -> Point?)? {
        let side = UInt32(LiveRules.thumbSide)
        guard let reference = motionReference(thumbnail: thumb, width: side, height: side) else { return nil }
        return { current in reference.shiftTo(thumbnail: current).map { Point(x: $0.dx, y: $0.dy) } }
    }
}

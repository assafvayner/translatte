import Foundation

/// One change to the live overlay: "add", "move", "retext" or "remove" (with a `reason`) of `tracked`.
struct LiveEvent: Equatable {
    let ev: String
    let tracked: TrackedRegion
    var reason: String?
}

/// Debug builds: the live overlay's updates and changes as JSON lines for scripts/live/analyze.py,
/// in the same format as Android's `LiveEventLog`. Times are ms since `start` (a replay's start) or,
/// with the camera, since the first event.
final class LiveEventLog {
    private let emit: (String) -> Void
    private var startMs: Int64?

    init(emit: @escaping (String) -> Void) {
        self.emit = emit
    }

    func start(nowMs: Int64) {
        startMs = nowMs
    }

    func update(nowMs: Int64, prev: [TrackedRegion], next: [TrackedRegion], result: TranslationResult, intervalMs: Int64?, ptsMs: Int64?) {
        let t = elapsed(nowMs)
        let seen = next.filter { $0.missedSinceMs == nil }.map(\.id)
        emit(Self.updateJson(tMs: t, result: result, seen: seen, shown: next.count, intervalMs: intervalMs, ptsMs: ptsMs))
        Self.diff(prev: prev, next: next, incoming: result.regions).forEach { emit(Self.json(tMs: t, $0)) }
    }

    func prune(nowMs: Int64, prev: [TrackedRegion], next: [TrackedRegion]) {
        let t = elapsed(nowMs)
        Self.diff(prev: prev, next: next, incoming: nil).forEach { emit(Self.json(tMs: t, $0)) }
    }

    func reset(nowMs: Int64, prev: [TrackedRegion]) {
        guard !prev.isEmpty else { return }
        let t = elapsed(nowMs)
        prev.forEach { emit(Self.json(tMs: t, LiveEvent(ev: "remove", tracked: $0, reason: "reset"))) }
    }

    private func elapsed(_ nowMs: Int64) -> Int64 {
        if startMs == nil { startMs = nowMs }
        return nowMs - startMs!
    }

    /// What changed from `prev` to `next`. A removed region is "empty" when the update had no
    /// regions, "overlap" when a new region covers it, and otherwise "expired" (`incoming` is nil
    /// for a prune).
    static func diff(prev: [TrackedRegion], next: [TrackedRegion], incoming: [Region]?) -> [LiveEvent] {
        let nextIds = Set(next.map(\.id))
        let prevById = Dictionary(prev.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        var out: [LiveEvent] = []
        for old in prev where !nextIds.contains(old.id) {
            let reason: String
            if let incoming {
                if incoming.isEmpty {
                    reason = "empty"
                } else if incoming.contains(where: { intersects(old.region.corners, $0.corners) }) {
                    reason = "overlap"
                } else {
                    reason = "expired"
                }
            } else {
                reason = "expired"
            }
            out.append(LiveEvent(ev: "remove", tracked: old, reason: reason))
        }
        for t in next {
            if let old = prevById[t.id] {
                if old.region.translatedText != t.region.translatedText {
                    out.append(LiveEvent(ev: "retext", tracked: t))
                } else if box(old.region.corners) != box(t.region.corners) {
                    out.append(LiveEvent(ev: "move", tracked: t))
                }
            } else {
                out.append(LiveEvent(ev: "add", tracked: t))
            }
        }
        return out
    }

    /// `seen` are the ids this update matched or added; `shown` counts the boxes on screen after it.
    /// `blocks` lists every region of the update, matched or not.
    static func updateJson(tMs: Int64, result: TranslationResult, seen: [Int], shown: Int, intervalMs: Int64?, ptsMs: Int64?) -> String {
        let t = result.timings
        var s = "{\"t_ms\":\(tMs),\"ev\":\"update\",\"regions\":\(result.regions.count),\"shown\":\(shown),\"seen\":[\(seen.map(String.init).joined(separator: ","))]"
        s += ",\"det_ms\":\(t.detectMs),\"rec_ms\":\(t.recognizeMs),\"tr_ms\":\(t.translateMs),\"engine\":\"\(t.ocrEngine)\",\"reused\":\(t.reused)"
        s += ",\"w\":\(result.imageWidth),\"h\":\(result.imageHeight)"
        if let intervalMs { s += ",\"interval_ms\":\(intervalMs)" }
        if let ptsMs { s += ",\"pts_ms\":\(ptsMs)" }
        let blocks = result.regions.map { r in
            "{\"src\":\(quoted(r.sourceText)),\"box\":[\(box(r.corners).map(String.init).joined(separator: ","))]}"
        }
        return s + ",\"blocks\":[\(blocks.joined(separator: ","))]}"
    }

    static func json(tMs: Int64, _ e: LiveEvent) -> String {
        let r = e.tracked.region
        var s = "{\"t_ms\":\(tMs),\"ev\":\"\(e.ev)\",\"id\":\(e.tracked.id),\"src\":\(quoted(r.sourceText))"
        s += ",\"tr\":\(r.translatedText.map(quoted) ?? "null")"
        s += ",\"box\":[\(box(r.corners).map(String.init).joined(separator: ","))]"
        if let reason = e.reason { s += ",\"reason\":\"\(reason)\"" }
        return s + "}"
    }

    /// The quad's axis-aligned bounds, [x0, y0, x1, y1], rounded to whole pixels.
    static func box(_ corners: [Point]) -> [Int] {
        let xs = corners.map(\.x), ys = corners.map(\.y)
        return [xs.min() ?? 0, ys.min() ?? 0, xs.max() ?? 0, ys.max() ?? 0].map { Int($0.rounded()) }
    }

    private static func intersects(_ a: [Point], _ b: [Point]) -> Bool {
        guard let ax0 = a.map(\.x).min(), let ax1 = a.map(\.x).max(),
              let ay0 = a.map(\.y).min(), let ay1 = a.map(\.y).max(),
              let bx0 = b.map(\.x).min(), let bx1 = b.map(\.x).max(),
              let by0 = b.map(\.y).min(), let by1 = b.map(\.y).max() else { return false }
        return min(ax1, bx1) > max(ax0, bx0) && min(ay1, by1) > max(ay0, by0)
    }

    private static func quoted(_ s: String) -> String {
        var out = "\""
        for c in s.unicodeScalars {
            switch c {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case _ where c.value < 0x20: out += String(format: "\\u%04x", c.value)
            default: out.unicodeScalars.append(c)
            }
        }
        return out + "\""
    }
}

#if DEBUG
/// Where debug builds send `liveev` lines: stderr, and Documents/liveev.jsonl (emptied at launch).
enum LiveEventSink {
    private static let queue = DispatchQueue(label: "translatte.liveev")
    private static let file: FileHandle? = {
        guard let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else { return nil }
        let url = dir.appendingPathComponent("liveev.jsonl")
        FileManager.default.createFile(atPath: url.path, contents: nil)
        return try? FileHandle(forWritingTo: url)
    }()

    static func write(_ json: String) {
        let line = Data((json + "\n").utf8)
        queue.async {
            FileHandle.standardError.write(Data("liveev ".utf8) + line)
            file?.write(line)
        }
    }
}
#endif

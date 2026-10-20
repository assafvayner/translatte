import CoreGraphics

enum RegionHitTest {
    /// The index of the topmost translated region whose box contains `point`, in image pixels.
    static func region(at point: CGPoint, in regions: [Region]) -> Int? {
        regions.indices.reversed().first { i in
            regions[i].translatedText != nil
                && contains(regions[i].corners.map { CGPoint(x: CGFloat($0.x), y: CGFloat($0.y)) }, point)
        }
    }

    /// Even-odd point-in-polygon test.
    static func contains(_ polygon: [CGPoint], _ p: CGPoint) -> Bool {
        var inside = false
        var j = polygon.count - 1
        for i in polygon.indices {
            let (a, b) = (polygon[i], polygon[j])
            if (a.y > p.y) != (b.y > p.y), p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside.toggle()
            }
            j = i
        }
        return inside
    }
}

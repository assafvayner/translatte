import SwiftUI

/// Paints live regions over the camera preview in the result viewer's style, placed with
/// `LiveRules.fill` from the upright frame size. Kept boxes slide to their new place.
struct LiveOverlay: View {
    let tracked: [TrackedRegion]
    let imageSize: CGSize
    let rtl: Bool

    var body: some View {
        GeometryReader { geo in
            let fill = LiveRules.fill(
                imageWidth: imageSize.width, imageHeight: imageSize.height,
                viewWidth: geo.size.width, viewHeight: geo.size.height
            )
            ZStack(alignment: .topLeading) {
                ForEach(tracked, id: \.id) { t in
                    RegionLabel(region: t.region, scale: fill.scale, rtl: rtl, showOriginal: false)
                        .offset(x: fill.left, y: fill.top)
                        .animation(.easeOut(duration: Double(LiveRules.moveMs) / 1000), value: t.region.corners)
                        .accessibilityIdentifier("live-region-\(t.id)")
                }
            }
        }
        .accessibilityElement(children: .contain)
    }
}

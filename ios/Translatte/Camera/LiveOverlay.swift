import SwiftUI

/// Paints live regions over the camera preview in the result viewer's style, placed with
/// `LiveRules.fill` from the upright frame size (the preview aspect-fills the same way). Kept boxes
/// slide to their new place. Reads the translator here so the rest of the camera screen doesn't
/// re-render with each update.
struct LiveOverlay: View {
    @ObservedObject var live: LiveTranslator
    let rtl: Bool

    var body: some View {
        GeometryReader { geo in
            let fill = LiveRules.fill(
                imageWidth: live.imageSize.width, imageHeight: live.imageSize.height,
                viewWidth: geo.size.width, viewHeight: geo.size.height
            )
            ZStack(alignment: .topLeading) {
                ForEach(live.overlay, id: \.id) { t in
                    LiveRegion(region: t.region, scale: fill.scale, rtl: rtl)
                        .equatable()
                        .offset(x: fill.left, y: fill.top)
                        .animation(.easeOut(duration: Double(LiveRules.moveMs) / 1000), value: t.region.corners)
                        .accessibilityIdentifier("live-region-\(t.id)")
                }
            }
        }
        .accessibilityElement(children: .contain)
    }
}

/// Skips re-rendering a region whose box, text and colors haven't changed since the last update.
private struct LiveRegion: View, Equatable {
    let region: Region
    let scale: CGFloat
    let rtl: Bool

    var body: some View {
        RegionLabel(region: region, scale: scale, rtl: rtl, showOriginal: false)
    }
}

/// The debug readout of the translator's latest update.
struct LiveReadout: View {
    @ObservedObject var live: LiveTranslator

    var body: some View {
        #if DEBUG
        if let readout = live.readout {
            Text(readout)
                .font(Typography.caption)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color(red: 0x16 / 255, green: 0x11 / 255, blue: 0x0E / 255).opacity(0.6), in: RoundedRectangle(cornerRadius: 8))
                .accessibilityIdentifier("live-readout")
        }
        #endif
    }
}

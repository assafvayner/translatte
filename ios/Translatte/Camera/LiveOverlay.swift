import SwiftUI

/// Paints live regions over the camera preview in the result viewer's style, placed with
/// `LiveRules.fill` from the upright frame size (the preview aspect-fills the same way) and moved
/// with the camera by the translator's shift. Kept boxes slide to their new place and swap a changed
/// translation's text over a steady background. Reads the translator here so the rest of the camera screen doesn't
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
                    ZStack(alignment: .topLeading) {
                        LiveBackground(region: t.region, scale: fill.scale)
                        LiveRegion(region: t.region, scale: fill.scale, rtl: rtl)
                            .equatable()
                            .id(t.region.translatedText)
                            .transition(Self.retext)
                    }
                    .animation(.easeOut(duration: Double(LiveRules.moveMs) / 1000), value: t.region.translatedText)
                    .offset(x: fill.left + live.shift.x * fill.scale, y: fill.top + live.shift.y * fill.scale)
                    .animation(.easeOut(duration: Double(LiveRules.moveMs) / 1000), value: t.region.corners)
                    .accessibilityIdentifier("live-region-\(t.id)")
                }
            }
        }
        .accessibilityElement(children: .contain)
    }

    /// A changed translation's old text fades out before the new fades in, so the two never show together.
    private static let retext = AnyTransition.asymmetric(
        insertion: .opacity.animation(.easeIn(duration: Double(LiveRules.moveMs) / 2000).delay(Double(LiveRules.moveMs) / 2000)),
        removal: .opacity.animation(.easeOut(duration: Double(LiveRules.moveMs) / 2000))
    )
}

/// The region's box, kept opaque while its text changes.
private struct LiveBackground: View {
    let region: Region
    let scale: CGFloat

    var body: some View {
        let box = RegionBox(region, scale: scale)
        Rectangle()
            .fill(Color(region.background))
            .frame(width: box.width, height: box.height)
            .rotationEffect(.radians(Double(region.angle)))
            .position(box.center)
            .accessibilityHidden(true)
    }
}

/// Skips re-rendering a region whose box, text and colors haven't changed since the last update.
private struct LiveRegion: View, Equatable {
    let region: Region
    let scale: CGFloat
    let rtl: Bool

    var body: some View {
        RegionLabel(region: region, scale: scale, rtl: rtl, showOriginal: false, minFontSize: LiveRules.minTextSize, fillsBackground: false)
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

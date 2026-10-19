import SwiftUI

/// Draws the photo with each translated region painted over its source text.
struct OverlayView: View {
    let image: UIImage
    let result: TranslationResult?
    let rtlTarget: Bool
    let onTap: (Region) -> Void

    var body: some View {
        GeometryReader { geo in
            let fit = AspectFit(imageSize: image.size, in: geo.size)
            ZStack(alignment: .topLeading) {
                Image(uiImage: image)
                    .resizable()
                    .frame(width: fit.size.width, height: fit.size.height)
                    .offset(x: fit.origin.x, y: fit.origin.y)
                if let result {
                    ForEach(result.regions.filter { $0.translatedText != nil }) { region in
                        RegionLabel(region: region, scale: fit.scale, rtl: rtlTarget)
                            .offset(x: fit.origin.x, y: fit.origin.y)
                            .onTapGesture { onTap(region) }
                    }
                }
            }
        }
        .accessibilityIdentifier("overlay")
    }
}

private struct AspectFit {
    let scale: CGFloat
    let size: CGSize
    let origin: CGPoint

    init(imageSize: CGSize, in container: CGSize) {
        let s = min(container.width / max(imageSize.width, 1), container.height / max(imageSize.height, 1))
        scale = s
        size = CGSize(width: imageSize.width * s, height: imageSize.height * s)
        origin = CGPoint(x: (container.width - size.width) / 2, y: (container.height - size.height) / 2)
    }
}

/// A rotated box in the region's colors with the translation shrunk to fit.
private struct RegionLabel: View {
    let region: Region
    let scale: CGFloat
    let rtl: Bool

    var body: some View {
        let c = region.corners.map { CGPoint(x: CGFloat($0.x) * scale, y: CGFloat($0.y) * scale) }
        let width = max(distance(c[0], c[1]), distance(c[3], c[2]))
        let height = max(distance(c[0], c[3]), distance(c[1], c[2]))
        let center = CGPoint(x: c.map(\.x).reduce(0, +) / 4, y: c.map(\.y).reduce(0, +) / 4)
        Text(region.translatedText ?? "")
            .font(.system(size: max(height * 0.8, 6), weight: .medium))
            .minimumScaleFactor(0.1)
            .lineLimit(nil)
            .multilineTextAlignment(rtl ? .trailing : .leading)
            .environment(\.layoutDirection, rtl ? .rightToLeft : .leftToRight)
            .foregroundStyle(Color(region.foreground))
            .padding(1)
            .frame(width: width, height: height, alignment: rtl ? .trailing : .leading)
            .background(Color(region.background))
            .rotationEffect(.radians(Double(region.angle)))
            .position(center)
            .accessibilityLabel(region.translatedText ?? "")
    }

    private func distance(_ a: CGPoint, _ b: CGPoint) -> CGFloat {
        hypot(a.x - b.x, a.y - b.y)
    }
}

private extension Color {
    init(_ rgb: Rgb) {
        self.init(red: Double(rgb.r) / 255, green: Double(rgb.g) / 255, blue: Double(rgb.b) / 255)
    }
}

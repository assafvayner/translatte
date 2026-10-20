import SwiftUI

/// The photo with each translated region painted over its source text. Regions in `peeked` show their
/// original text instead.
struct TranslatedImage: View {
    let image: UIImage
    let result: TranslationResult?
    let rtl: Bool
    var peeked: Set<Int> = []
    var showTranslation = true
    var onToggle: (Int) -> Void = { _ in }

    var body: some View {
        GeometryReader { geo in
            let fit = PhotoFit(imageSize: image.size, in: geo.size)
            ZStack(alignment: .topLeading) {
                Image(uiImage: image)
                    .resizable()
                    .frame(width: fit.size.width, height: fit.size.height)
                    .offset(x: fit.origin.x, y: fit.origin.y)
                    .accessibilityHidden(true)
                if showTranslation, let regions = result?.regions {
                    ForEach(Array(regions.enumerated()), id: \.offset) { index, region in
                        if region.translatedText != nil {
                            RegionLabel(region: region, scale: fit.scale, rtl: rtl, showOriginal: peeked.contains(index))
                                .offset(x: fit.origin.x, y: fit.origin.y)
                                .accessibilityIdentifier("region-\(index)")
                                .accessibilityAction { onToggle(index) }
                        }
                    }
                }
            }
        }
    }
}

struct PhotoFit {
    let scale: CGFloat
    let size: CGSize
    let origin: CGPoint

    init(imageSize: CGSize, in container: CGSize) {
        let s = min(container.width / max(imageSize.width, 1), container.height / max(imageSize.height, 1))
        scale = s
        size = CGSize(width: imageSize.width * s, height: imageSize.height * s)
        origin = CGPoint(x: (container.width - size.width) / 2, y: (container.height - size.height) / 2)
    }

    /// Maps a point in the unzoomed layout to image pixels.
    func pixel(_ point: CGPoint) -> CGPoint {
        CGPoint(x: (point.x - origin.x) / scale, y: (point.y - origin.y) / scale)
    }
}

/// A rotated box in the region's colors with its translation (or original, when peeked) shrunk to fit.
private struct RegionLabel: View {
    let region: Region
    let scale: CGFloat
    let rtl: Bool
    let showOriginal: Bool

    var body: some View {
        let c = region.corners.map { CGPoint(x: CGFloat($0.x) * scale, y: CGFloat($0.y) * scale) }
        let width = max(distance(c[0], c[1]), distance(c[3], c[2]))
        let height = max(distance(c[0], c[3]), distance(c[1], c[2]))
        let center = CGPoint(x: c.map(\.x).reduce(0, +) / 4, y: c.map(\.y).reduce(0, +) / 4)
        let rightToLeft = rtl && !showOriginal
        Text(showOriginal ? region.sourceText : region.translatedText ?? "")
            .font(.system(size: max(height * 0.8, 6), weight: .medium))
            .minimumScaleFactor(0.1)
            .lineLimit(nil)
            .multilineTextAlignment(rightToLeft ? .trailing : .leading)
            .environment(\.layoutDirection, rightToLeft ? .rightToLeft : .leftToRight)
            .foregroundStyle(Color(region.foreground))
            .padding(1)
            .frame(width: width, height: height, alignment: rightToLeft ? .trailing : .leading)
            .background(Color(region.background))
            .rotationEffect(.radians(Double(region.angle)))
            .position(center)
            .accessibilityLabel(region.translatedText ?? "")
            .accessibilityValue(region.sourceText)
            .accessibilityHint(showOriginal ? "Shows the translation" : "Shows the original text")
            .accessibilityAddTraits(showOriginal ? [.isButton, .isSelected] : .isButton)
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

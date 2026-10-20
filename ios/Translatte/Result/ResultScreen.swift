import SwiftUI

struct ResultScreen: View {
    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var navigator: AppNavigator
    @AppStorage("resultHintSeen") private var hintSeen = false
    @State private var showTranslation = true
    @State private var peeked: Set<Int> = []
    @State private var zoom = ZoomTransform()
    @State private var pinchBase: CGFloat?
    @State private var panBase: CGSize?

    var body: some View {
        ZStack {
            Palette.background.ignoresSafeArea()
            if let image = model.image {
                photo(image).ignoresSafeArea()
            }
            status.padding(.horizontal, 32)
            VStack {
                topBar
                Spacer()
                bottomBar
            }
            .padding(.horizontal, 16)
        }
        .foregroundStyle(Palette.text)
        .buttonStyle(.plain)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("result-screen")
        .onChange(of: model.image) {
            zoom = ZoomTransform()
            peeked = []
            showTranslation = true
        }
    }

    private func photo(_ image: UIImage) -> some View {
        GeometryReader { geo in
            let container = geo.size
            let fit = PhotoFit(imageSize: image.size, in: container)
            TranslatedImage(
                image: image,
                result: model.result,
                rtl: model.targetIsRTL,
                peeked: peeked,
                showTranslation: showTranslation,
                onToggle: toggle
            )
            .scaleEffect(zoom.scale)
            .offset(zoom.offset)
            .frame(width: container.width, height: container.height)
            .clipped()
            .contentShape(Rectangle())
            .gesture(taps(fit: fit, container: container))
            .simultaneousGesture(pinch(fit: fit, container: container))
            .simultaneousGesture(pan(fit: fit, container: container))
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("result-image")
        }
    }

    private func taps(fit: PhotoFit, container: CGSize) -> some Gesture {
        SpatialTapGesture(count: 2)
            .onEnded { tap in
                gestured()
                withAnimation(.spring(duration: 0.3)) {
                    zoom.toggleDoubleTap(at: tap.location, content: fit.size, container: container)
                }
            }
            .exclusively(before: SpatialTapGesture().onEnded { tap in
                guard showTranslation, let regions = model.result?.regions else { return }
                let pixel = fit.pixel(zoom.unzoomed(tap.location, container: container))
                if let index = RegionHitTest.region(at: pixel, in: regions) { toggle(index) }
            })
    }

    private func pinch(fit: PhotoFit, container: CGSize) -> some Gesture {
        MagnifyGesture()
            .onChanged { value in
                gestured()
                let base = pinchBase ?? zoom.scale
                pinchBase = base
                zoom.scale = base * value.magnification
                zoom.clamp(content: fit.size, container: container)
            }
            .onEnded { _ in pinchBase = nil }
    }

    private func pan(fit: PhotoFit, container: CGSize) -> some Gesture {
        DragGesture(minimumDistance: 8)
            .onChanged { value in
                guard zoom.isZoomed else { return }
                gestured()
                let base = panBase ?? zoom.offset
                panBase = base
                zoom.offset = CGSize(width: base.width + value.translation.width, height: base.height + value.translation.height)
                zoom.clamp(content: fit.size, container: container)
            }
            .onEnded { _ in panBase = nil }
    }

    private func toggle(_ index: Int) {
        gestured()
        if peeked.remove(index) == nil { peeked.insert(index) }
    }

    private func gestured() {
        if !hintSeen { hintSeen = true }
    }

    private var topBar: some View {
        HStack(spacing: 10) {
            Button(action: navigator.closeResult) {
                Image(systemName: "chevron.left")
                    .font(.body.weight(.semibold))
                    .frame(width: 44, height: 44)
                    .background(Palette.glass, in: Circle())
            }
            .accessibilityLabel("Back")
            .accessibilityIdentifier("result-back")
            Text(model.directionLabel)
                .font(.subheadline.weight(.semibold))
                .padding(.horizontal, 16)
                .frame(height: 44)
                .background(Palette.glass, in: Capsule())
                .accessibilityIdentifier("result-direction")
            Spacer()
            if zoom.isZoomed {
                Text("\(zoom.percent)%")
                    .font(.footnote.monospacedDigit())
                    .padding(.horizontal, 12)
                    .frame(height: 32)
                    .background(Palette.glass, in: Capsule())
                    .accessibilityIdentifier("result-zoom")
            }
        }
        .padding(.top, 8)
    }

    @ViewBuilder private var bottomBar: some View {
        if let result = model.result, !result.regions.isEmpty {
            VStack(spacing: 10) {
                Picker("Overlay", selection: $showTranslation) {
                    Text("Translated").tag(true)
                    Text("Original").tag(false)
                }
                .pickerStyle(.segmented)
                .frame(width: 240)
                .padding(4)
                .background(Palette.glass, in: Capsule())
                .accessibilityIdentifier("result-mode")
                if !hintSeen {
                    Text("Pinch to zoom · drag to pan · double-tap to fit")
                        .font(.caption)
                        .foregroundStyle(Color(red: 0xF1 / 255, green: 0xE6 / 255, blue: 0xD8 / 255))
                        .shadow(color: .black.opacity(0.6), radius: 3, y: 1)
                        .accessibilityIdentifier("result-hint")
                }
            }
            .padding(.bottom, 16)
        }
    }

    @ViewBuilder private var status: some View {
        switch model.phase {
        case let .working(label):
            VStack(spacing: 14) {
                ProgressView().controlSize(.large).tint(Palette.primary)
                Text(label).font(.headline)
            }
            .glassCard()
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("result-progress")
        case let .failed(message):
            VStack(spacing: 14) {
                Image(systemName: noText ? "text.magnifyingglass" : "exclamationmark.triangle.fill")
                    .font(.title)
                    .foregroundStyle(noText ? Palette.primary : Palette.error)
                Text(message)
                    .font(.callout)
                    .multilineTextAlignment(.center)
                HStack(spacing: 12) {
                    Button("Back", action: navigator.closeResult)
                        .buttonStyle(.bordered)
                    if !noText && model.canRetry {
                        Button("Retry") { Task { await model.retry() } }
                            .buttonStyle(.borderedProminent)
                            .accessibilityIdentifier("result-retry")
                    }
                }
            }
            .glassCard()
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("result-error")
        case .idle, .done:
            EmptyView()
        }
    }

    private var noText: Bool {
        guard let result = model.result else { return false }
        return result.translationError == nil && result.regions.isEmpty
    }
}

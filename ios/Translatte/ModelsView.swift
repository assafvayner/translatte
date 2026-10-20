import SwiftUI

/// First-run setup (`setup`: core models plus chosen packs, one Download button) and the
/// Languages screen (per-pack download and delete).
struct ModelsView: View {
    let setup: Bool
    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var downloads: Downloads
    @State private var selected: Set<String> = ["rec-latin"]
    @State private var meteredPrompt: [String]?

    var body: some View {
        let free = model.freeBytes
        ScrollView {
            VStack(spacing: 16) {
                if setup { hero }
                Card(title: "Always included") {
                    ForEach(model.models.filter(\.core), id: \.id) { row($0, free: free) }
                }
                Card(title: "Scripts to read") {
                    ForEach(model.models.filter { !$0.core }, id: \.id) { row($0, free: free) }
                }
            }
            .padding(16)
        }
        .background(Palette.background.ignoresSafeArea())
        .tint(Palette.primary)
        .navigationTitle(setup ? "" : "Languages")
        .safeAreaInset(edge: .bottom) { bottomBar(free: free) }
        .alert(
            "Download on cellular data?",
            isPresented: Binding(get: { meteredPrompt != nil }, set: { if !$0 { meteredPrompt = nil } }),
            presenting: meteredPrompt
        ) { ids in
            Button("Download now") { downloads.start(ids, allowMetered: true) }
            Button("Wait for Wi-Fi") { downloads.start(ids, allowMetered: false) }
            Button("Cancel", role: .cancel) {}
        } message: { ids in
            Text("This downloads \(formatSize(model.bytesToDownload(ids))) over a metered connection.")
        }
    }

    private var hero: some View {
        VStack(spacing: 8) {
            LogoCup(size: 92)
            Text("Welcome to translatte")
                .font(.display(26))
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)
            Text("Download the languages you read. After that, everything runs on your phone, with no internet needed.")
                .font(.callout)
                .foregroundStyle(Palette.textMuted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .padding(.top, 12)
    }

    private var wanted: [ModelInfo] {
        guard setup else { return [] }
        return model.models.filter { $0.state != .installed && ($0.core || selected.contains($0.id)) }
    }

    private func startDownload(_ ids: [String]) {
        if Downloads.needsMeteredPrompt(bytes: model.bytesToDownload(ids), metered: downloads.isMetered) {
            meteredPrompt = ids
        } else {
            downloads.start(ids, allowMetered: true)
        }
    }

    @ViewBuilder
    private func bottomBar(free: UInt64) -> some View {
        let error = model.setupError ?? downloads.error
        let needed = wanted.reduce(0) { $0 + $1.remainingBytes }
        let hasPack = model.models.contains { !$0.core && ($0.state == .installed || selected.contains($0.id)) }
        let waiting = downloads.waitingForUnmetered
        let queued = downloads.queued && !downloads.running && downloads.error == nil
        if setup || downloads.busy || error != nil {
            VStack(spacing: 8) {
                if let error {
                    NoticeBanner(message: error)
                }
                if waiting {
                    Text("Waiting for Wi-Fi to download.").foregroundStyle(Palette.textMuted)
                } else if queued {
                    Text("Download queued.").foregroundStyle(Palette.textMuted)
                }
                if downloads.busy {
                    Button("Cancel download", role: .cancel, action: downloads.cancel)
                        .buttonStyle(BigButtonStyle(kind: .tonal))
                } else if setup {
                    if needed > free {
                        Text("Needs \(formatSize(needed)), only \(formatSize(free)) free").foregroundStyle(Palette.error)
                    }
                    Button(needed > 0 ? "Download \(formatSize(needed))" : "Download") {
                        startDownload(wanted.map(\.id))
                    }
                    .buttonStyle(BigButtonStyle(kind: .primary))
                    .disabled(wanted.isEmpty || !hasPack || needed > free)
                    if free != .max {
                        Text("Wi-Fi recommended · \(formatSize(free)) free on this phone")
                            .font(.caption)
                            .foregroundStyle(Palette.textMuted)
                    }
                }
            }
            .font(.callout)
            .multilineTextAlignment(.center)
            .padding(16)
            .background(Palette.background.ignoresSafeArea(edges: .bottom))
        }
    }

    private func row(_ m: ModelInfo, free: UInt64) -> some View {
        let installed = m.state == .installed
        let toggleable = setup && !m.core && !installed
        let checked = m.core || installed || (setup && selected.contains(m.id))
        let progress = downloads.running && !installed ? downloads.progress[m.id] : nil
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                let label = HStack(spacing: 12) {
                    Image(systemName: checked ? "checkmark.circle.fill" : "circle")
                        .imageScale(.large)
                        .foregroundStyle(checked || toggleable ? Palette.primary : Palette.textMuted)
                    ModelLabel(model: m)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if toggleable {
                    Button {
                        if checked { selected.remove(m.id) } else { selected.insert(m.id) }
                    } label: {
                        label.contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(checked ? .isSelected : [])
                    .accessibilityIdentifier("model-\(m.id)")
                } else {
                    label
                        .accessibilityElement(children: .combine)
                        .accessibilityAddTraits(checked ? .isSelected : [])
                        .accessibilityIdentifier("model-\(m.id)")
                }
                if !setup && !m.core && !downloads.busy {
                    VStack(alignment: .trailing, spacing: 6) {
                        if !installed {
                            Button(m.state == .incomplete ? "Resume" : "Download") { startDownload([m.id]) }
                                .buttonStyle(.borderedProminent)
                                .disabled(m.remainingBytes > free)
                                .accessibilityIdentifier("download-\(m.id)")
                        }
                        if m.state != .notInstalled || m.downloadedBytes > 0 {
                            Button("Delete", role: .destructive) { model.deleteModel(m.id) }
                                .buttonStyle(.bordered)
                                .accessibilityIdentifier("delete-\(m.id)")
                        }
                    }
                    .controlSize(.small)
                }
            }
            .frame(minHeight: 44)
            if let progress {
                ProgressView(value: Double(progress.done), total: Double(max(progress.total, 1)))
                    .tint(Palette.primary)
                Text(progress.done == 0 ? "Waiting for data…" : "\(formatSize(progress.done)) of \(formatSize(progress.total))")
                    .font(.caption)
                    .foregroundStyle(Palette.textMuted)
            }
        }
    }
}

private struct ModelLabel: View {
    let model: ModelInfo

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(model.displayName)
                .font(.body.weight(.medium))
                .foregroundStyle(Palette.text)
            if !model.languages.isEmpty {
                Text(model.languages.joined(separator: ", "))
                    .font(.caption)
                    .foregroundStyle(Palette.textMuted)
                    .lineLimit(2)
            }
            Text(model.status)
                .font(.caption)
                .foregroundStyle(Palette.textMuted)
        }
    }
}

extension ModelInfo {
    var remainingBytes: UInt64 { sizeBytes - min(downloadedBytes, sizeBytes) }

    var status: String {
        let size = formatSize(sizeBytes)
        if core { return "Required · \(size)" }
        switch state {
        case .installed: return "Installed · \(size)"
        case .incomplete: return "Incomplete · \(size)"
        case .notInstalled: return size
        }
    }
}

func formatSize(_ bytes: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
}

import SwiftUI

/// The ids among `ids` that aren't installed yet, each once.
func missingModels(_ ids: [String], in models: [ModelInfo]) -> [String] {
    var seen = Set<String>()
    return ids.filter { id in seen.insert(id).inserted && models.first { $0.id == id }?.state != .installed }
}

/// First-run setup (`setup`: the core models plus chosen languages, one Download button) and the
/// Languages screen (per-language download and delete). A language pack is its OCR script pack plus
/// its translation model; languages that share a script pack download it once.
struct ModelsView: View {
    let setup: Bool
    @EnvironmentObject private var model: TranslatorModel
    @EnvironmentObject private var downloads: Downloads
    @State private var selected: Set<String> = []
    @EnvironmentObject private var dialogs: DialogCenter

    var body: some View {
        let free = model.freeBytes
        ScrollView {
            VStack(spacing: Metrics.spacing) {
                if setup { hero }
                Card(title: "Always included") {
                    ForEach(model.models.filter(\.core), id: \.id) { coreRow($0) }
                }
                Card(title: "Languages to read") {
                    ForEach(model.packs, id: \.code) { packRow($0, free: free) }
                }
            }
            .padding(Metrics.gutter)
        }
        .background(Palette.background.ignoresSafeArea())
        .tint(Palette.primary)
        .navigationTitle(setup ? "" : "Languages")
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom) { bottomBar(free: free) }
    }

    private var hero: some View {
        VStack(spacing: 12) {
            LogoCup(size: 96)
            Text("Welcome to translatte")
                .font(Typography.display)
                .foregroundStyle(Palette.text)
                .accessibilityAddTraits(.isHeader)
            Text("Download the languages you read. After that, everything runs on your phone, with no internet needed.")
                .font(Typography.body)
                .lineSpacing(Typography.bodyLineSpacing)
                .foregroundStyle(Palette.textMuted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .padding(.top, 8)
    }

    /// Models setup still has to fetch: the core ones plus the chosen languages' packs.
    private var wanted: [String] {
        guard setup else { return [] }
        let ids = model.models.filter(\.core).map(\.id)
            + model.packs.filter { selected.contains($0.code) }.flatMap(\.modelIds)
        return missingModels(ids, in: model.models)
    }

    private func startDownload(_ ids: [String]) {
        if Downloads.needsMeteredPrompt(bytes: model.bytesToDownload(ids), metered: downloads.isMetered) {
            dialogs.show(AppDialog(
                title: "Download on cellular data?",
                message: "This downloads \(formatSize(model.bytesToDownload(ids))) over a metered connection.",
                actions: [
                    .init(title: "Cancel"),
                    .init(title: "Wait for Wi-Fi") { downloads.start(ids, allowMetered: false) },
                    .init(title: "Download now") { downloads.start(ids, allowMetered: true) },
                ],
                identifier: "metered-dialog"
            ))
        } else {
            downloads.start(ids, allowMetered: true)
        }
    }

    @ViewBuilder
    private func bottomBar(free: UInt64) -> some View {
        let error = model.setupError ?? downloads.error
        let needed = model.bytesToDownload(wanted)
        let hasPack = model.packs.contains { $0.state == .installed || selected.contains($0.code) }
        let waiting = downloads.waitingForUnmetered
        let queued = downloads.queued && !downloads.running && downloads.error == nil
        if setup || waiting || queued || error != nil {
            VStack(spacing: 8) {
                if let error {
                    NoticeBanner(message: error)
                }
                if waiting {
                    Text("Waiting for Wi-Fi to download.").foregroundStyle(Palette.textMuted)
                } else if queued {
                    Text("Download queued.").foregroundStyle(Palette.textMuted)
                }
                if downloads.busy && setup {
                    Button("Cancel download", role: .cancel, action: downloads.cancel)
                        .buttonStyle(BigButtonStyle(kind: .outlined))
                } else if setup {
                    if needed > free {
                        Text("Needs \(formatSize(needed)), only \(formatSize(free)) free").foregroundStyle(Palette.error)
                    }
                    Button(needed > 0 ? "Download \(formatSize(needed))" : "Download") {
                        startDownload(wanted)
                    }
                    .buttonStyle(BigButtonStyle(kind: .primary))
                    .disabled(wanted.isEmpty || !hasPack || needed > free)
                    if free != .max {
                        Text("Wi-Fi recommended · \(formatSize(free)) free on this phone")
                            .font(Typography.caption)
                            .foregroundStyle(Palette.textMuted)
                    }
                }
            }
            .font(Typography.body)
            .multilineTextAlignment(.center)
            .padding(Metrics.gutter)
            .background(Palette.background.ignoresSafeArea(edges: .bottom))
        }
    }

    /// Combined byte progress of the models among `ids` in a running download.
    private func progress(_ ids: [String], installed: Bool) -> ByteProgress? {
        guard downloads.running, !installed else { return nil }
        let parts = ids.compactMap { downloads.progress[$0] }
        guard !parts.isEmpty else { return nil }
        return ByteProgress(done: parts.reduce(0) { $0 + $1.done }, total: parts.reduce(0) { $0 + $1.total })
    }

    private func coreRow(_ m: ModelInfo) -> some View {
        row(
            id: "model-\(m.id)", title: m.displayName, status: "Required · \(formatSize(m.sizeBytes))",
            checked: true, toggle: nil, progress: progress([m.id], installed: m.state == .installed)
        ) { EmptyView() }
    }

    private func packRow(_ p: LanguagePack, free: UInt64) -> some View {
        let installed = p.state == .installed
        let checked = installed || (setup && selected.contains(p.code))
        let toggle: (() -> Void)? = setup && !installed
            ? { if checked { selected.remove(p.code) } else { selected.insert(p.code) } }
            : nil
        return row(
            id: "pack-\(p.code)", title: p.name, status: p.status, checked: checked, toggle: toggle,
            progress: progress(p.modelIds, installed: installed)
        ) {
            if !setup {
                VStack(alignment: .trailing, spacing: 6) {
                    if downloads.busy {
                        if !installed && !downloads.pendingIDs.isDisjoint(with: p.modelIds) {
                            Button("Cancel", action: downloads.cancel)
                                .buttonStyle(SmallButtonStyle(kind: .outlined))
                                .accessibilityIdentifier("cancel-\(p.code)")
                        }
                    } else if !installed {
                        Button(p.state == .incomplete ? "Resume" : "Download") {
                            startDownload(missingModels(p.modelIds, in: model.models))
                        }
                        .buttonStyle(SmallButtonStyle(kind: .primary))
                        .disabled(p.remainingBytes > free)
                        .accessibilityIdentifier("download-\(p.code)")
                    }
                    if !downloads.busy && p.deletable {
                        Button("Delete", role: .destructive) { model.deleteLanguage(p.code) }
                            .buttonStyle(SmallButtonStyle(kind: .outlined))
                            .accessibilityIdentifier("delete-\(p.code)")
                    }
                }
            }
        }
    }

    private func row<Actions: View>(
        id: String, title: String, status: String, checked: Bool, toggle: (() -> Void)?,
        progress: ByteProgress?, @ViewBuilder actions: () -> Actions
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 12) {
                let label = HStack(spacing: 12) {
                    Image(systemName: checked ? "checkmark.circle.fill" : "circle")
                        .iconFont(22)
                        .frame(width: 24, height: 24)
                        .foregroundStyle(checked || toggle != nil ? Palette.primary : Palette.textMuted)
                    RowLabel(title: title, status: status)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if let toggle {
                    Button(action: toggle) {
                        label.contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(checked ? .isSelected : [])
                    .accessibilityIdentifier(id)
                } else {
                    label
                        .accessibilityElement(children: .combine)
                        .accessibilityAddTraits(checked ? .isSelected : [])
                        .accessibilityIdentifier(id)
                }
                actions()
            }
            if let progress {
                ProgressView(value: Double(progress.done), total: Double(max(progress.total, 1)))
                    .tint(Palette.primary)
                    .padding(.top, 4)
                Text(progress.done == 0 ? "Waiting for data…" : "\(formatSize(progress.done)) of \(formatSize(progress.total))")
                    .font(Typography.caption)
                    .foregroundStyle(Palette.textMuted)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(minHeight: Metrics.rowMinHeight)
    }
}

private struct RowLabel: View {
    let title: String
    let status: String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(Typography.headline)
                .lineSpacing(Typography.headlineLineSpacing)
                .foregroundStyle(Palette.text)
            Text(status)
                .font(Typography.caption)
                .foregroundStyle(Palette.textMuted)
        }
    }
}

extension ModelInfo {
    var remainingBytes: UInt64 { sizeBytes - min(downloadedBytes, sizeBytes) }
}

extension LanguagePack {
    var remainingBytes: UInt64 { sizeBytes - min(downloadedBytes, sizeBytes) }

    var status: String {
        let size = formatSize(sizeBytes)
        switch state {
        case .installed: return "Installed · \(size)"
        case .incomplete: return "Incomplete · \(size)"
        case .notInstalled: return size
        }
    }
}

/// Decimal units, as on Android: MB below 1000 MB, else GB; under 10 one decimal for MB and two for
/// GB, under 100 one decimal, from 100 none ("4.8 MB", "2.49 GB", "16.6 MB", "512 MB", "207 GB").
func formatSize(_ bytes: UInt64) -> String {
    func digits(_ value: Double, gb: Bool) -> Int {
        value < 10 ? (gb ? 2 : 1) : value < 100 ? 1 : 0
    }
    func rounded(_ value: Double, _ digits: Int) -> Double {
        let p = pow(10, Double(digits))
        return (value * p).rounded() / p
    }
    let mb = Double(bytes) / 1_000_000
    if rounded(mb, digits(mb, gb: false)) < 1000 {
        return "\(mb.formatted(.number.precision(.fractionLength(digits(mb, gb: false))).grouping(.never))) MB"
    }
    let gb = Double(bytes) / 1_000_000_000
    return "\(gb.formatted(.number.precision(.fractionLength(digits(gb, gb: true))).grouping(.never))) GB"
}

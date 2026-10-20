import Foundation
import Network
import UIKit

/// What `Downloads` needs from the engine; a protocol so tests can stand in for the network.
protocol ModelDownloading: AnyObject, Sendable {
    func models() throws -> [ModelInfo]
    func download(modelIds: [String], listener: DownloadListener, token: DownloadToken) throws
}

extension Engine: ModelDownloading {}

struct ByteProgress: Equatable {
    var done: UInt64
    var total: UInt64
}

struct DownloadRequest: Codable, Equatable {
    var ids: [String]
    /// Without this the download waits for a network that isn't expensive or constrained.
    var allowMetered: Bool
}

/// Model downloads, run by the Rust downloader on a background thread. Requests are saved until
/// they finish, so an interrupted download resumes from its `.part` files on the next launch. A
/// download keeps going while the app is open (the screen doesn't lock) and asks iOS for extra
/// time when the app is backgrounded.
@MainActor
final class Downloads: ObservableObject {
    /// Downloads at least this big ask before using a metered connection.
    static let largeDownload: UInt64 = 100_000_000
    static let maxAttempts = 5
    private static let pendingKey = "pendingDownloads"

    /// Download calls in flight; a cancelled one can still be unwinding when the next starts.
    @Published private(set) var active = 0
    /// Model id to bytes done and total for models in the current or last run.
    @Published private(set) var progress: [String: ByteProgress] = [:]
    @Published private(set) var error: String?
    /// Requests not finished yet, the running one first.
    @Published private(set) var pending: [DownloadRequest] {
        didSet { defaults.set(try? JSONEncoder().encode(pending), forKey: Self.pendingKey) }
    }
    /// False until the path monitor's first update, so a Wi-Fi-only request saved before a relaunch
    /// can't start on cellular.
    @Published private(set) var networkAvailable: Bool
    /// Cellular, a personal hotspot or Low Data Mode.
    @Published private(set) var isMetered = false
    /// Bumped when a model finishes or a run ends, so screens re-read install state.
    @Published private(set) var finished = 0

    var engine: ModelDownloading? {
        didSet { pump() }
    }

    var running: Bool { active > 0 }
    /// A request is pending, also while it waits for a network or backs off before a retry.
    var queued: Bool { !pending.isEmpty }
    /// A download is running or pending: it can be cancelled, and starting or deleting would race it.
    var busy: Bool { running || queued }
    var waitingForUnmetered: Bool {
        !running && pending.first.map { !$0.allowMetered && isMetered } ?? false
    }

    private enum StopReason {
        case user, network, expired
    }

    @MainActor
    private final class Run {
        let request: DownloadRequest
        let token = DownloadToken()
        var stopReason: StopReason?
        var backgroundTask = UIBackgroundTaskIdentifier.invalid

        init(request: DownloadRequest) {
            self.request = request
        }
    }

    private let defaults: UserDefaults
    private let backoff: (Int) -> Duration
    private let queue = DispatchQueue(label: "translatte.downloads", qos: .utility)
    private var current: Run?
    private var attempts = 0
    private var retry: Task<Void, Never>?
    private var monitor: NWPathMonitor?

    init(
        defaults: UserDefaults = .standard,
        monitorNetwork: Bool = true,
        backoff: @escaping (Int) -> Duration = { .seconds(10 << min($0 - 1, 6)) }
    ) {
        self.defaults = defaults
        self.backoff = backoff
        networkAvailable = !monitorNetwork
        pending = defaults.data(forKey: Self.pendingKey)
            .flatMap { try? JSONDecoder().decode([DownloadRequest].self, from: $0) } ?? []
        guard monitorNetwork else { return }
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] path in
            let available = path.status == .satisfied
            let metered = path.isExpensive || path.isConstrained
            Task { @MainActor in self?.networkChanged(available: available, metered: metered) }
        }
        monitor.start(queue: DispatchQueue(label: "translatte.network"))
        self.monitor = monitor
    }

    static func needsMeteredPrompt(bytes: UInt64, metered: Bool) -> Bool {
        metered && bytes >= largeDownload
    }

    /// Queues `ids` behind any pending download.
    func start(_ ids: [String], allowMetered: Bool) {
        guard !ids.isEmpty else { return }
        error = nil
        pending.append(DownloadRequest(ids: ids, allowMetered: allowMetered))
        pump()
    }

    func cancel() {
        pending.removeAll()
        attempts = 0
        retry?.cancel()
        retry = nil
        error = nil
        stop(.user)
    }

    /// Starts the next pending request if nothing is running and the network allows it. Called
    /// again when the app returns to the foreground, after an expired background task stopped it.
    func pump() {
        guard current == nil, retry == nil, let engine, let request = pending.first else { return }
        guard networkAvailable, request.allowMetered || !isMetered else { return }
        guard UIApplication.shared.applicationState != .background else { return }
        let run = Run(request: request)
        current = run
        active += 1
        error = nil
        let sizes = Dictionary(uniqueKeysWithValues: ((try? engine.models()) ?? []).map { ($0.id, $0.sizeBytes) })
        for id in request.ids {
            progress[id] = ByteProgress(done: 0, total: sizes[id] ?? 0)
        }
        run.backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Download models") { [weak self] in
            MainActor.assumeIsolated {
                self?.stop(.expired)
                self?.endBackgroundTask(run)
            }
        }
        UIApplication.shared.isIdleTimerDisabled = true
        let listener = ProgressRelay { [weak self] id, done, total in
            Task { @MainActor in self?.record(id, done: done, total: total) }
        }
        queue.async { [weak self, token = run.token] in
            let result = Result { try engine.download(modelIds: request.ids, listener: listener, token: token) }
            Task { @MainActor in self?.finish(run, result) }
        }
    }

    func networkChanged(available: Bool, metered: Bool) {
        networkAvailable = available
        isMetered = metered
        if let current, metered, !current.request.allowMetered {
            stop(.network)
        }
        pump()
    }

    private func stop(_ reason: StopReason) {
        guard let current, current.stopReason == nil else { return }
        current.stopReason = reason
        current.token.cancel()
    }

    private func record(_ id: String, done: UInt64, total: UInt64) {
        let complete = total > 0 && done == total && progress[id]?.done != done
        progress[id] = ByteProgress(done: done, total: total)
        if complete { finished += 1 }
    }

    private func finish(_ run: Run, _ result: Result<Void, Error>) {
        active -= 1
        finished += 1
        current = nil
        endBackgroundTask(run)
        UIApplication.shared.isIdleTimerDisabled = false
        let ownsHead = run.stopReason != .user && pending.first == run.request
        switch result {
        case .success:
            if ownsHead { pending.removeFirst() }
            attempts = 0
        case .failure(FfiError.Cancelled):
            break
        case let .failure(failure):
            guard ownsHead else { break }
            attempts += 1
            let message = Self.message(failure)
            if attempts < Self.maxAttempts {
                error = "Download interrupted, retrying: \(message)"
                let delay = backoff(attempts)
                retry = Task { [weak self] in
                    try? await Task.sleep(for: delay)
                    guard !Task.isCancelled else { return }
                    self?.retry = nil
                    self?.pump()
                }
                return
            }
            // Requests queued behind this one are dropped with it.
            attempts = 0
            error = message
            progress = [:]
            pending.removeAll()
        }
        pump()
    }

    private func endBackgroundTask(_ run: Run) {
        guard run.backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(run.backgroundTask)
        run.backgroundTask = .invalid
    }

    private static func message(_ error: Error) -> String {
        if case let FfiError.Failed(reason) = error { return reason }
        return error.localizedDescription
    }
}

/// Forwards byte progress from the download thread, at most every 200ms per model plus each
/// model's first and last report.
private final class ProgressRelay: DownloadListener, @unchecked Sendable {
    private let update: @Sendable (String, UInt64, UInt64) -> Void
    private let lock = NSLock()
    private var lastSent: [String: ContinuousClock.Instant] = [:]

    init(update: @escaping @Sendable (String, UInt64, UInt64) -> Void) {
        self.update = update
    }

    func onProgress(modelId: String, done: UInt64, total: UInt64) {
        let now = ContinuousClock.now
        let send = lock.withLock {
            let due = done == 0 || done == total || lastSent[modelId].map { now - $0 >= .milliseconds(200) } ?? true
            if due { lastSent[modelId] = now }
            return due
        }
        if send { update(modelId, done, total) }
    }
}

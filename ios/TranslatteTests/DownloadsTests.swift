import XCTest
@testable import Translatte

/// Drives `Downloads` against a fake engine, so no network is needed.
@MainActor
final class DownloadsTests: XCTestCase {
    private var defaults: UserDefaults!

    override func setUp() async throws {
        defaults = UserDefaults(suiteName: "DownloadsTests-\(UUID().uuidString)")
    }

    private func makeDownloads(_ engine: FakeEngine) -> Downloads {
        let downloads = Downloads(defaults: defaults, monitorNetwork: false, backoff: { _ in .milliseconds(10) })
        downloads.engine = engine
        return downloads
    }

    private func waitUntil(_ condition: @MainActor () -> Bool, file: StaticString = #filePath, line: UInt = #line) async {
        for _ in 0..<500 where !condition() {
            try? await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertTrue(condition(), file: file, line: line)
    }

    func testSuccessfulDownloadReportsProgressAndClearsTheRequest() async {
        let engine = FakeEngine()
        let downloads = makeDownloads(engine)
        downloads.start(["rec-latin"], allowMetered: true)
        XCTAssertTrue(downloads.running)
        XCTAssertEqual(downloads.progress["rec-latin"], ByteProgress(done: 0, total: 100))
        engine.finish(with: .success(()))
        await waitUntil { !downloads.busy }
        XCTAssertEqual(engine.calls, [["rec-latin"]])
        XCTAssertEqual(downloads.progress["rec-latin"], ByteProgress(done: 100, total: 100))
        XCTAssertNil(downloads.error)
        XCTAssertEqual(downloads.finished, 2, "one bump for the finished model, one for the run")
    }

    func testCancelStopsTheRunAndDropsPendingRequests() async {
        let engine = FakeEngine()
        let downloads = makeDownloads(engine)
        downloads.start(["pair-fr-en"], allowMetered: true)
        downloads.start(["rec-latin"], allowMetered: true)
        downloads.cancel()
        XCTAssertFalse(downloads.queued)
        XCTAssertTrue(downloads.running, "still unwinding")
        engine.finish(with: .failure(FfiError.Cancelled))
        await waitUntil { !downloads.busy }
        XCTAssertEqual(engine.calls, [["pair-fr-en"]])
        XCTAssertNil(downloads.error)
    }

    func testFailureRetriesThenGivesUp() async {
        let engine = FakeEngine(autoFail: "connection reset")
        let downloads = makeDownloads(engine)
        downloads.start(["rec-latin"], allowMetered: true)
        await waitUntil { !downloads.busy }
        XCTAssertEqual(engine.calls.count, Downloads.maxAttempts)
        XCTAssertEqual(downloads.error, "connection reset")
        XCTAssertTrue(downloads.progress.isEmpty)
    }

    func testRetryMessageWhileAttemptsRemain() async {
        let engine = FakeEngine()
        let downloads = Downloads(defaults: defaults, monitorNetwork: false, backoff: { _ in .seconds(60) })
        downloads.engine = engine
        downloads.start(["rec-latin"], allowMetered: true)
        engine.finish(with: .failure(FfiError.Failed(reason: "timed out")))
        await waitUntil { !downloads.running }
        XCTAssertEqual(downloads.error, "Download interrupted, retrying: timed out")
        XCTAssertTrue(downloads.queued)
        downloads.cancel()
        XCTAssertFalse(downloads.busy)
        XCTAssertNil(downloads.error)
    }

    func testWaitsForUnmeteredNetworkAndStopsWhenItGoesAway() async {
        let engine = FakeEngine()
        let downloads = makeDownloads(engine)
        downloads.networkChanged(available: true, metered: true)
        downloads.start(["pair-fr-en"], allowMetered: false)
        XCTAssertFalse(downloads.running)
        XCTAssertTrue(downloads.waitingForUnmetered)

        downloads.networkChanged(available: true, metered: false)
        XCTAssertTrue(downloads.running)
        XCTAssertFalse(downloads.waitingForUnmetered)

        downloads.networkChanged(available: true, metered: true)
        engine.finish(with: .failure(FfiError.Cancelled))
        await waitUntil { !downloads.running }
        XCTAssertTrue(downloads.waitingForUnmetered, "the request is kept for when Wi-Fi is back")

        downloads.networkChanged(available: true, metered: false)
        XCTAssertTrue(downloads.running)
        engine.finish(with: .success(()))
        await waitUntil { !downloads.busy }
        XCTAssertEqual(engine.calls.count, 2)
    }

    func testMeteredDownloadsDontWaitWhenAllowed() {
        let downloads = makeDownloads(FakeEngine())
        downloads.networkChanged(available: true, metered: true)
        downloads.start(["rec-latin"], allowMetered: true)
        XCTAssertTrue(downloads.running)
        XCTAssertFalse(downloads.waitingForUnmetered)
    }

    func testNoNetworkQueues() {
        let downloads = makeDownloads(FakeEngine())
        downloads.networkChanged(available: false, metered: false)
        downloads.start(["rec-latin"], allowMetered: true)
        XCTAssertFalse(downloads.running)
        XCTAssertTrue(downloads.queued)
        XCTAssertFalse(downloads.waitingForUnmetered)
    }

    func testPendingRequestsSurviveRelaunch() async {
        let first = Downloads(defaults: defaults, monitorNetwork: false)
        first.networkChanged(available: false, metered: false)
        first.start(["det", "rec-latin"], allowMetered: false)

        let engine = FakeEngine()
        let relaunched = makeDownloads(engine)
        XCTAssertEqual(relaunched.pending, [DownloadRequest(ids: ["det", "rec-latin"], allowMetered: false)])
        await waitUntil { engine.calls == [["det", "rec-latin"]] }
        engine.finish(with: .success(()))
        await waitUntil { !relaunched.busy }
        XCTAssertTrue(makeDownloads(FakeEngine()).pending.isEmpty)
    }

    func testRelaunchWaitsForTheFirstNetworkUpdate() async {
        let first = Downloads(defaults: defaults, monitorNetwork: false)
        first.networkChanged(available: false, metered: false)
        first.start(["rec-latin"], allowMetered: true)

        let engine = FakeEngine()
        let relaunched = Downloads(defaults: defaults, backoff: { _ in .milliseconds(10) })
        relaunched.engine = engine
        XCTAssertFalse(relaunched.running, "the network isn't known yet")
        XCTAssertTrue(relaunched.queued)

        await waitUntil { relaunched.running }
        relaunched.cancel()
        engine.finish(with: .failure(FfiError.Cancelled))
        await waitUntil { !relaunched.busy }
    }

    func testMeteredPromptOnlyForLargeDownloads() {
        XCTAssertTrue(Downloads.needsMeteredPrompt(bytes: 2_500_000_000, metered: true))
        XCTAssertFalse(Downloads.needsMeteredPrompt(bytes: 8_000_000, metered: true))
        XCTAssertFalse(Downloads.needsMeteredPrompt(bytes: 2_500_000_000, metered: false))
    }

    func testModelRemainingBytes() {
        let info = ModelInfo(id: "x", displayName: "X", core: false, sizeBytes: 8_000_000, downloadedBytes: 3_000_000, state: .incomplete, languages: [])
        XCTAssertEqual(info.remainingBytes, 5_000_000)
    }
}

/// Blocks each download until the test finishes it, reporting full progress on success.
private final class FakeEngine: ModelDownloading, @unchecked Sendable {
    private let lock = NSLock()
    private let autoFail: String?
    private var outcomes: [DispatchSemaphore] = []
    private var results: [Result<Void, Error>] = []
    private(set) var calls: [[String]] = []

    init(autoFail: String? = nil) {
        self.autoFail = autoFail
    }

    func models() throws -> [ModelInfo] {
        ["det", "pair-fr-en", "rec-latin"].map {
            ModelInfo(id: $0, displayName: $0, core: $0 == "det", sizeBytes: 100, downloadedBytes: 0, state: .notInstalled, languages: [])
        }
    }

    func download(modelIds: [String], listener: DownloadListener, token: DownloadToken) throws {
        let gate = DispatchSemaphore(value: 0)
        lock.withLock {
            calls.append(modelIds)
            outcomes.append(gate)
        }
        if let autoFail { throw FfiError.Failed(reason: autoFail) }
        gate.wait()
        let result = lock.withLock { results.removeFirst() }
        if case .success = result {
            for id in modelIds { listener.onProgress(modelId: id, done: 100, total: 100) }
        }
        try result.get()
    }

    /// Ends the oldest download still blocked, waiting for it to start if needed.
    func finish(with result: Result<Void, Error>) {
        var gate: DispatchSemaphore?
        for _ in 0..<500 {
            gate = lock.withLock { outcomes.isEmpty ? nil : outcomes.removeFirst() }
            if gate != nil { break }
            Thread.sleep(forTimeInterval: 0.01)
        }
        lock.withLock { results.append(result) }
        gate?.signal()
    }
}

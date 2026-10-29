import XCTest
@testable import HoplaKit

final class PolicyTests: XCTestCase {
    /// SS1/SS2 policy: secure layer always on, cover in the app switcher and while captured.
    func testSS_coverPolicy() {
        XCTAssertTrue(ScreenCapturePolicy.secureLayerAlwaysOn)
        XCTAssertFalse(ScreenCapturePolicy.sendsScreenshotNotice)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .active, isCaptured: false), .none)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .inactive, isCaptured: false), .appSwitcher)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .background, isCaptured: false), .appSwitcher)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .active, isCaptured: true), .recording)
    }

    /// GATT values must match Android exactly (reads the Kotlin source when present).
    func testGattMatchesAndroid() throws {
        let kt = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("android/app/src/main/java/chat/omcp/app/core/OmcpCore.kt")
        guard let src = try? String(contentsOf: kt, encoding: .utf8) else { throw XCTSkip("Android source not available") }
        for (name, v) in [("SERVICE", OmcpGatt.service), ("INBOX", OmcpGatt.inbox), ("OUTBOX", OmcpGatt.outbox),
                          ("CONTROL", OmcpGatt.control), ("CCCD", OmcpGatt.cccd)] {
            XCTAssertTrue(src.contains("const val \(name) = \"\(v)\""), name)
        }
        XCTAssertTrue(src.contains("MANUFACTURER_ID = 0xFFFF"))
        XCTAssertEqual(OmcpGatt.manufacturerId, 0xFFFF)
        XCTAssertTrue(src.contains("const val LOCAL_NAME = \"\(OmcpGatt.localName)\""))
    }

    func testFileLimitFromCore() throws {
        let (r, core) = makeRepo()
        try r.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "A")
        let c = try r.addContact(qr: "omcp:1:b0b:Bob", action: .chatWithoutVerifying)
        XCTAssertEqual(FilePolicy.check(size: 50_000_000, core: core), .ok)
        XCTAssertEqual(FilePolicy.check(size: 50_000_001, core: core), .tooLarge(max: 50_000_000))
        XCTAssertEqual(FilePolicy.label(core.maxFileBytes()), "50 MB")
        core.maxFile = 10 // UI follows the core, not a constant
        XCTAssertThrowsError(try r.checkFile(size: 11))
        XCTAssertThrowsError(try r.sendFile(c.id, name: "a", mime: "a/b", bytes: Data(count: 11)))
        XCTAssertNoThrow(try r.sendFile(c.id, name: "a", mime: "a/b", bytes: Data(count: 10)))
    }

    final class MemSink: FileSink {
        var saved: [String] = []
        func save(_ file: ReceivedFileInfo) -> String? { saved.append(file.name); return "/tmp/\(file.name)" }
    }

    func testFileTransferStatesFollowLinkEvents() throws {
        let (r, core) = makeRepo()
        try r.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "A")
        let c = try r.addContact(qr: "omcp:1:b0b:Bob", action: .chatWithoutVerifying)
        let id = try r.sendFile(c.id, name: "a.pdf", mime: "application/pdf", bytes: Data(count: 200_000))
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .queued)
        r.onFileProgress(FileProgressInfo(transferId: id, contactId: c.id, incoming: false, name: "a.pdf", size: 200_000, doneChunks: 2, totalChunks: 4))
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .sending)
        XCTAssertEqual(r.messages[c.id]?.last?.file?.doneBytes, 100_000)
        r.pauseTransfers(c.id)
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .paused)
        r.onFileProgress(FileProgressInfo(transferId: id, contactId: c.id, incoming: false, name: "a.pdf", size: 200_000, doneChunks: 4, totalChunks: 4))
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .sent)

        let sink = MemSink()
        r.fileSink = sink
        r.onFileProgress(FileProgressInfo(transferId: "in1", contactId: c.id, incoming: true, name: "b.jpg", size: 10, doneChunks: 0, totalChunks: 1))
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .receiving)
        r.onFileReceived(ReceivedFileInfo(transferId: "in1", contactId: c.id, name: "b.jpg", mime: "image/jpeg", bytes: Data(count: 10)))
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .received)
        XCTAssertEqual(r.messages[c.id]?.last?.file?.localPath, "/tmp/b.jpg")
        XCTAssertEqual(sink.saved, ["b.jpg"])
        r.onFileProgress(FileProgressInfo(transferId: "in2", contactId: c.id, incoming: true, name: "c", size: 10, doneChunks: 0, totalChunks: 1))
        r.onFileFailed(FileFailureInfo(transferId: "in2", contactId: c.id, reason: "integrity"))
        XCTAssertEqual(r.messages[c.id]?.last?.file?.state, .integrityFailed)
        r.cancelFile(c.id, transferId: id)
        XCTAssertFalse(r.messages[c.id]!.contains { $0.id == id })
        XCTAssertFalse(core.files.contains(id))
    }

    func testGatewayFromCorePolicy() {
        let (r, core) = makeRepo()
        let s = r.store
        XCTAssertFalse(GatewayPolicy.state(store: s, core: core, network: .wifi, metered: false).decision.bridge)
        s.gatewayEnabled = true
        let st = GatewayPolicy.state(store: s, core: core, network: .cellular, metered: true)
        XCTAssertFalse(st.decision.bridge, "Wi-Fi only by default")
        XCTAssertEqual(st.usedBytes, 0)
        XCTAssertEqual(st.dailyCapBytes, 500_000_000)
        s.gatewayAllowMobile = true
        let m = GatewayPolicy.state(store: s, core: core, network: .cellular, metered: true).decision
        XCTAssertTrue(m.bridge); XCTAssertTrue(m.meteredWarning)
    }

    /// Verified badge only on verified contacts.
    func testContactAddVerifyOrNot() throws {
        let (r, core) = makeRepo()
        try r.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "A")
        let v = try r.addContact(qr: "omcp:1:aaa:Ann", action: .verifyAndChat)
        let u = try r.addContact(qr: "omcp:1:bbb:Ben", action: .chatWithoutVerifying)
        XCTAssertTrue(v.showsVerifiedBadge)
        XCTAssertFalse(u.showsVerifiedBadge)
        let pre = try core.previewContact("omcp:1:ccc:Ann")
        XCTAssertEqual(pre.mismatchWith?.id, "aaa")
        XCTAssertEqual(pre.contact.safetyNumber.count, 12)
        XCTAssertThrowsError(try core.previewContact("https://evil"))
    }

    func testRetryLimiterBackoffCaps() {
        let clock = Clock()
        let l = RetryLimiter(clock: clock.fn)
        for _ in 0..<20 { l.onFailure() }
        XCTAssertEqual(l.remainingLockMs(), RetryLimiter.maxMs)
        l.onSuccess()
        XCTAssertTrue(l.canTry)
    }
}

/// iOS News fetch mirrors Android: any phone with internet fetches, under the network rules.
final class NewsFetchRulesTests: XCTestCase {
    func testAnyPhoneFetchesWifiOnlyByDefault() {
        let core = FakeOmcpCore(); let store = InMemoryAppStore()
        core.newsPlan = NewsFetchPlan(feeds: ["https://news.google.com/rss/x"], blocked: nil)
        XCTAssertFalse(store.gatewayEnabled)
        XCTAssertEqual(NewsFetchRules.run(core: core, store: store, network: .wifi, metered: false, nowMs: 1) { _ in "<rss/>" }, 3)
        XCTAssertEqual(core.lastNewsWifiOnly, true)
        store.gatewayAllowMobile = true
        core.newsPlan = NewsFetchPlan(blocked: "daily_cap")
        core.newsIngested = []
        XCTAssertEqual(NewsFetchRules.run(core: core, store: store, network: .cellular, metered: true, nowMs: 1) { _ in "<rss/>" }, 0)
        XCTAssertEqual(core.lastNewsWifiOnly, false)
        XCTAssertTrue(core.newsIngested.isEmpty)
    }
}

/// Live tab rules (mirrors Android LiveViewModel).
final class LiveRulesTests: XCTestCase {
    func testGoLiveNeedsAListenerAndSplitsOwnStream() {
        XCTAssertFalse(LiveRules.canGoLive(picked: [], hosting: false))
        XCTAssertTrue(LiveRules.canGoLive(picked: ["c"], hosting: false))
        XCTAssertFalse(LiveRules.canGoLive(picked: ["c"], hosting: true))
        let a = LiveStreamModel(streamIdHex: "aa", title: "Mine", hostContactId: "", mine: true, listening: false, video: false, startedMs: 1)
        let b = LiveStreamModel(streamIdHex: "bb", title: "Town hall", hostContactId: "h", mine: false, listening: true, video: false, startedMs: 2)
        let s = LiveRules.split([b, a])
        XCTAssertEqual(s.mine, a)
        XCTAssertEqual(s.others, [b])
        XCTAssertEqual(LiveStatsModel(played: 95, lost: 5).lossPercent, 5)
        XCTAssertEqual(LiveStatsModel().lossPercent, 0)
        XCTAssertTrue(FakeOmcpCore().liveStreams().isEmpty)
    }
}

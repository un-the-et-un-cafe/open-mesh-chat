import Foundation
import XCTest
import HoplaKit
#if canImport(HoplaRust)
import HoplaRust // Linux: scripts/verify-linux.sh
#else
@testable import OpenMeshChat // iOS simulator (xcodebuild test)
#endif

/// Real Rust core through the Swift bindings: two nodes, QR add with safety numbers, a Noise
/// link over in-memory "BLE", burn after read with OPENED, and iCloud/Drive backups (O2–O4).
final class RustBridgeTests: XCTestCase {
    func node(_ name: String) throws -> (RustOmcpCore, MeshRepository) {
        let core = RustOmcpCore()
        let repo = MeshRepository(core: core, store: InMemoryAppStore())
        try repo.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: name)
        return (core, repo)
    }

    /// Feed PDUs back and forth until both sides are quiet; collect what each side received.
    func pump(_ a: CoreLink, _ b: CoreLink, _ first: LinkResult, into ra: MeshRepository, _ rb: MeshRepository) throws {
        var queue: [(toB: Bool, pdus: [Data])] = [(true, first.outgoing)]
        while !queue.isEmpty {
            let (toB, pdus) = queue.removeFirst()
            for p in pdus {
                let out = try (toB ? b : a).onPdu(p)
                let repo = toB ? rb : ra
                out.received.forEach(repo.onReceived)
                out.opened.forEach(repo.onOpened)
                if !out.outgoing.isEmpty { queue.append((!toB, out.outgoing)) }
            }
        }
    }

    /// Encrypted local store: contacts, verification, chat, queue and an unopened burn message
    /// survive an app restart; once burned it is gone after the next restart and never on disk.
    func testMessagesQueueAndBurnSurviveRestart() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("omcp-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appendingPathComponent("omcp-store.db").path
        let store = InMemoryAppStore()
        var t = Int64(Date().timeIntervalSince1970 * 1000)
        func bob() -> (RustOmcpCore, MeshRepository) {
            let c = RustOmcpCore()
            return (c, MeshRepository(core: c, store: store, dbPath: db, clock: { t }))
        }
        let (ca, ra) = try node("Ana")
        var (cb, rb) = bob()
        try rb.saveIdentity(phrase: try cb.generateRecoveryPhrase(), name: "Ben")
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .chatWithoutVerifying)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .verifyAndChat)
        XCTAssertTrue(store.contacts.isEmpty) // encrypted store, not UserDefaults
        XCTAssertNotNil(store.storageKey)
        let secret = "burn: meet at gate 7"
        try ra.send(bOnA.id, text: "hello Ben")
        try ra.send(bOnA.id, text: secret, burnOnce: .timer(10))
        var la = try ca.openInitiatorLink(bOnA.id)
        var lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        try rb.send(aOnB.id, text: "queued for Ana")
        la.close(); lb.close()

        (cb, rb) = bob()
        XCTAssertTrue(rb.restore())
        XCTAssertEqual(rb.contacts.first?.verified, true)
        let chat = rb.messages[aOnB.id] ?? []
        XCTAssertEqual(chat.map(\.text), ["hello Ben", "", "queued for Ana"])
        XCTAssertEqual(chat[1].burnState, .unopened)
        XCTAssertEqual(chat[2].status, .queued)
        XCTAssertEqual(cb.queuedFor(aOnB.id), 1)
        XCTAssertEqual(try cb.openBurn(chat[1].id, nowMs: t)?.text, secret)
        t += 10_000
        XCTAssertEqual(cb.tickBurn(nowMs: t), [chat[1].id])

        (cb, rb) = bob()
        XCTAssertTrue(rb.restore())
        XCTAssertEqual((rb.messages[aOnB.id] ?? []).map(\.text), ["hello Ben", "queued for Ana"])
        XCTAssertFalse(cb.burnIsLive(chat[1].id))
        la = try ca.openInitiatorLink(bOnA.id)
        lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        XCTAssertEqual(ra.messages[bOnA.id]?.last?.text, "queued for Ana")
        XCTAssertEqual(cb.queuedFor(aOnB.id), 0)
        la.close(); lb.close()
        for f in try FileManager.default.contentsOfDirectory(atPath: dir.path) {
            let bytes = try Data(contentsOf: dir.appendingPathComponent(f))
            XCTAssertNil(bytes.range(of: Data(secret.utf8)), "burn content on disk in \(f)")
            XCTAssertNil(bytes.range(of: Data("hello Ben".utf8)), "plaintext on disk in \(f)")
        }
    }

    func testUnreadableStoreIsSetAsideNotFatal() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("omcp-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appendingPathComponent("omcp-store.db").path
        let store = InMemoryAppStore()
        let c1 = RustOmcpCore()
        try MeshRepository(core: c1, store: store, dbPath: db).saveIdentity(phrase: try c1.generateRecoveryPhrase(), name: "Ben")
        store.storageKey = Data(repeating: 7, count: 32) // e.g. Keychain reset
        XCTAssertTrue(MeshRepository(core: RustOmcpCore(), store: store, dbPath: db).restore())
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: dir.path).contains { $0.hasPrefix("omcp-store.db.unreadable-") })
    }

    func testLinkedAndPolicyFromCore() {
        let c = RustOmcpCore()
        XCTAssertTrue(c.linked)
        XCTAssertEqual(c.maxFileBytes(), 50_000_000)
        XCTAssertEqual(c.gatewayDailyCapBytes(), 500_000_000)
        XCTAssertFalse(c.gatewayDecision(wifiOnly: true, network: .cellular, metered: true).bridge)
        XCTAssertTrue(c.gatewayDecision(wifiOnly: false, network: .cellular, metered: true).meteredWarning)
    }

    final class MemSink: FileSink {
        var bytes: Data?
        func save(_ file: ReceivedFileInfo) -> String? { bytes = file.bytes; return "mem:\(file.name)" }
    }

    /// File transfer through the real core: 64 KiB chunks drained over several flushes.
    func testFileTransferIn64kChunks() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .chatWithoutVerifying)
        _ = try rb.addContact(qr: try ca.qrPayload(), action: .chatWithoutVerifying)
        let la = try ca.openInitiatorLink(bOnA.id)
        let lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        let data = Data((0..<200_000).map { UInt8($0 % 251) })
        let sink = MemSink()
        rb.fileSink = sink
        let id = try ra.sendFile(bOnA.id, name: "flyer.jpg", mime: "image/jpeg", bytes: data)
        var flushes = 0
        var more = true
        while more {
            let out = try la.flush()
            flushes += 1
            more = out.morePending
            ra.applyFileEvents(out)
            for p in out.outgoing {
                XCTAssertLessThanOrEqual(p.count, 512)
                rb.applyFileEvents(try lb.onPdu(p))
            }
        }
        XCTAssertGreaterThanOrEqual(flushes, 2)
        XCTAssertEqual(sink.bytes, data)
        XCTAssertEqual(rb.messages.values.flatMap { $0 }.first?.file?.state, .received)
        XCTAssertEqual(ra.messages[bOnA.id]?.first { $0.id == id }?.file?.state, .sent)
        XCTAssertThrowsError(try ra.sendFile(bOnA.id, name: "big", mime: "", bytes: Data(count: 50_000_001)))
    }

    func testQrSafetyNumbersAndChatAndBurn() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let pa = try ca.previewContact(try cb.qrPayload())
        XCTAssertEqual(pa.contact.safetyNumber.count, 12)
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .verifyAndChat)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .chatWithoutVerifying)
        XCTAssertEqual(bOnA.safetyNumber, aOnB.safetyNumber)
        XCTAssertTrue(bOnA.showsVerifiedBadge)
        XCTAssertFalse(aOnB.showsVerifiedBadge)

        try ra.send(bOnA.id, text: "hello")
        try ra.send(bOnA.id, text: "burn me", burnOnce: .onOpen())
        let la = try ca.openInitiatorLink(bOnA.id)
        let lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        let got = rb.messages[aOnB.id] ?? []
        XCTAssertEqual(got.map(\.text), ["hello", ""])
        let burnId = got[1].id
        XCTAssertNotNil(got[1].burn)
        XCTAssertTrue(rb.search("burn").isEmpty)

        let opened = try rb.openBurn(aOnB.id, msgId: burnId)
        XCTAssertEqual(opened?.text, "burn me")
        XCTAssertNil(try rb.openBurn(aOnB.id, msgId: burnId), "content only once")
        try rb.closeBurn(aOnB.id, msgId: burnId)
        XCTAssertFalse(cb.burnIsLive(burnId))
        // OPENED receipt flows back and shreds the sender copy.
        try pump(lb, la, try lb.flush(), into: rb, ra)
        XCTAssertFalse(ca.burnIsLive(burnId))
    }

    /// O2 + O3 + O4 with the real Argon2id / XChaCha20-Poly1305 sealing.
    func testBackupsThroughOnboarding() throws {
        let (core, repo) = try node("Ana")
        _ = try repo.addContact(qr: try RustOmcpCore().qrPayloadFor(name: "Ben"), action: .verifyAndChat)
        let phrase = try core.recoveryPhrase()
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let keys = InMemoryBackupKeyStore()
        let settings = BackupSettings(repo: repo, stores: [icloud], keyStore: keys)
        try settings.backUp(backupICloud, password: nil)
        let blob = try XCTUnwrap(icloud.blob)
        XCTAssertNil(blob.range(of: Data(phrase.utf8)), "O2 phrase")
        XCTAssertNil(blob.range(of: try XCTUnwrap(keys.key)), "O2 key")

        // O3: reinstall, restore from iCloud with the Keychain key.
        let core2 = RustOmcpCore()
        let repo2 = MeshRepository(core: core2, store: InMemoryAppStore())
        let f = OnboardingFlow(repo: repo2, iCloud: icloud, drive: nil, keyStore: keys)
        f.detectBackup()
        XCTAssertEqual(f.state.step, .restorePrompt)
        XCTAssertTrue(f.restoreFromCloud())
        XCTAssertEqual(core2.identity()?.peerIdHex, core.identity()?.peerIdHex)
        XCTAssertEqual(repo2.contacts.map(\.verified), [true])

        // O4: password mode, wrong password fails cleanly.
        try settings.changePassword(backupICloud, to: "violet-orbit-71-tundra")
        let f2 = OnboardingFlow(repo: MeshRepository(core: RustOmcpCore(), store: InMemoryAppStore()),
                                iCloud: icloud, drive: nil, keyStore: keys)
        f2.detectBackup()
        XCTAssertEqual(f2.state.foundBackup?.needsPassword, true)
        f2.setRestorePassword("wrong-password-123")
        XCTAssertFalse(f2.restoreFromCloud())
        XCTAssertEqual(f2.state.error, "wrongpw")
        f2.setRestorePassword("violet-orbit-71-tundra")
        XCTAssertTrue(f2.restoreFromCloud())
    }

    /// CB1/CB4 (padded incremental blobs, restore chats) + RS1/RS2 (notice, verified cleared, old QR fails).
    func testChatBackupRestoreNoticeAndReverify() throws {
        let t = Int64(Date().timeIntervalSince1970 * 1000)
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .verifyAndChat)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .verifyAndChat)
        let oldQr = try ca.qrPayload()
        try ra.send(bOnA.id, text: "see you at the market")
        var la = try ca.openInitiatorLink(bOnA.id)
        var lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        la.close(); lb.close()

        let cloud = InMemoryChatBlobStore()
        XCTAssertEqual(try ChatBackupSync(core: ca, blobs: cloud, clock: { t }).runText(onWifi: true, force: true), 1)
        XCTAssertFalse(cloud.blobs.isEmpty)
        for b in cloud.blobs.values { XCTAssertEqual(b.count % 65536, 0) }

        let idBlob = try ca.createBackup(password: "correct horse battery staple", deviceHint: "test", nowMs: t)
        let restored = try ca.openBackup(idBlob, password: "correct horse battery staple")
        let c2 = RustOmcpCore()
        let r2 = MeshRepository(core: c2, store: InMemoryAppStore(), clock: { t + 1000 })
        r2.chatBackup = ChatBackupSync(core: c2, blobs: cloud, clock: { t + 1000 })
        try r2.restoreFromBackup(restored, name: "Ana", method: "icloud")
        let res = try XCTUnwrap(try r2.restoreChats())
        XCTAssertGreaterThanOrEqual(res.messages, 1)
        XCTAssertEqual(r2.messages[bOnA.id]?.last(where: { $0.status != .system })?.text, "see you at the market")
        XCTAssertNotNil(r2.restoredAtMs)

        // RS1: Ben sees the notice on the next link. RS2: verified cleared, old QR fails.
        var notices: [RestoreNoticeInfo] = []
        // v0.1.4a: the restored device has a new X25519 key, so it dials first.
        la = try c2.openInitiatorLink(bOnA.id)
        lb = try cb.openResponderLink()
        var queue: [(toB: Bool, pdus: [Data])] = [(true, try la.start().outgoing)]
        while !queue.isEmpty {
            let (toB, pdus) = queue.removeFirst()
            for p in pdus {
                let out = try (toB ? lb : la).onPdu(p)
                if toB { notices += out.restoreNotices; rb.applyFileEvents(out) }
                if !out.outgoing.isEmpty { queue.append((!toB, out.outgoing)) }
            }
        }
        XCTAssertEqual(notices.count, 1)
        let c = try XCTUnwrap(rb.contact(aOnB.id))
        XCTAssertFalse(c.verified)
        XCTAssertTrue(c.restoredUnverified)
        XCTAssertEqual(rb.messages[aOnB.id]?.last?.status, .system)
        XCTAssertThrowsError(try rb.addContact(qr: oldQr, action: .verifyAndChat)) { e in
            XCTAssertEqual(e as? CoreError, .verificationFailed)
        }
        let re = try rb.addContact(qr: try c2.qrPayload(), action: .verifyAndChat)
        XCTAssertTrue(re.verified)
        XCTAssertFalse(re.restoredUnverified)
    }

    /// Keep sent media + one-message Ask to resend over the real core (REQUIREMENTS v0.1.4b).
    func testKeptMediaSettingsRoundTrip() throws {
        let core = RustOmcpCore()
        try MeshRepository(core: core, store: InMemoryAppStore()).saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "Ana")
        XCTAssertEqual(core.keptMediaSettings(), KeptMediaPrefs())
        XCTAssertEqual(try core.setKeptMediaSettings(KeptMediaPrefs(maxDays: 30, maxBytes: 500_000_000), nowMs: 1), 0)
        XCTAssertEqual(core.keptMediaSettings(), KeptMediaPrefs(maxDays: 30, maxBytes: 500_000_000))
        XCTAssertEqual(core.keptMediaUsage(), HoplaKit.KeptMediaUsage(count: 0, bytes: 0))
    }


    // MARK: Multi-hop mesh relay (PROTOCOL §22)

    private func linkUp(_ x: RustOmcpCore, _ y: RustOmcpCore) throws -> (CoreLink, CoreLink, [IncomingText]) {
        let lx = try x.openInitiatorLink(x.contacts().first { $0.id == y.identity()!.peerIdHex }!.id)
        let ly = try y.openResponderLink()
        var got: [IncomingText] = []
        var queue: [(toY: Bool, pdus: [Data])] = [(true, try lx.start().outgoing)]
        func run() throws {
            while !queue.isEmpty {
                let (toY, pdus) = queue.removeFirst()
                for p in pdus {
                    let out = try (toY ? ly : lx).onPdu(p)
                    got += out.received
                    if !out.outgoing.isEmpty { queue.append((!toY, out.outgoing)) }
                }
            }
        }
        try run()
        for _ in 0..<4 {
            queue.append((true, try lx.flush().outgoing)); queue.append((false, try ly.flush().outgoing))
            try run()
        }
        XCTAssertTrue(lx.isEstablished && ly.isEstablished)
        XCTAssertNotEqual(lx.handle, 0)
        return (lx, ly, got)
    }

    /// A → B → C where A and C are never connected: B carries an opaque envelope, C gets it.
    func testMeshChainAtoBtoCNeverInRange() throws {
        let a = try node("A").0
        let b = try node("B").0
        let c = try node("C").0
        for (x, y) in [(a, b), (b, c), (a, c)] {
            _ = try x.addContactFromQr(try y.qrPayload())
            _ = try y.addContactFromQr(try x.qrPayload())
        }
        let cid = c.identity()!.peerIdHex, aid = a.identity()!.peerIdHex
        _ = try a.queueMessage(cid, text: "via B", burn: nil)
        XCTAssertEqual(a.meshStats().ownPending, 1)
        let (ab, ba, gotB) = try linkUp(a, b)
        XCTAssertTrue(gotB.isEmpty)
        XCTAssertEqual(b.meshStats().carried, 1)
        ab.close(); ba.close()
        let (bc, cb, gotC) = try linkUp(b, c)
        XCTAssertEqual(gotC.map { $0.text }, ["via B"])
        XCTAssertEqual(gotC.first?.contactId, aid)
        bc.close(); cb.close()
        XCTAssertEqual(b.meshSettings(), MeshRelayPrefs(enabled: true, capBytes: 50_000_000))
        try b.setMeshSettings(MeshRelayPrefs(enabled: false, capBytes: 10_000_000))
        XCTAssertEqual(b.meshStats().carried, 0)
        XCTAssertEqual(a.meshBeacon().count, 6)
    }
}

extension RustOmcpCore {
    /// A throwaway identity's QR, for tests.
    func qrPayloadFor(name: String) throws -> String {
        _ = try load(phrase: try generateRecoveryPhrase(), displayName: name)
        return try qrPayload()
    }
}

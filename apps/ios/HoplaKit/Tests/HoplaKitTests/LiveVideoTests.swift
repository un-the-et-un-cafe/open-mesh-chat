import XCTest
@testable import HoplaKit

/// Live video tree client (same cases as Android LiveVideoTest).
final class LiveVideoTests: XCTestCase {
    final class Peers: RtcPeers {
        var log: [String] = []
        var kids: [UInt64] = []
        func offerTo(_ peer: String) { log.append("offer:\(peer)") }
        func onRemote(_ peer: String, kind: String, body: String) { log.append("\(kind):\(peer)") }
        func close(_ peer: String) { log.append("close:\(peer)") }
        func closeAll() { log.append("closeAll") }
        func setKey(_ key: LiveMediaKeyModel) { kids.append(key.kid) }
    }

    private func p(_ i: Int) -> String { "a" + String(repeating: "0", count: 62) + String(i % 10) }
    private let host = LiveVideoController.host

    func testHostFanoutSixAndKeyRotation() {
        let core = FakeOmcpCore(), peers = Peers()
        let c = LiveVideoController(core: core, peers: peers)
        c.startHosting(now: 0)
        XCTAssertEqual(c.state, .hosting)
        core.fakeSignals = (1...8).map { LiveSignalModel(peerHex: p($0), kind: "assign", body: "") }
        c.poll(now: 10)
        XCTAssertEqual(c.children.count, 6)
        core.fakeMediaKey = LiveMediaKeyModel(baseKey: Data(repeating: 2, count: 16), kid: 1)
        c.poll(now: 20)
        XCTAssertEqual(peers.kids, [0, 1])
        c.onFailed(p(1))
        XCTAssertEqual(core.droppedPeers, [p(1)])
    }

    func testViewerFanoutFourAndParentOnly() {
        let core = FakeOmcpCore(), peers = Peers()
        let c = LiveVideoController(core: core, peers: peers)
        c.startWatching(now: 0)
        XCTAssertEqual(core.videoJoins, 1)
        core.fakeSignals = [LiveSignalModel(peerHex: host, kind: "offer", body: "sdp"),
                            LiveSignalModel(peerHex: p(9), kind: "ice", body: "x")]
            + (1...5).map { LiveSignalModel(peerHex: p($0), kind: "assign", body: "") }
        c.poll(now: 10)
        XCTAssertTrue(peers.log.contains("offer:\(host)"))
        XCTAssertFalse(peers.log.contains("ice:\(p(9))"))
        XCTAssertEqual(c.children.count, 4)
        c.onConnected(host)
        XCTAssertEqual(c.state, .watching)
    }

    func testNoDirectPathFallsBackToMeshAudio() {
        let core = FakeOmcpCore(), peers = Peers()
        let c = LiveVideoController(core: core, peers: peers, connectTimeoutMs: 1_000)
        c.startWatching(now: 1)
        c.poll(now: 500)
        XCTAssertEqual(c.state, .connecting)
        c.poll(now: 1_600)
        XCTAssertEqual(c.state, .noDirectPath)
        XCTAssertEqual(core.sentSignals.last, LiveSignalModel(peerHex: host, kind: "bye", body: ""))
        XCTAssertEqual(LiveVideoController.messageKey(.noDirectPath), "live_video_no_path")
    }

    func testParkedAndIceFailure() {
        let core = FakeOmcpCore(), peers = Peers()
        let c = LiveVideoController(core: core, peers: peers)
        c.startWatching(now: 0)
        core.fakeSignals = [LiveSignalModel(peerHex: host, kind: "offer", body: "sdp")]
        c.poll(now: 1)
        c.onFailed(host)
        XCTAssertEqual(c.state, .noDirectPath)
        c.startWatching(now: 2)
        core.fakeSignals = [LiveSignalModel(peerHex: host, kind: "bye", body: "parked")]
        c.poll(now: 3)
        XCTAssertEqual(c.state, .noSlot)
    }

    func testIceBodyMatchesAndroid() {
        let b = LiveVideoController.encodeIce(mid: "video", index: 1, candidate: "candidate:1 1 udp 1 198.51.100.7 1 typ srflx")
        XCTAssertEqual(b, "video\n1\ncandidate:1 1 udp 1 198.51.100.7 1 typ srflx")
        let d = LiveVideoController.decodeIce(b)
        XCTAssertEqual(d?.mid, "video"); XCTAssertEqual(d?.index, 1)
        XCTAssertNil(LiveVideoController.decodeIce("bad"))
    }
}

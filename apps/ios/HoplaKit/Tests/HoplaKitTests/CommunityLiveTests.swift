import XCTest
@testable import HoplaKit

/// Community live client rules (same cases as Android CommunityLiveTest).
final class CommunityLiveTests: XCTestCase {
    func testRelayingViewerClosesEveryPeerWhenTheStreamIsGone() {
        let core = FakeOmcpCore(), peers = LiveVideoTests.Peers()
        let c = LiveVideoController(core: core, peers: peers)
        c.startWatching(now: 0)
        c.poll(now: 1)
        XCTAssertEqual(c.state, .connecting)
        core.fakeMediaKey = nil // team takedown: Rust dropped the stream key
        c.poll(now: 2)
        XCTAssertEqual(c.state, .off)
        XCTAssertTrue(peers.log.contains("closeAll"))
    }

    func testLiveStartNotificationIsGenericEvenWithPreviewsOn() {
        let (r, core) = makeRepo()
        let n = RecordingNotifier()
        r.notifier = n
        XCTAssertFalse(r.store.privateNotifications)
        core.fakeLiveStarted = [("s1", "c1")]
        r.notifyLiveStarts()
        XCTAssertEqual(n.posted.count, 1)
        XCTAssertEqual(n.posted[0].title, "Hopla")
        XCTAssertEqual(n.posted[0].body, r.genericBody)
        XCTAssertFalse(n.posted[0].showAvatar)
        r.notifyLiveStarts()
        XCTAssertEqual(n.posted.count, 1, "each start notifies once")
    }

    func testStreamModelCarriesTheCommunity() {
        let s = LiveStreamModel(streamIdHex: "a", title: "t", hostContactId: "h", mine: false, listening: false,
                                video: false, startedMs: 1, communityId: "c1")
        XCTAssertEqual(s.communityId, "c1")
        XCTAssertEqual(LiveStreamModel(streamIdHex: "a", title: "t", hostContactId: "", mine: true, listening: false,
                                       video: false, startedMs: 1).communityId, "")
    }
}

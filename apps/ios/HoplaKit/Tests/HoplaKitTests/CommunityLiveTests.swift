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

    func testDefaultModeShadeNamesHostAndCommunityLockScreenGeneric() {
        let (r, core) = makeRepo()
        let n = RecordingNotifier()
        r.notifier = n
        core.fakeCommunities = [CommunityModel(id: "c1", name: "Choir", members: 3, founderIsMe: false)]
        core.fakeMembers["c1"] = [CommunityMemberModel(peerId: "h1", name: "Ana", isContact: true, isMe: false)]
        core.fakeStreams = [LiveStreamModel(streamIdHex: "s1", title: "Warm-up", hostContactId: "h1", mine: false,
                                            listening: false, video: false, startedMs: 1, communityId: "c1")]
        XCTAssertFalse(r.store.privateNotifications)
        core.fakeLiveStarted = [("s1", "c1")]
        r.notifyLiveStarts()
        XCTAssertEqual(n.posted.count, 1)
        XCTAssertEqual(n.posted[0].title, "Ana is live in Choir")
        XCTAssertEqual(n.posted[0].body, "Warm-up · Tap to listen")
        XCTAssertTrue(n.posted[0].hideOnLockScreen, "lock screen: hidden-previews category, no title")
        XCTAssertEqual(n.posted[0].hiddenPreviewsPlaceholder, r.genericBody)
        XCTAssertEqual(n.posted[0].kind, "live")
        r.notifyLiveStarts()
        XCTAssertEqual(n.posted.count, 1, "each start notifies once")
    }

    func testPrivateModeLiveNotificationIsGeneric() {
        let (r, core) = makeRepo()
        let n = RecordingNotifier()
        r.notifier = n
        r.store.privateNotifications = true
        core.fakeLiveStarted = [("s1", "c1")]
        r.notifyLiveStarts()
        XCTAssertEqual(n.posted[0].title, "Hopla")
        XCTAssertEqual(n.posted[0].body, r.genericBody)
        XCTAssertTrue(n.posted[0].isPrivate)
    }

    func testRemoveFromLiveCountsWhoGetsTheNewKey() {
        let m = ["me", "a", "b", "c", "d"]
        XCTAssertEqual(CommunityLiveRules.rekeyRecipients(members: m, me: "me", removed: ["a"]), 3)
        XCTAssertEqual(CommunityLiveRules.rekeyRecipients(members: m, me: "me", removed: ["a", "b"]), 2)
        XCTAssertTrue(CommunityLiveRules.isFull(members: 32))
        XCTAssertFalse(CommunityLiveRules.isFull(members: 31))
    }

    func testStreamIdGroupingReportAndReasons() {
        let id = "00112233445566778899AABBCCDDEEFF"
        XCTAssertEqual(CommunityLiveRules.groupStreamId(id), "0011 2233 4455 6677 8899 aabb ccdd eeff")
        XCTAssertTrue(CommunityLiveRules.reportText(streamIdHex: id, reason: "Spam").contains("00112233445566778899aabbccddeeff"))
        XCTAssertEqual(CommunityLiveRules.reportReasonKeys.count, 5)
        XCTAssertEqual(CommunityLiveRules.reasonKey(4), "mod_reason_4")
        XCTAssertEqual(CommunityLiveRules.reasonKey(7), "mod_reason_9")
    }

    func testStreamModelCarriesTheCommunity() {
        let s = LiveStreamModel(streamIdHex: "a", title: "t", hostContactId: "h", mine: false, listening: false,
                                video: false, startedMs: 1, communityId: "c1")
        XCTAssertEqual(s.communityId, "c1")
        XCTAssertEqual(LiveStreamModel(streamIdHex: "a", title: "t", hostContactId: "", mine: true, listening: false,
                                       video: false, startedMs: 1).communityId, "")
    }
}

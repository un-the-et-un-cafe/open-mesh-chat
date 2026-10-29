package chat.omcp.app

import chat.omcp.app.core.LiveMediaKeyUi
import chat.omcp.app.core.LiveSignalUi
import chat.omcp.app.live.LiveVideoController
import chat.omcp.app.live.LiveVideoController.Companion.HOST
import chat.omcp.app.live.RtcPeers
import chat.omcp.app.live.VideoState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LiveVideoTest {
    private class Core : chat.omcp.app.core.OmcpCore by chat.omcp.app.core.FakeOmcpCore() {
        val inbox = ArrayDeque<LiveSignalUi>()
        val sent = mutableListOf<Triple<String, String, String>>()
        val dropped = mutableListOf<String>()
        var joins = 0
        var key = LiveMediaKeyUi(ByteArray(16) { 1 }, 0)
        override fun liveVideoJoin() { joins++ }
        override fun liveSendSignal(peerHex: String, kind: String, body: String): Boolean { sent += Triple(peerHex, kind, body); return true }
        override fun liveTakeSignals(): List<LiveSignalUi> = inbox.toList().also { inbox.clear() }
        override fun liveVideoDrop(peerHex: String) { dropped += peerHex }
        override fun liveMediaKey() = key
    }

    private class Peers : RtcPeers {
        val log = mutableListOf<String>()
        val keys = mutableListOf<Long>()
        override fun offerTo(peer: String) { log += "offer:$peer" }
        override fun onRemote(peer: String, kind: String, body: String) { log += "$kind:$peer" }
        override fun close(peer: String) { log += "close:$peer" }
        override fun closeAll() { log += "closeAll" }
        override fun setKey(key: LiveMediaKeyUi) { keys += key.kid }
    }

    private fun p(i: Int) = i.toString().padStart(64, '0').replaceFirst('0', 'a')

    @Test fun hostOffersToAtMostSixChildrenAndRotatesKey() {
        val core = Core(); val peers = Peers()
        val c = LiveVideoController(core, peers)
        c.startHosting(0)
        assertEquals(VideoState.Hosting, c.state)
        (1..8).forEach { core.inbox += LiveSignalUi(p(it), "assign", "") }
        c.poll(10)
        assertEquals(6, c.children.size)
        assertEquals(6, peers.log.count { it.startsWith("offer:") })
        core.key = LiveMediaKeyUi(ByteArray(16) { 2 }, 1)
        c.poll(20)
        assertEquals(listOf(0L, 1L), peers.keys)
        c.onFailed(p(1))
        assertEquals(listOf(p(1)), core.dropped)
    }

    @Test fun viewerRelaysToAtMostFourAndAnswersParent() {
        val core = Core(); val peers = Peers()
        val c = LiveVideoController(core, peers)
        c.startWatching(0)
        assertEquals(1, core.joins)
        assertEquals(VideoState.Connecting, c.state)
        core.inbox += LiveSignalUi(HOST, "offer", "sdp")
        core.inbox += LiveSignalUi(HOST, "ice", LiveVideoController.encodeIce("0", 0, "candidate:1"))
        core.inbox += LiveSignalUi(p(9), "ice", "x") // not a neighbour: ignored
        (1..5).forEach { core.inbox += LiveSignalUi(p(it), "assign", "") }
        c.poll(10)
        assertTrue("offer:$HOST" in peers.log && "ice:$HOST" in peers.log)
        assertTrue("ice:${p(9)}" !in peers.log)
        assertEquals(4, c.children.size)
        c.onConnected(HOST)
        assertEquals(VideoState.Watching, c.state)
    }

    @Test fun noDirectPathFallsBackToMeshAudio() {
        val core = Core(); val peers = Peers()
        val c = LiveVideoController(core, peers, connectTimeoutMs = 1_000)
        c.startWatching(0)
        c.poll(500)
        assertEquals(VideoState.Connecting, c.state)
        c.poll(1_500) // nothing connected (e.g. symmetric NAT both sides, no TURN)
        assertEquals(VideoState.NoDirectPath, c.state)
        assertEquals(Triple(HOST, "bye", ""), core.sent.last())
        assertTrue("closeAll" in peers.log)
    }

    @Test fun iceFailureAndFullTreeFallBack() {
        val core = Core(); val peers = Peers()
        val c = LiveVideoController(core, peers)
        c.startWatching(0)
        core.inbox += LiveSignalUi(HOST, "offer", "sdp"); c.poll(1)
        c.onFailed(HOST)
        assertEquals(VideoState.NoDirectPath, c.state)
        c.startWatching(2)
        core.inbox += LiveSignalUi(HOST, "bye", "parked"); c.poll(3)
        assertEquals(VideoState.NoSlot, c.state)
    }

    @Test fun parentByeWaitsForReparent() {
        val core = Core(); val peers = Peers()
        val c = LiveVideoController(core, peers)
        c.startWatching(0)
        core.inbox += LiveSignalUi(p(3), "offer", "sdp"); c.poll(1); c.onConnected(p(3))
        core.inbox += LiveSignalUi(p(3), "bye", ""); c.poll(2)
        assertEquals(VideoState.Connecting, c.state)
        core.inbox += LiveSignalUi(p(4), "offer", "sdp2"); c.poll(3); c.onConnected(p(4))
        assertEquals(VideoState.Watching, c.state)
    }

    @Test fun iceBodyRoundTrip() {
        val b = LiveVideoController.encodeIce("video", 1, "candidate:842163049 1 udp 1677729535 198.51.100.7 46154 typ srflx")
        assertEquals(Triple("video", 1, "candidate:842163049 1 udp 1677729535 198.51.100.7 46154 typ srflx"), LiveVideoController.decodeIce(b))
        assertEquals(null, LiveVideoController.decodeIce("bad"))
    }
}

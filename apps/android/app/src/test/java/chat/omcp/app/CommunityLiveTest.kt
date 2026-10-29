package chat.omcp.app

import chat.omcp.app.core.LiveEndedUi
import chat.omcp.app.core.LiveMediaKeyUi
import chat.omcp.app.core.LiveSignalUi
import chat.omcp.app.core.LiveStreamUi
import chat.omcp.app.live.LiveVideoController
import chat.omcp.app.live.RtcPeers
import chat.omcp.app.live.VideoState
import chat.omcp.app.mesh.MeshNotifications
import chat.omcp.app.ui.vm.LiveAudio
import chat.omcp.app.ui.vm.LiveViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CommunityLiveTest {
    private class Audio : LiveAudio {
        val calls = mutableListOf<String>()
        override fun startHosting() { calls += "host" }
        override fun startListening() { calls += "listen" }
        override fun stop() { calls += "stop" }
    }

    private class Core : chat.omcp.app.core.OmcpCore by chat.omcp.app.core.FakeOmcpCore() {
        val streams = mutableListOf<LiveStreamUi>()
        val ended = mutableListOf<LiveEndedUi>()
        val removed = mutableListOf<String>()
        var startedIn: String? = null
        var key: LiveMediaKeyUi? = LiveMediaKeyUi(ByteArray(16), 0)
        override fun liveStartCommunity(communityId: String, title: String, video: Boolean): String? {
            startedIn = communityId
            streams += LiveStreamUi("s1", title, "", mine = true, listening = false, video = video, startedMs = 1, communityId = communityId)
            return "s1"
        }
        override fun liveStreams() = streams.toList()
        override fun liveRemoveListener(contactId: String) { removed += contactId }
        override fun liveTakeEnded(): List<LiveEndedUi> = ended.toList().also { ended.clear() }
        override fun liveMediaKey() = key
        override fun liveTakeSignals() = emptyList<LiveSignalUi>()
    }

    @Test fun goLiveInACommunityAndRemoveAViewer() {
        val core = Core(); val audio = Audio()
        val vm = LiveViewModel(core, audio)
        assertTrue(vm.goLiveCommunity("c1", "", hasInternet = false))
        assertEquals("c1", core.startedIn)
        assertEquals("c1", vm.state.value.hosting?.communityId)
        assertEquals(listOf("host"), audio.calls)
        vm.removeViewer("peer2")
        assertEquals(listOf("peer2"), core.removed)
    }

    @Test fun teamTakedownStopsTheMicAndShowsWhy() {
        val core = Core(); val audio = Audio()
        val vm = LiveViewModel(core, audio)
        vm.goLiveCommunity("c1", "t", hasInternet = false)
        core.streams.clear()
        core.ended += LiveEndedUi("s1", "taken_down", mine = true)
        vm.refresh()
        assertNull(vm.state.value.hosting)
        assertEquals("taken_down", vm.state.value.notice)
        assertEquals("stop", audio.calls.last())
    }

    @Test fun relayingViewerClosesEveryPeerWhenTheStreamIsGone() {
        val core = Core()
        val closed = mutableListOf<String>()
        val peers = object : RtcPeers {
            override fun offerTo(peer: String) {}
            override fun onRemote(peer: String, kind: String, body: String) {}
            override fun close(peer: String) { closed += peer }
            override fun closeAll() { closed += "*" }
            override fun setKey(key: LiveMediaKeyUi) {}
        }
        val ctl = LiveVideoController(core, peers)
        ctl.startWatching(0)
        ctl.poll(1)
        assertEquals(VideoState.Connecting, ctl.state)
        core.key = null // takedown: Rust dropped the stream key
        ctl.poll(2)
        assertEquals(VideoState.Off, ctl.state)
        assertTrue("*" in closed)
    }

    @Test fun liveStartNotificationIsAlwaysGeneric() {
        assertEquals("Hopla" to "New message", MeshNotifications.liveStartedText("Hopla", "New message"))
    }
}

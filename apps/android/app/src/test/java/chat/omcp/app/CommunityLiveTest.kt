package chat.omcp.app

import chat.omcp.app.core.LiveEndedUi
import chat.omcp.app.data.InMemoryAppStore
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

    @Test fun defaultModeShadeShowsHostAndCommunityButLockScreenIsGeneric() {
        val store = InMemoryAppStore()
        val c = chat.omcp.app.notify.NotificationPolicy.liveContent(store, "c1", "Ana", "Choir", "Warm-up", "Hopla", "New message",
            { h, k -> "$h is live in $k" }, { t -> "$t · Tap to listen" })
        assertEquals("Ana is live in Choir", c.title)
        assertEquals("Warm-up · Tap to listen", c.body)
        assertEquals(chat.omcp.app.notify.LockVisibility.PRIVATE, c.visibility)
        assertEquals("Hopla" to "New message", c.publicTitle to c.publicBody)
        assertEquals(androidx.core.app.NotificationCompat.VISIBILITY_PRIVATE, MeshNotifications.lockVisibility(c.visibility))
    }

    @Test fun privateModeLiveNotificationIsGenericEverywhere() {
        val store = InMemoryAppStore().apply { privateNotifications = true }
        val c = chat.omcp.app.notify.NotificationPolicy.liveContent(store, "c1", "Ana", "Choir", "Warm-up", "Hopla", "New message",
            { h, k -> "$h $k" }, { it })
        assertEquals("Hopla" to "New message", c.title to c.body)
        assertEquals(chat.omcp.app.notify.LockVisibility.PRIVATE, c.visibility)
        assertTrue(c.private)
    }

    @Test fun removeFromLiveCountsWhoGetsTheNewKey() {
        val core = Core()
        val vm = LiveViewModel(core, Audio())
        vm.goLiveCommunity("c1", "t", hasInternet = false)
        val members = listOf("me", "a", "b", "c", "d")
        vm.removeFromLive("a", "Ana", members, me = "me")
        assertEquals("Ana" to 3, vm.state.value.removedToast)
        vm.dismissToast()
        vm.removeFromLive("b", "Bo", members, me = "me")
        assertEquals("Bo" to 2, vm.state.value.removedToast)
        assertEquals(listOf("a", "b"), core.removed)
        assertEquals(setOf("a", "b"), vm.state.value.removedViewers)
    }

    @Test fun hostTakedownCarriesTheReasonViewerDoesNot() {
        val core = object : chat.omcp.app.core.OmcpCore by Core() {
            val ended = mutableListOf(LiveEndedUi("s1", "taken_down", mine = true))
            override fun liveTakeEnded(): List<LiveEndedUi> = ended.toList().also { ended.clear() }
            override fun moderationLiveRemovedReason(streamIdHex: String): Int? = 4
        }
        val vm = LiveViewModel(core, Audio())
        vm.refresh()
        assertTrue(vm.state.value.noticeMine)
        assertEquals(4, vm.state.value.noticeReason)
        vm.dismissNotice()
        assertNull(vm.state.value.notice)
    }

    @Test fun streamIdIsGroupedInFoursAndReportCarriesIt() {
        val id = "00112233445566778899AABBCCDDEEFF"
        assertEquals("0011 2233 4455 6677 8899 aabb ccdd eeff", chat.omcp.app.ui.screens.groupStreamId(id))
        assertTrue(chat.omcp.app.ui.screens.liveReportText(id, "Spam").contains("00112233445566778899aabbccddeeff"))
        assertEquals(5, chat.omcp.app.ui.screens.LIVE_REPORT_REASONS.size)
    }

    @Test fun newCommunityLiveStringsExistInEnAndFrAndSayListen() {
        val res = java.io.File("src/main/res")
        fun load(d: String) = Regex("<string name=\"([a-z0-9_]+)\">(.*?)</string>").findAll(java.io.File(res, "$d/strings.xml").readText())
            .associate { it.groupValues[1] to it.groupValues[2] }
        val en = load("values"); val fr = load("values-fr")
        val keys = listOf("live_remove_viewer", "community_counter", "community_founder_only", "community_full_t", "community_full_d",
            "live_rm_t", "live_rm_d", "live_rm_done", "live_notif_title", "live_notif_body", "live_report", "live_report_sid",
            "live_td_host", "live_td_reason", "live_appeal", "live_appeal_d")
        for (k in keys) { assertTrue("en $k", k in en); assertTrue("fr $k", k in fr) }
        assertEquals("Remove from live", en["live_remove_viewer"])
        assertEquals("Retirer du direct", fr["live_remove_viewer"])
        assertTrue(en["community_live_host_note"]!!.contains("listening"))
        assertTrue(en["community_removed_body"]!!.contains("listened to"))
        assertTrue("no watch wording", listOf("community_live_host_note", "community_removed_body").none { en[it]!!.contains("watch") })
        // No anonymity claim on the report screen (provisional, owner decision pending).
        assertTrue(en["live_report_note"]!!.lowercase().let { "know who" !in it && "anonym" !in it })
    }

}

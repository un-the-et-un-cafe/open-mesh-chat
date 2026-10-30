package chat.omcp.app

import chat.omcp.app.core.LiveStatsUi
import chat.omcp.app.core.LiveStreamUi
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.live.OpusFormat
import chat.omcp.app.ui.vm.LiveAudio
import chat.omcp.app.ui.vm.LiveViewModel
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.nio.file.Files

class LiveTest {
    private class FakeAudio : LiveAudio {
        val calls = mutableListOf<String>()
        override fun startHosting() { calls += "host" }
        override fun startListening() { calls += "listen" }
        override fun stop() { calls += "stop" }
    }

    private class LiveCore : chat.omcp.app.core.OmcpCore by chat.omcp.app.core.FakeOmcpCore() {
        var streams = mutableListOf<LiveStreamUi>()
        var started: Triple<String, Boolean, List<String>>? = null
        override fun liveStart(title: String, video: Boolean, contactIds: List<String>): String? {
            started = Triple(title, video, contactIds)
            streams += LiveStreamUi("aa", title, "", mine = true, listening = false, video = video, startedMs = 1)
            return "aa"
        }
        override fun liveStop() { streams.removeAll { it.mine } }
        override fun liveStreams() = streams.toList()
        override fun liveJoin(streamIdHex: String, hasInternet: Boolean): Boolean {
            val i = streams.indexOfFirst { it.streamIdHex == streamIdHex }
            if (i < 0) return false
            streams[i] = streams[i].copy(listening = true); return true
        }
        override fun liveLeave() { streams.replaceAll { it.copy(listening = false) } }
        override fun liveStats() = LiveStatsUi(played = 95, lost = 5, listeners = 2)
    }

    @Test fun goLiveNeedsListenersAndStartsTheMic() {
        val core = LiveCore(); val audio = FakeAudio()
        val vm = LiveViewModel(core, audio)
        assertFalse(vm.goLive("t", emptyList(), hasInternet = false))
        assertTrue(audio.calls.isEmpty())
        assertTrue(vm.goLive("", listOf("c1"), hasInternet = true))
        assertEquals("Live", core.started!!.first)
        assertEquals(LiveViewModel.VIDEO_ENABLED, core.started!!.second) // video only with the build flag
        assertEquals(listOf("host"), audio.calls)
        assertEquals("aa", vm.state.value.hosting?.streamIdHex)
        vm.stopLive()
        assertNull(vm.state.value.hosting)
        assertEquals("stop", audio.calls.last())
    }

    @Test fun listenLeaveAndHostEndingStopsPlayback() {
        val core = LiveCore(); val audio = FakeAudio()
        core.streams += LiveStreamUi("bb", "Town hall", "host1", mine = false, listening = false, video = false, startedMs = 2)
        val vm = LiveViewModel(core, audio)
        vm.refresh()
        assertEquals(1, vm.state.value.streams.size)
        vm.listen("bb", hasInternet = false)
        assertEquals("bb", vm.state.value.listening?.streamIdHex)
        assertEquals(5, LiveViewModel.lossPercent(vm.state.value.stats))
        core.streams.clear() // host ended
        vm.refresh()
        assertNull(vm.state.value.listening)
        assertEquals(listOf("listen", "stop"), audio.calls)
    }

    @Test fun opusHeadIsRfc7845() {
        val h = OpusFormat.opusHead()
        assertEquals(19, h.size)
        assertArrayEquals("OpusHead".toByteArray(), h.copyOfRange(0, 8))
        assertEquals(1, h[8].toInt()); assertEquals(1, h[9].toInt())
        assertEquals(312, (h[10].toInt() and 0xff) or ((h[11].toInt() and 0xff) shl 8))
        assertEquals(960, OpusFormat.FRAME_SAMPLES)
        assertEquals(6_500_000L, OpusFormat.preSkipNs())
    }

    @Test fun realCoreRejectsUnknownListeners() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue(File(dir, "libomcp_ffi.so").exists())
        val core = RustOmcpCore()
        val repo = MeshRepository(core, InMemoryAppStore(), File(Files.createTempDirectory("live").toFile(), "omcp.db").path) { 1L }
        repo.saveIdentity(core.generateRecoveryPhrase(), "N")
        assertNull(core.liveStart("t", false, listOf("00".repeat(32))))
        assertTrue(core.liveStreams().isEmpty())
        assertTrue(core.liveTakeAudio(0).isEmpty())
        assertFalse(core.liveJoin("00".repeat(16), false))
    }
}

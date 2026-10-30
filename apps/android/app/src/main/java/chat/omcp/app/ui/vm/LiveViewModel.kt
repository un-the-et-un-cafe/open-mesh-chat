package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.LiveStatsUi
import chat.omcp.app.core.LiveStreamUi
import chat.omcp.app.core.OmcpCore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Audio driver seam (real one: [chat.omcp.app.live.LiveAudioEngine]). */
interface LiveAudio {
    fun startHosting()
    fun startListening()
    fun stop()
}

/**
 * Live tab (REQUIREMENTS "Live", design §5.12 adapted to the 2026-10-24 decision:
 * offline audio over the mesh, video when online).
 */
class LiveViewModel(
    private val core: OmcpCore,
    private val audio: LiveAudio?,
) : ViewModel() {
    data class State(
        val streams: List<LiveStreamUi> = emptyList(),
        val hosting: LiveStreamUi? = null,
        val listening: LiveStreamUi? = null,
        val stats: LiveStatsUi = LiveStatsUi(),
        val error: String? = null,
    )

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    fun refresh() {
        val all = core.liveStreams()
        _state.value = _state.value.copy(
            streams = all.filter { !it.mine },
            hosting = all.firstOrNull { it.mine },
            listening = all.firstOrNull { it.listening },
            stats = core.liveStats(),
        )
        // The host ended the stream we were listening to.
        if (_state.value.listening == null && wasListening) { audio?.stop(); wasListening = false }
    }

    private var wasListening = false

    /** Call only once RECORD_AUDIO is granted (asked on first use). */
    fun goLive(title: String, contactIds: List<String>, hasInternet: Boolean): Boolean {
        if (contactIds.isEmpty()) { _state.value = _state.value.copy(error = "pick"); return false }
        leave()
        val id = core.liveStart(title.ifBlank { "Live" }, video = hasInternet && VIDEO_ENABLED, contactIds = contactIds)
        if (id == null) { _state.value = _state.value.copy(error = "start"); return false }
        audio?.startHosting()
        refresh()
        return true
    }

    fun stopLive() {
        audio?.stop()
        core.liveStop()
        refresh()
    }

    fun listen(streamIdHex: String, hasInternet: Boolean) {
        if (_state.value.hosting != null) return
        if (core.liveJoin(streamIdHex, hasInternet)) {
            wasListening = true
            audio?.startListening()
        }
        refresh()
    }

    fun leave() {
        if (wasListening) { audio?.stop(); wasListening = false }
        core.liveLeave()
        refresh()
    }

    override fun onCleared() {
        audio?.stop()
    }

    companion object {
        /** WebRTC video is behind the OMCP_LIVE_VIDEO build flag (APK size, docs/LIVE.md). */
        val VIDEO_ENABLED: Boolean get() = chat.omcp.app.BuildConfig.LIVE_VIDEO

        fun lossPercent(s: LiveStatsUi): Int {
            val t = s.played + s.lost
            return if (t == 0L) 0 else ((s.lost * 100) / t).toInt()
        }
    }
}

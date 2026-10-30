package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import chat.omcp.app.live.LiveVideo
import chat.omcp.app.live.VideoState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
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
    /** Online video (WebRTC); null = audio-only build or no WebRTC. */
    private val video: LiveVideo? = null,
) : ViewModel() {
    data class State(
        val streams: List<LiveStreamUi> = emptyList(),
        val hosting: LiveStreamUi? = null,
        val listening: LiveStreamUi? = null,
        val stats: LiveStatsUi = LiveStatsUi(),
        val error: String? = null,
        val video: VideoState = VideoState.Off,
    )

    /** For the renderer (WebRTC builds only). */
    val webRtc: chat.omcp.app.live.WebRtcPeers? get() = (video as? chat.omcp.app.live.WebRtcLiveVideo)?.peers

    private val _state = MutableStateFlow(State())
    val state: StateFlow<State> = _state.asStateFlow()

    fun refresh() {
        val all = core.liveStreams()
        _state.value = _state.value.copy(
            streams = all.filter { !it.mine },
            hosting = all.firstOrNull { it.mine },
            listening = all.firstOrNull { it.listening },
            stats = core.liveStats(),
            video = video?.state ?: VideoState.Off,
        )
        // The host ended the stream we were listening to.
        if (_state.value.listening == null && wasListening) { audio?.stop(); stopVideo(); wasListening = false }
    }

    private var wasListening = false

    /** Call only once RECORD_AUDIO is granted (asked on first use). */
    fun goLive(title: String, contactIds: List<String>, hasInternet: Boolean): Boolean {
        if (contactIds.isEmpty()) { _state.value = _state.value.copy(error = "pick"); return false }
        leave()
        val id = core.liveStart(title.ifBlank { "Live" }, video = hasInternet && VIDEO_ENABLED, contactIds = contactIds)
        if (id == null) { _state.value = _state.value.copy(error = "start"); return false }
        audio?.startHosting()
        if (hasInternet && video != null) startVideo { it.startHosting() }
        refresh()
        return true
    }

    fun stopLive() {
        audio?.stop()
        stopVideo()
        core.liveStop()
        refresh()
    }

    fun listen(streamIdHex: String, hasInternet: Boolean) {
        if (_state.value.hosting != null) return
        if (core.liveJoin(streamIdHex, hasInternet)) {
            wasListening = true
            audio?.startListening()
            val st = core.liveStreams().firstOrNull { it.streamIdHex == streamIdHex }
            if (hasInternet && st?.video == true && video != null) startVideo { it.startWatching() }
        }
        refresh()
    }

    fun leave() {
        if (wasListening) { audio?.stop(); wasListening = false }
        stopVideo()
        core.liveLeave()
        refresh()
    }

    private var videoJob: Job? = null

    private fun startVideo(start: (LiveVideo) -> Unit) {
        val v = video ?: return
        runCatching { start(v) }.onFailure { _state.value = _state.value.copy(error = "video") }
        videoJob?.cancel()
        // Signals (offer/answer/ICE) need a fast poll; the screen refresh is 1 s.
        videoJob = viewModelScope.launch(Dispatchers.Default) {
            while (isActive) {
                runCatching { v.poll() }
                _state.value = _state.value.copy(video = v.state)
                delay(100)
            }
        }
    }

    private fun stopVideo() {
        videoJob?.cancel(); videoJob = null
        video?.stop()
    }

    override fun onCleared() {
        audio?.stop()
        stopVideo()
    }

    companion object {
        /** WebRTC video ships in every build (docs/LIVE.md). */
        val VIDEO_ENABLED: Boolean get() = chat.omcp.app.BuildConfig.LIVE_VIDEO

        fun lossPercent(s: LiveStatsUi): Int {
            val t = s.played + s.lost
            return if (t == 0L) 0 else ((s.lost * 100) / t).toInt()
        }
    }
}

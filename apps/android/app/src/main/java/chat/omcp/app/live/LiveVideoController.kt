package chat.omcp.app.live

import chat.omcp.app.core.LiveMediaKeyUi
import chat.omcp.app.core.OmcpCore

/**
 * WebRTC seam (real one: [WebRtcPeers]). One peer connection per tree neighbour:
 * the parent (upstream, we answer) and up to 4 children (6 for the host; we offer).
 */
interface RtcPeers {
    /** Open a connection to child [peer] and send it an offer (we forward our video). */
    fun offerTo(peer: String)
    /** Remote offer/answer/ICE from [peer]; an offer opens the upstream connection. */
    fun onRemote(peer: String, kind: String, body: String)
    fun close(peer: String)
    fun closeAll()
    /** Media key for every sender/receiver frame cryptor (rotates with the stream key). */
    fun setKey(key: LiveMediaKeyUi)
}

/** What the Live screen shows about video. */
enum class VideoState { Off, Connecting, Watching, Hosting, NoDirectPath, NoSlot }

/**
 * Online video for Live (docs/LIVE.md): the host's tree lives in Rust; this class
 * turns its 0x50 signals into peer connections and decides when to fall back.
 * Audio always keeps flowing over the mesh, so a fallback only loses the picture.
 */
class LiveVideoController(
    private val core: OmcpCore,
    private val peers: RtcPeers,
    private val connectTimeoutMs: Long = CONNECT_TIMEOUT_MS,
) {
    @Volatile var state: VideoState = VideoState.Off
        private set
    private var isHost = false
    private var since = 0L
    private var key: LiveMediaKeyUi? = null
    private var parent: String? = null
    val children = linkedSetOf<String>()
    private val maxChildren get() = if (isHost) HOST_FANOUT else VIEWER_FANOUT

    @Synchronized fun startHosting(now: Long) {
        stop(); isHost = true; state = VideoState.Hosting; since = now; refreshKey()
    }

    @Synchronized fun startWatching(now: Long) {
        stop(); isHost = false; state = VideoState.Connecting; since = now; refreshKey()
        core.liveVideoJoin()
    }

    @Synchronized fun stop() {
        if (state == VideoState.Off) return
        peers.closeAll(); children.clear(); parent = null; key = null; state = VideoState.Off
    }

    private fun refreshKey() {
        val k = core.liveMediaKey() ?: return
        if (k != key) { key = k; peers.setKey(k) }
    }

    /** Call every ~50–100 ms with the audio loop. */
    @Synchronized fun poll(now: Long) {
        if (state == VideoState.Off) return
        // No key = the stream ended or was taken down by the team: close every
        // connection, so we neither play nor forward video to our children.
        if (core.liveMediaKey() == null && key != null) {
            peers.closeAll(); children.clear(); parent = null; key = null; state = VideoState.Off
            return
        }
        refreshKey()
        for (s in core.liveTakeSignals()) when (s.kind) {
            "assign" -> if (s.peerHex !in children && children.size < maxChildren) {
                children += s.peerHex; peers.offerTo(s.peerHex)
            }
            "offer" -> if (!isHost) { parent = s.peerHex; peers.onRemote(s.peerHex, s.kind, s.body) }
            "answer", "ice" -> if (s.peerHex == parent || s.peerHex in children) peers.onRemote(s.peerHex, s.kind, s.body)
            "bye" -> when {
                s.body == PARKED -> { fallBack(VideoState.NoSlot) }
                s.peerHex == parent -> { peers.close(s.peerHex); parent = null; state = VideoState.Connecting; since = now }
                else -> { children -= s.peerHex; peers.close(s.peerHex) }
            }
        }
        if (state == VideoState.Connecting && now - since > connectTimeoutMs) fallBack(VideoState.NoDirectPath)
    }

    /** From [RtcPeers]: ICE connected. */
    @Synchronized fun onConnected(peer: String) {
        if (peer == parent && state == VideoState.Connecting) state = VideoState.Watching
    }

    /** From [RtcPeers]: ICE failed (no direct path; no TURN by design). */
    @Synchronized fun onFailed(peer: String) {
        peers.close(peer)
        if (peer == parent) fallBack(VideoState.NoDirectPath)
        else if (children.remove(peer) && isHost) core.liveVideoDrop(peer)
    }

    private fun fallBack(s: VideoState) {
        // Leave the tree so the host re-parents our children; audio stays on the mesh.
        if (!isHost) core.liveSendSignal(HOST, "bye", "")
        peers.closeAll(); children.clear(); parent = null; state = s
    }

    companion object {
        const val HOST = "0000000000000000000000000000000000000000000000000000000000000000"
        const val PARKED = "parked"
        const val HOST_FANOUT = 6
        const val VIEWER_FANOUT = 4
        /** No connection after this long = no direct path; keep audio only. */
        const val CONNECT_TIMEOUT_MS = 20_000L

        /** ICE body over 0x50: "mid\nindex\ncandidate". */
        fun encodeIce(mid: String?, index: Int, candidate: String) = "${mid.orEmpty()}\n$index\n$candidate"
        fun decodeIce(body: String): Triple<String, Int, String>? {
            val p = body.split('\n', limit = 3)
            if (p.size != 3) return null
            return Triple(p[0], p[1].toIntOrNull() ?: return null, p[2])
        }
    }
}

/** Live screen seam for video (real: [WebRtcLiveVideo]; null when video is off). */
interface LiveVideo {
    val state: VideoState
    fun startHosting()
    fun startWatching()
    fun stop()
    fun poll()
}

/** Glues [LiveVideoController] to [WebRtcPeers]. */
class WebRtcLiveVideo(val peers: WebRtcPeers, core: OmcpCore) : LiveVideo {
    private val ctl = LiveVideoController(core, peers).also { peers.controller = it }
    override val state get() = ctl.state
    override fun startHosting() { ctl.startHosting(System.currentTimeMillis()); peers.startCamera() }
    override fun startWatching() = ctl.startWatching(System.currentTimeMillis())
    override fun stop() = ctl.stop()
    override fun poll() = ctl.poll(System.currentTimeMillis())
}

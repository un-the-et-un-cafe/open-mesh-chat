package chat.omcp.app.live

import android.content.Context
import chat.omcp.app.core.LiveMediaKeyUi
import chat.omcp.app.core.OmcpCore
import org.webrtc.Camera2Enumerator
import org.webrtc.CameraVideoCapturer
import org.webrtc.DataChannel
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.DefaultVideoEncoderFactory
import org.webrtc.EglBase
import org.webrtc.FrameCryptor
import org.webrtc.FrameCryptorAlgorithm
import org.webrtc.FrameCryptorFactory
import org.webrtc.FrameCryptorKeyDerivationAlgorithm
import org.webrtc.FrameCryptorKeyProvider
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpReceiver
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import org.webrtc.SurfaceTextureHelper
import org.webrtc.VideoSink
import org.webrtc.VideoSource
import org.webrtc.VideoTrack

/**
 * Real WebRTC for Live video: STUN only (no TURN), one connection per tree neighbour,
 * every encoded frame end-to-end encrypted by the WebRTC frame cryptor (AES-GCM) with
 * the SFrame base key derived in Rust from the stream key (docs/LIVE.md › Video).
 * The host sends its camera; a viewer shows its parent's track and forwards the same
 * track to its children.
 */
class WebRtcPeers(
    context: Context,
    private val core: OmcpCore,
) : RtcPeers {
    var controller: LiveVideoController? = null
    val egl: EglBase = EglBase.create()
    private val app = context.applicationContext
    private val factory: PeerConnectionFactory
    private val keys: FrameCryptorKeyProvider
    private var keyIndex = 0
    private val pcs = HashMap<String, PeerConnection>()
    private val cryptors = HashMap<String, MutableList<FrameCryptor>>()
    private val pendingOffers = LinkedHashSet<String>()
    private var capturer: CameraVideoCapturer? = null
    private var source: VideoSource? = null
    private var surface: SurfaceTextureHelper? = null
    /** Camera (host) or the parent's track (viewer): what we show and forward. */
    @Volatile var track: VideoTrack? = null
        private set
    private var sink: VideoSink? = null
    private val callbacks = java.util.concurrent.Executors.newSingleThreadExecutor { r -> Thread(r, "live-rtc-cb").apply { isDaemon = true } }

    init {
        ensureInit(app)
        factory = PeerConnectionFactory.builder()
            .setVideoEncoderFactory(DefaultVideoEncoderFactory(egl.eglBaseContext, true, true))
            .setVideoDecoderFactory(DefaultVideoDecoderFactory(egl.eglBaseContext))
            .createPeerConnectionFactory()
        keys = FrameCryptorFactory.createFrameCryptorKeyProvider(
            // PBKDF2 frame-key derivation, same as iOS (WebRtcPeers.swift).
            true, RATCHET_SALT, 0, MAGIC, -1, KEY_RING, true, FrameCryptorKeyDerivationAlgorithm.PBKDF2,
        )
    }

    /** Renderer for the Live screen (null to detach). */
    @Synchronized fun setSink(s: VideoSink?) {
        sink?.let { track?.removeSink(it) }
        sink = s
        s?.let { track?.addSink(it) }
    }

    /** Host: start the front camera (CAMERA must be granted). */
    @Synchronized fun startCamera() {
        if (capturer != null) return
        val en = Camera2Enumerator(app)
        val name = en.deviceNames.firstOrNull { en.isFrontFacing(it) } ?: en.deviceNames.firstOrNull() ?: return
        val cap = en.createCapturer(name, null)
        val src = factory.createVideoSource(false)
        surface = SurfaceTextureHelper.create("live-cam", egl.eglBaseContext)
        cap.initialize(surface, app, src.capturerObserver)
        cap.startCapture(640, 480, 24)
        capturer = cap; source = src
        setTrack(factory.createVideoTrack("omcp-v", src))
    }

    private fun setTrack(t: VideoTrack) {
        track = t
        sink?.let { t.addSink(it) }
        val queued = pendingOffers.toList(); pendingOffers.clear()
        queued.forEach { offerTo(it) }
    }

    @Synchronized override fun setKey(key: LiveMediaKeyUi) {
        keyIndex = (key.kid % KEY_RING).toInt()
        keys.setSharedKey(keyIndex, key.baseKey)
        cryptors.values.flatten().forEach { it.setKeyIndex(keyIndex) }
    }

    private fun newPc(peer: String): PeerConnection? {
        val ice = core.liveStunServers().map { PeerConnection.IceServer.builder(it).createIceServer() }
        val cfg = PeerConnection.RTCConfiguration(ice).apply {
            sdpSemantics = PeerConnection.SdpSemantics.UNIFIED_PLAN
            continualGatheringPolicy = PeerConnection.ContinualGatheringPolicy.GATHER_ONCE
            iceTransportsType = PeerConnection.IceTransportsType.ALL
        }
        val pc = factory.createPeerConnection(cfg, object : PeerConnection.Observer {
            override fun onIceCandidate(c: IceCandidate) {
                core.liveSendSignal(peer, "ice", LiveVideoController.encodeIce(c.sdpMid, c.sdpMLineIndex, c.sdp))
            }
            override fun onIceConnectionChange(s: PeerConnection.IceConnectionState) {
                // Hop off the WebRTC signalling thread: the controller may close connections.
                when (s) {
                    PeerConnection.IceConnectionState.CONNECTED, PeerConnection.IceConnectionState.COMPLETED -> callbacks.execute { controller?.onConnected(peer) }
                    PeerConnection.IceConnectionState.FAILED -> callbacks.execute { controller?.onFailed(peer) }
                    else -> {}
                }
            }
            override fun onTrack(t: RtpTransceiver) {
                val r = t.receiver
                encrypt(peer, FrameCryptorFactory.createFrameCryptorForRtpReceiver(factory, r, PARTICIPANT, FrameCryptorAlgorithm.AES_GCM, keys))
                (r.track() as? VideoTrack)?.let { v -> callbacks.execute { synchronized(this@WebRtcPeers) { setTrack(v) } } }
            }
            override fun onSignalingChange(s: PeerConnection.SignalingState) {}
            override fun onIceConnectionReceivingChange(b: Boolean) {}
            override fun onIceGatheringChange(s: PeerConnection.IceGatheringState) {}
            override fun onIceCandidatesRemoved(c: Array<out IceCandidate>) {}
            override fun onAddStream(s: MediaStream) {}
            override fun onRemoveStream(s: MediaStream) {}
            override fun onDataChannel(d: DataChannel) {}
            override fun onRenegotiationNeeded() {}
            override fun onAddTrack(r: RtpReceiver, s: Array<out MediaStream>) {}
        }) ?: return null
        pcs[peer] = pc
        return pc
    }

    private fun encrypt(peer: String, fc: FrameCryptor) {
        fc.setKeyIndex(keyIndex); fc.setEnabled(true)
        callbacks.execute { synchronized(this) { cryptors.getOrPut(peer) { mutableListOf() } += fc } }
    }

    @Synchronized override fun offerTo(peer: String) {
        val t = track ?: run { pendingOffers += peer; return }
        if (pcs.containsKey(peer)) return
        val pc = newPc(peer) ?: return
        val sender = pc.addTrack(t, listOf("omcp-live"))
        encrypt(peer, FrameCryptorFactory.createFrameCryptorForRtpSender(factory, sender, PARTICIPANT, FrameCryptorAlgorithm.AES_GCM, keys))
        val c = MediaConstraints().apply { mandatory += MediaConstraints.KeyValuePair("OfferToReceiveVideo", "false") }
        pc.createOffer(sdp { d -> pc.setLocalDescription(sdp {}, d); core.liveSendSignal(peer, "offer", d.description) }, c)
    }

    @Synchronized override fun onRemote(peer: String, kind: String, body: String) {
        when (kind) {
            "offer" -> {
                pcs.remove(peer)?.dispose()
                val pc = newPc(peer) ?: return
                pc.setRemoteDescription(sdp {
                    pc.createAnswer(sdp { d -> pc.setLocalDescription(sdp {}, d); core.liveSendSignal(peer, "answer", d.description) }, MediaConstraints())
                }, SessionDescription(SessionDescription.Type.OFFER, body))
            }
            "answer" -> pcs[peer]?.setRemoteDescription(sdp {}, SessionDescription(SessionDescription.Type.ANSWER, body))
            "ice" -> LiveVideoController.decodeIce(body)?.let { (mid, i, c) -> pcs[peer]?.addIceCandidate(IceCandidate(mid, i, c)) }
        }
    }

    @Synchronized override fun close(peer: String) {
        pendingOffers -= peer
        cryptors.remove(peer)?.forEach { runCatching { it.dispose() } }
        pcs.remove(peer)?.let { runCatching { it.dispose() } }
    }

    @Synchronized override fun closeAll() {
        pcs.keys.toList().forEach { close(it) }
        pendingOffers.clear()
        sink?.let { track?.removeSink(it) }
        runCatching { capturer?.stopCapture() }; capturer?.dispose(); capturer = null
        if (source != null) runCatching { track?.dispose() }
        source?.dispose(); source = null
        surface?.dispose(); surface = null
        track = null
    }

    private fun sdp(onOk: (SessionDescription) -> Unit) = object : SdpObserver {
        override fun onCreateSuccess(d: SessionDescription) = onOk(d)
        override fun onSetSuccess() = onOk(SessionDescription(SessionDescription.Type.OFFER, ""))
        override fun onCreateFailure(e: String?) {}
        override fun onSetFailure(e: String?) {}
    }

    companion object {
        private const val PARTICIPANT = "omcp-live"
        private const val KEY_RING = 16
        /** Shared with iOS so both frame cryptors derive the same frame key. */
        val RATCHET_SALT = "OMCP-live-frame-v1".toByteArray()
        val MAGIC = "OMCP".toByteArray()
        @Volatile private var inited = false
        fun ensureInit(ctx: Context) {
            if (inited) return
            synchronized(this) {
                if (!inited) {
                    PeerConnectionFactory.initialize(PeerConnectionFactory.InitializationOptions.builder(ctx).createInitializationOptions())
                    inited = true
                }
            }
        }
    }
}

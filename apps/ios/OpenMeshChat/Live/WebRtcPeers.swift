import Foundation
import AVFoundation
import HoplaKit
import WebRTC

/// Real WebRTC for Live video (iOS side of Android `WebRtcPeers`): STUN only, one peer
/// connection per tree neighbour, frames end-to-end encrypted by the WebRTC frame cryptor
/// (AES-GCM) with the SFrame base key from Rust. Same salt/magic/key ring as Android so the
/// two platforms decrypt each other's frames.
final class WebRtcPeers: NSObject, RtcPeers {
    static let ratchetSalt = Data("OMCP-live-frame-v1".utf8)
    static let magic = Data("OMCP".utf8)
    static let keyRing: Int32 = 16
    static let participant = "omcp-live"

    weak var controller: LiveVideoController?
    private let core: OmcpCore
    private let factory: RTCPeerConnectionFactory
    private let keys: RTCFrameCryptorKeyProvider
    private var keyIndex: Int32 = 0
    private var pcs: [String: RTCPeerConnection] = [:]
    private var cryptors: [String: [RTCFrameCryptor]] = [:]
    private var pending: [String] = []
    private var capturer: RTCCameraVideoCapturer?
    private var source: RTCVideoSource?
    private let q = DispatchQueue(label: "omcp.live.rtc")
    /// Camera (host) or the parent's track (viewer): shown and forwarded to children.
    private(set) var track: RTCVideoTrack?
    private weak var renderer: RTCVideoRenderer?

    init(core: OmcpCore) {
        self.core = core
        RTCInitializeSSL()
        factory = RTCPeerConnectionFactory(encoderFactory: RTCDefaultVideoEncoderFactory(),
                                           decoderFactory: RTCDefaultVideoDecoderFactory())
        keys = RTCFrameCryptorKeyProvider(ratchetSalt: Self.ratchetSalt, ratchetWindowSize: 0, sharedKeyMode: true,
                                          uncryptedMagicBytes: Self.magic, failureTolerance: -1, keyRingSize: Self.keyRing,
                                          discardFrameWhenCryptorNotReady: true, keyDerivationAlgorithm: .HKDF)
        super.init()
    }

    func attach(_ r: RTCVideoRenderer?) {
        q.async {
            if let old = self.renderer { self.track?.remove(old) }
            self.renderer = r
            if let r { self.track?.add(r) }
        }
    }

    /// Host: front camera (camera permission asked on first video Go live).
    func startCamera() {
        q.async {
            guard self.capturer == nil else { return }
            let src = self.factory.videoSource()
            let cap = RTCCameraVideoCapturer(delegate: src)
            let devs = RTCCameraVideoCapturer.captureDevices()
            guard let dev = devs.first(where: { $0.position == .front }) ?? devs.first else { return }
            let formats = RTCCameraVideoCapturer.supportedFormats(for: dev)
            let fmt = formats.first { CMVideoFormatDescriptionGetDimensions($0.formatDescription).width >= 640 } ?? formats.last
            guard let fmt else { return }
            cap.startCapture(with: dev, format: fmt, fps: 24)
            self.capturer = cap; self.source = src
            self.setTrack(self.factory.videoTrack(with: src, trackId: "omcp-v"))
        }
    }

    private func setTrack(_ t: RTCVideoTrack) {
        track = t
        if let r = renderer { t.add(r) }
        let queued = pending; pending = []
        queued.forEach { offerNow($0) }
    }

    func setKey(_ key: LiveMediaKeyModel) {
        q.async {
            self.keyIndex = Int32(key.kid % UInt64(Self.keyRing))
            self.keys.setSharedKey(key.baseKey, with: self.keyIndex)
            self.cryptors.values.flatMap { $0 }.forEach { $0.keyIndex = self.keyIndex }
        }
    }

    private func newPc(_ peer: String) -> RTCPeerConnection? {
        let cfg = RTCConfiguration()
        cfg.iceServers = [RTCIceServer(urlStrings: core.liveStunServers())]
        cfg.sdpSemantics = .unifiedPlan
        cfg.continualGatheringPolicy = .gatherOnce
        let c = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        let d = PcDelegate(peer: peer, owner: self)
        guard let pc = factory.peerConnection(with: cfg, constraints: c, delegate: d) else { return nil }
        delegates[peer] = d
        pcs[peer] = pc
        return pc
    }
    private var delegates: [String: PcDelegate] = [:]

    private func add(_ peer: String, _ fc: RTCFrameCryptor?) {
        guard let fc else { return }
        fc.keyIndex = keyIndex; fc.enabled = true
        cryptors[peer, default: []].append(fc)
    }

    func offerTo(_ peer: String) { q.async { self.offerNow(peer) } }

    private func offerNow(_ peer: String) {
        guard let t = track else { if !pending.contains(peer) { pending.append(peer) }; return }
        guard pcs[peer] == nil, let pc = newPc(peer) else { return }
        if let sender = pc.add(t, streamIds: ["omcp-live"]) {
            add(peer, RTCFrameCryptor(factory: factory, rtpSender: sender, participantId: Self.participant,
                                      algorithm: .aesGcm, keyProvider: keys))
        }
        let c = RTCMediaConstraints(mandatoryConstraints: ["OfferToReceiveVideo": "false"], optionalConstraints: nil)
        pc.offer(for: c) { [weak self] sdp, _ in
            guard let self, let sdp else { return }
            pc.setLocalDescription(sdp) { _ in }
            self.core.liveSendSignal(peerHex: peer, kind: "offer", body: sdp.sdp)
        }
    }

    func onRemote(_ peer: String, kind: String, body: String) {
        q.async {
            switch kind {
            case "offer":
                self.closeNow(peer)
                guard let pc = self.newPc(peer) else { return }
                pc.setRemoteDescription(RTCSessionDescription(type: .offer, sdp: body)) { [weak self] err in
                    guard let self, err == nil else { return }
                    pc.answer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)) { sdp, _ in
                        guard let sdp else { return }
                        pc.setLocalDescription(sdp) { _ in }
                        self.core.liveSendSignal(peerHex: peer, kind: "answer", body: sdp.sdp)
                    }
                }
            case "answer":
                self.pcs[peer]?.setRemoteDescription(RTCSessionDescription(type: .answer, sdp: body)) { _ in }
            case "ice":
                if let ice = LiveVideoController.decodeIce(body) {
                    self.pcs[peer]?.add(RTCIceCandidate(sdp: ice.candidate, sdpMLineIndex: ice.index, sdpMid: ice.mid)) { _ in }
                }
            default: break
            }
        }
    }

    fileprivate func onTrack(_ peer: String, _ receiver: RTCRtpReceiver) {
        q.async {
            self.add(peer, RTCFrameCryptor(factory: self.factory, rtpReceiver: receiver, participantId: Self.participant,
                                           algorithm: .aesGcm, keyProvider: self.keys))
            if let v = receiver.track as? RTCVideoTrack { self.setTrack(v) }
        }
    }

    func close(_ peer: String) { q.async { self.closeNow(peer) } }

    private func closeNow(_ peer: String) {
        pending.removeAll { $0 == peer }
        cryptors[peer] = nil
        pcs.removeValue(forKey: peer)?.close()
        delegates[peer] = nil
    }

    func closeAll() {
        q.async {
            Array(self.pcs.keys).forEach { self.closeNow($0) }
            self.pending = []
            if let r = self.renderer { self.track?.remove(r) }
            self.capturer?.stopCapture(); self.capturer = nil; self.source = nil
            self.track = nil
        }
    }

    /// Delegate callbacks hop off the WebRTC thread before touching the controller.
    fileprivate final class PcDelegate: NSObject, RTCPeerConnectionDelegate {
        let peer: String
        weak var owner: WebRtcPeers?
        init(peer: String, owner: WebRtcPeers) { self.peer = peer; self.owner = owner }
        func peerConnection(_ pc: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}
        func peerConnection(_ pc: RTCPeerConnection, didAdd stream: RTCMediaStream) {}
        func peerConnection(_ pc: RTCPeerConnection, didRemove stream: RTCMediaStream) {}
        func peerConnectionShouldNegotiate(_ pc: RTCPeerConnection) {}
        func peerConnection(_ pc: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
            let p = peer
            DispatchQueue.main.async { [weak owner] in
                switch newState {
                case .connected, .completed: owner?.controller?.onConnected(p)
                case .failed: owner?.controller?.onFailed(p)
                default: break
                }
            }
        }
        func peerConnection(_ pc: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}
        func peerConnection(_ pc: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {
            owner?.core.liveSendSignal(peerHex: peer, kind: "ice",
                                       body: LiveVideoController.encodeIce(mid: candidate.sdpMid, index: candidate.sdpMLineIndex, candidate: candidate.sdp))
        }
        func peerConnection(_ pc: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}
        func peerConnection(_ pc: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {}
        func peerConnection(_ pc: RTCPeerConnection, didAdd rtpReceiver: RTCRtpReceiver, streams mediaStreams: [RTCMediaStream]) {
            owner?.onTrack(peer, rtpReceiver)
        }
    }
}

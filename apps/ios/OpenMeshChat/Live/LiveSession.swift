import Foundation
import SwiftUI
import UIKit
import HoplaKit
import WebRTC

/// Owns Live audio (mesh) and video (WebRTC tree) for the Live tab.
final class LiveSession: ObservableObject {
    @Published private(set) var video: VideoState = .off
    let peers: WebRtcPeers
    private let core: OmcpCore
    private let audio: LiveAudioIO
    private let controller: LiveVideoController
    private var timer: Timer?

    init(core: OmcpCore, kick: MeshKick?) {
        self.core = core
        audio = LiveAudioIO(core: core, kick: kick)
        peers = WebRtcPeers(core: core)
        controller = LiveVideoController(core: core, peers: peers)
        peers.controller = controller
    }

    func startHosting(video withVideo: Bool) {
        audio.startHosting()
        if withVideo {
            controller.startHosting(now: Self.now())
            peers.startCamera()
            startPoll()
        }
    }

    func startListening(video withVideo: Bool) {
        audio.startListening()
        if withVideo {
            controller.startWatching(now: Self.now())
            startPoll()
        }
    }

    func stop() {
        timer?.invalidate(); timer = nil
        audio.stop()
        controller.stop()
        video = .off
    }

    private func startPoll() {
        timer?.invalidate()
        // Signals (offer/answer/ICE) need a fast poll; the screen reload is 1 s.
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            guard let self else { return }
            self.controller.poll(now: Self.now())
            if self.video != self.controller.state { self.video = self.controller.state }
        }
    }

    static func now() -> UInt64 { UInt64(Date().timeIntervalSince1970 * 1000) }
}

/// Renders the host camera or the parent's track.
struct LiveVideoView: UIViewRepresentable {
    let peers: WebRtcPeers
    func makeUIView(context: Context) -> RTCMTLVideoView {
        let v = RTCMTLVideoView(frame: .zero)
        v.videoContentMode = .scaleAspectFit
        peers.attach(v)
        return v
    }
    func updateUIView(_ uiView: RTCMTLVideoView, context: Context) {}
    static func dismantleUIView(_ uiView: RTCMTLVideoView, coordinator: ()) {}
}

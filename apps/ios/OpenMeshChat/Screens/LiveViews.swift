import SwiftUI
import AVFoundation
import Network
import HoplaKit

/// Live tab (REQUIREMENTS "Live"): who's live, listen, or go live. Mirrors Android.
/// Audio goes over the mesh (LiveAudioIO); with internet, video goes peer to peer in the
/// viewer tree (WebRtcPeers, STUN only, E2EE frames). Keys and the tree live in Rust.
struct LiveTabView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var streams: [LiveStreamModel] = []
    @State private var stats = LiveStatsModel()
    @State private var title = ""
    @State private var picked: Set<String> = []
    @State private var micDenied = false
    @State private var online = false
    @State private var session: LiveSession?
    @State private var videoState: VideoState = .off
    private let monitor = NWPathMonitor()

    var body: some View {
        let split = LiveRules.split(streams)
        List {
            Section {
                Text(L(online ? "live_online_callout" : "live_offline_callout"))
                    .font(.footnote).foregroundStyle(c.ink2)
            }
            if let mine = split.mine {
                Section {
                    Text(L("live_badge")).font(.caption).foregroundStyle(c.danger)
                    Text(mine.title).font(.headline)
                    if let session, videoState == .hosting {
                        LiveVideoView(peers: session.peers).aspectRatio(4.0 / 3.0, contentMode: .fit)
                    }
                    Text(String(format: L("live_you_are_live"), Int(stats.listeners))).foregroundStyle(c.ink2)
                    Button(L("live_stop")) { session?.stop(); model.core.liveStop(); reload() }
                }
            }
            Section(L("live_happening")) {
                if split.others.isEmpty {
                    Text(L("live_empty")).foregroundStyle(c.ink3)
                }
                ForEach(split.others) { s in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(L("live_badge")).font(.caption).foregroundStyle(c.danger)
                            Text(L(s.video && online ? "live_video_badge" : "live_audio_badge")).font(.caption).foregroundStyle(c.ink2)
                        }
                        Text(s.title).font(.headline)
                        Text(hostName(s)).foregroundStyle(c.ink2)
                        if s.listening {
                            if let session, videoState == .watching {
                                LiveVideoView(peers: session.peers).aspectRatio(4.0 / 3.0, contentMode: .fit)
                            }
                            if let key = LiveVideoController.messageKey(videoState) {
                                Text(L(key)).font(.footnote).foregroundStyle(c.ink2)
                            }
                            Text(String(format: L("live_listening"), stats.lossPercent)).foregroundStyle(c.ink2)
                            Button(L("live_leave")) { session?.stop(); model.core.liveLeave(); reload() }
                        } else {
                            Button(L("live_join")) {
                                if model.core.liveJoin(streamIdHex: s.streamIdHex, hasInternet: online) {
                                    liveSession().startListening(video: online && s.video)
                                }
                                reload()
                            }.disabled(split.mine != nil)
                        }
                    }
                }
            }
            if split.mine == nil {
                Section(L("live_start_title")) {
                    Text(L("live_start_body")).font(.footnote).foregroundStyle(c.ink2)
                    TextField(L("live_title_hint"), text: $title)
                    ForEach(model.core.contacts()) { ct in
                        Toggle(ct.displayName, isOn: Binding(
                            get: { picked.contains(ct.id) },
                            set: { on in if on { picked.insert(ct.id) } else { picked.remove(ct.id) } }
                        ))
                    }
                    if micDenied { Text(L("live_mic_denied")).foregroundStyle(c.danger) }
                    Button(L("live_go")) { goLive() }
                        .disabled(!LiveRules.canGoLive(picked: picked, hosting: split.mine != nil))
                }
            }
        }
        .navigationTitle(L("tab_live"))
        .task {
            monitor.pathUpdateHandler = { p in
                let ok = p.status == .satisfied
                DispatchQueue.main.async { online = ok }
            }
            monitor.start(queue: .global(qos: .utility))
            while !Task.isCancelled {
                reload()
                try? await Task.sleep(nanoseconds: 1_000_000_000)
            }
        }
        .onDisappear { monitor.cancel() }
    }

    private func hostName(_ s: LiveStreamModel) -> String {
        model.core.contacts().first { $0.id == s.hostContactId }?.displayName
            ?? "omcp1:\(s.hostContactId.prefix(4))…"
    }

    private func reload() {
        // Team takedown: stop the mic and camera, close every WebRTC peer.
        if !model.core.liveTakeEnded().isEmpty { session?.stop() }
        streams = model.core.liveStreams()
        stats = model.core.liveStats()
        // The host ended the stream we were in.
        if session != nil && !streams.contains(where: { $0.listening || $0.mine }) { session?.stop() }
        videoState = session?.video ?? .off
    }

    private func liveSession() -> LiveSession {
        if let session { return session }
        let s = LiveSession(core: model.core, kick: model.repo.kick)
        session = s
        return s
    }

    /// Microphone: asked the first time the user goes live, never at start-up.
    private func goLive() {
        let session = AVAudioSession.sharedInstance()
        switch session.recordPermission {
        case .granted:
            withCamera { start(video: $0) }
        case .denied:
            micDenied = true
        default:
            session.requestRecordPermission { ok in
                DispatchQueue.main.async {
                    micDenied = !ok
                    if ok { withCamera { start(video: $0) } }
                }
            }
        }
    }

    /// Camera: asked the first time the user goes live with internet; denied = audio only.
    private func withCamera(_ then: @escaping (Bool) -> Void) {
        guard online else { then(false); return }
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: then(true)
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { ok in DispatchQueue.main.async { then(ok) } }
        default: then(false)
        }
    }

    private func start(video: Bool) {
        let t = title.trimmingCharacters(in: .whitespaces)
        if model.core.liveStart(title: t.isEmpty ? "Live" : t, video: video, contactIds: Array(picked)) != nil {
            liveSession().startHosting(video: video)
        }
        reload()
    }
}

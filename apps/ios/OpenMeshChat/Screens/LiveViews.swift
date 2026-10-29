import SwiftUI
import AVFoundation
import Network
import HoplaKit

/// Live tab (REQUIREMENTS "Live"): who's live, listen, or go live. Mirrors Android.
/// Audio capture/playback on iOS is not wired yet (docs/LIVE.md); stream control, keys and
/// mesh forwarding run in the shared Rust core.
struct LiveTabView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var streams: [LiveStreamModel] = []
    @State private var stats = LiveStatsModel()
    @State private var title = ""
    @State private var picked: Set<String> = []
    @State private var micDenied = false
    @State private var online = false
    private let monitor = NWPathMonitor()

    var body: some View {
        let split = LiveRules.split(streams)
        List {
            Section {
                Text(L(online ? "live_online_callout" : "live_offline_callout"))
                    .font(.footnote).foregroundStyle(c.ink2)
                Text(L("live_ios_audio_soon")).font(.footnote).foregroundStyle(c.ink3)
            }
            if let mine = split.mine {
                Section {
                    Text(L("live_badge")).font(.caption).foregroundStyle(c.danger)
                    Text(mine.title).font(.headline)
                    Text(String(format: L("live_you_are_live"), Int(stats.listeners))).foregroundStyle(c.ink2)
                    Button(L("live_stop")) { model.core.liveStop(); reload() }
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
                            Text(L("live_audio_badge")).font(.caption).foregroundStyle(c.ink2)
                        }
                        Text(s.title).font(.headline)
                        Text(hostName(s)).foregroundStyle(c.ink2)
                        if s.listening {
                            Text(String(format: L("live_listening"), stats.lossPercent)).foregroundStyle(c.ink2)
                            Button(L("live_leave")) { model.core.liveLeave(); reload() }
                        } else {
                            Button(L("live_join")) {
                                _ = model.core.liveJoin(streamIdHex: s.streamIdHex, hasInternet: online); reload()
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
        streams = model.core.liveStreams()
        stats = model.core.liveStats()
    }

    /// Microphone: asked the first time the user goes live, never at start-up.
    private func goLive() {
        let session = AVAudioSession.sharedInstance()
        switch session.recordPermission {
        case .granted:
            start()
        case .denied:
            micDenied = true
        default:
            session.requestRecordPermission { ok in
                DispatchQueue.main.async {
                    micDenied = !ok
                    if ok { start() }
                }
            }
        }
    }

    private func start() {
        let t = title.trimmingCharacters(in: .whitespaces)
        _ = model.core.liveStart(title: t.isEmpty ? "Live" : t, video: false, contactIds: Array(picked))
        reload()
    }
}

private func L(_ key: String) -> String { NSLocalizedString(key, comment: "") }

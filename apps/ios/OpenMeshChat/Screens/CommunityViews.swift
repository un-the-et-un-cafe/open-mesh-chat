import SwiftUI
import AVFoundation
import Network
import HoplaKit

/// Communities tab: Live entry on top (§5.107), invites, your communities, create one.
/// Mirrors Android `CommunitiesHomeScreen`.
struct CommunitiesHomeView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var list: [CommunityModel] = []
    @State private var invites: [CommunityInviteModel] = []
    @State private var name = ""

    var body: some View {
        NavigationStack {
            List {
                Section { NavigationLink(L("live_entry")) { LiveTabView() } }
                ForEach(invites) { inv in
                    Section {
                        Text(String(format: L("community_invite_t"), inv.name)).font(.headline)
                        Text(String(format: L("community_invite_from"), contactName(inv.fromContactId))).foregroundStyle(c.ink2)
                        HStack {
                            Button(L("community_accept")) { model.core.communityAccept(communityId: inv.communityId); reload() }
                            Spacer()
                            Button(L("community_decline")) { model.core.communityDecline(communityId: inv.communityId); reload() }
                        }.buttonStyle(.borderless)
                    }
                }
                Section {
                    if list.isEmpty && invites.isEmpty {
                        Text(L("community_none")).foregroundStyle(c.ink3)
                    }
                    ForEach(list) { k in
                        NavigationLink {
                            CommunityDetailView(communityId: k.id)
                        } label: {
                            VStack(alignment: .leading) {
                                Text(k.name).font(.headline)
                                Text(subtitle(k)).font(.footnote).foregroundStyle(k.removedReason != nil ? c.danger : c.ink2)
                            }
                        }
                    }
                }
                Section(L("community_create_t")) {
                    Text(L("community_create_d")).font(.footnote).foregroundStyle(c.ink2)
                    TextField(L("community_name"), text: $name)
                    Button(L("community_create")) {
                        if model.core.communityCreate(name: String(name.prefix(60))) != nil { name = "" }
                        reload()
                    }.disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
            .navigationTitle(L("tab_communities"))
            .task {
                while !Task.isCancelled {
                    reload()
                    try? await Task.sleep(nanoseconds: 1_000_000_000)
                }
            }
        }
    }

    private func reload() {
        list = model.core.communities()
        invites = model.core.communityInvites()
    }

    private func contactName(_ id: String) -> String {
        model.core.contacts().first { $0.id == id }?.displayName ?? "omcp1:\(id.prefix(4))…"
    }

    private func subtitle(_ k: CommunityModel) -> String {
        if k.removedReason != nil { return L("community_removed") }
        if k.liveCount > 0 { return String(format: L("community_sub_live"), k.members, k.liveCount) }
        return String(format: L("community_sub"), k.members)
    }
}

/// One community: who's live, go live, members (invite; remove from your live).
struct CommunityDetailView: View {
    let communityId: String
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var info: CommunityModel?
    @State private var members: [CommunityMemberModel] = []
    @State private var streams: [LiveStreamModel] = []
    @State private var stats = LiveStatsModel()
    @State private var title = ""
    @State private var online = false
    @State private var session: LiveSession?
    @State private var takenDown = false
    @State private var invited: Set<String> = []
    private let monitor = NWPathMonitor()

    var body: some View {
        let removed = info?.removedReason != nil
        let mine = streams.first { $0.mine && $0.communityId == communityId }
        let hostingAny = streams.contains { $0.mine }
        let lives = streams.filter { !$0.mine && $0.communityId == communityId }
        List {
            if removed { Section { Text(L("community_removed_body")).foregroundStyle(c.danger) } }
            if takenDown { Section { Text(L("live_taken_down")).foregroundStyle(c.danger) } }
            if let mine {
                Section {
                    Text(L("live_badge")).font(.caption).foregroundStyle(c.danger)
                    Text(mine.title).font(.headline)
                    Text(L("community_live_host_note")).font(.footnote).foregroundStyle(c.ink2)
                    Button(L("live_stop")) { session?.stop(); model.core.liveStop(); reload() }
                }
            }
            Section(L("live_happening")) {
                if lives.isEmpty { Text(L("community_no_live")).foregroundStyle(c.ink3) }
                ForEach(lives) { s in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(L("live_badge")).font(.caption).foregroundStyle(c.danger)
                        Text(s.title).font(.headline)
                        Text(memberName(s.hostContactId)).foregroundStyle(c.ink2)
                        if s.listening {
                            Text(String(format: L("live_listening"), stats.lossPercent)).foregroundStyle(c.ink2)
                            Button(L("live_leave")) { session?.stop(); model.core.liveLeave(); reload() }
                        } else {
                            Button(L("live_join")) {
                                if model.core.liveJoin(streamIdHex: s.streamIdHex, hasInternet: online) {
                                    takenDown = false
                                    liveSession().startListening(video: online && s.video)
                                }
                                reload()
                            }.disabled(hostingAny || removed)
                        }
                    }
                }
            }
            if !hostingAny && !removed {
                Section(L("live_start_title")) {
                    Text(L("community_live_body")).font(.footnote).foregroundStyle(c.ink2)
                    TextField(L("live_title_hint"), text: $title)
                    Button(L("live_go")) { goLive() }
                }
            }
            Section(String(format: L("community_members"), members.count)) {
                ForEach(members) { m in
                    HStack {
                        Text(m.isMe ? L("community_you") : m.name)
                        Spacer()
                        if mine != nil && !m.isMe {
                            Button(L("live_remove_viewer")) { model.core.liveRemoveListener(contactId: m.peerId); reload() }
                                .buttonStyle(.borderless)
                        }
                    }
                }
            }
            if info?.founderIsMe == true && !removed {
                let ids = Set(members.map(\.peerId))
                let invitable = model.core.contacts().filter { !ids.contains($0.id) }
                if !invitable.isEmpty {
                    Section(L("community_invite_h")) {
                        ForEach(invitable) { ct in
                            HStack {
                                Text(ct.displayName)
                                Spacer()
                                Button(L(invited.contains(ct.id) ? "community_invited" : "community_invite")) {
                                    if model.core.communityInvite(communityId: communityId, contactId: ct.id) { invited.insert(ct.id) }
                                }.buttonStyle(.borderless).disabled(invited.contains(ct.id))
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle(info?.name ?? "")
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

    private func memberName(_ id: String) -> String {
        members.first { $0.peerId == id }?.name ?? "omcp1:\(id.prefix(4))…"
    }

    private func reload() {
        // Team takedown: stop the mic and camera, close every WebRTC peer.
        if !model.core.liveTakeEnded().isEmpty { session?.stop(); takenDown = true }
        info = model.core.communities().first { $0.id == communityId }
        members = model.core.communityMembers(communityId: communityId)
        streams = model.core.liveStreams()
        stats = model.core.liveStats()
        if session != nil && !streams.contains(where: { $0.listening || $0.mine }) { session?.stop() }
    }

    private func liveSession() -> LiveSession {
        if let session { return session }
        let s = LiveSession(core: model.core, kick: model.repo.kick)
        session = s
        return s
    }

    private func goLive() {
        let audio = AVAudioSession.sharedInstance()
        let start = {
            let t = title.trimmingCharacters(in: .whitespaces)
            let video = online && AVCaptureDevice.authorizationStatus(for: .video) == .authorized
            if model.core.liveStartCommunity(communityId: communityId, title: t.isEmpty ? "Live" : t, video: video) != nil {
                takenDown = false
                liveSession().startHosting(video: video)
            }
            reload()
        }
        switch audio.recordPermission {
        case .granted: start()
        case .denied: break
        default: audio.requestRecordPermission { ok in DispatchQueue.main.async { if ok { start() } } }
        }
    }
}

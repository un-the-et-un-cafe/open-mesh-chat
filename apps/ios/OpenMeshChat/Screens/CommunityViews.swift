import SwiftUI
import UIKit
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
    @State private var liveNow: [LiveStreamModel] = []

    var body: some View {
        NavigationStack {
            List {
                Section { NavigationLink(L("live_entry")) { LiveTabView() } }
                // §5.120: one card per active community live on top.
                if !liveNow.isEmpty {
                    Section {
                        ForEach(liveNow) { st in
                            NavigationLink {
                                CommunityDetailView(communityId: st.communityId)
                            } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(L("live_badge")).font(.caption).foregroundStyle(c.danger)
                                    Text(st.title).font(.headline)
                                    Text(list.first { $0.id == st.communityId }?.name ?? "").foregroundStyle(c.ink2)
                                    Text(L("live_join")).font(.subheadline.weight(.semibold))
                                }
                            }
                        }
                    }
                }
                ForEach(invites) { inv in
                    Section {
                        Text(String(format: L("community_invite_t"), inv.name)).font(.headline)
                        Text(String(format: L("community_invite_from"), contactName(inv.fromContactId))).foregroundStyle(c.ink2)
                        Text(L("community_inv_note")).font(.footnote).foregroundStyle(c.ink2)
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
        liveNow = model.core.liveStreams().filter { !$0.communityId.isEmpty && !$0.mine }
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
    @State private var takenDownMine = false
    @State private var takedownReason: Int?
    @State private var removedViewers: Set<String> = []
    @State private var confirmRemove: CommunityMemberModel?
    @State private var toast: String?
    @State private var report: String?
    @State private var info2: (String, String)?
    @State private var invited: Set<String> = []
    private let monitor = NWPathMonitor()

    var body: some View {
        let removed = info?.removedReason != nil
        let mine = streams.first { $0.mine && $0.communityId == communityId }
        let hostingAny = streams.contains { $0.mine }
        let lives = streams.filter { !$0.mine && $0.communityId == communityId }
        List {
            if removed {
                Section {
                    Label(L("community_removed"), systemImage: "shield.lefthalf.filled").font(.headline).foregroundStyle(c.danger)
                    Text(L("community_removed_body"))
                    Button(L("mod_learn")) { info2 = (L("mod_learn"), L("mod_learn_d")) }
                }
            }
            if takenDown {
                // §5.123: host sees reason + Appeal (placeholder: §23.5 jury not built); viewer the plain line.
                Section {
                    if takenDownMine {
                        Label(L("live_td_t"), systemImage: "shield.lefthalf.filled").font(.headline).foregroundStyle(c.danger)
                        Text(L("live_td_host"))
                        if let r = takedownReason { Text(String(format: L("live_td_reason"), L(CommunityLiveRules.reasonKey(r)))) }
                        Button(L("live_appeal")) { info2 = (L("live_appeal_t"), L("live_appeal_d")) }
                        Button(L("mod_learn")) { info2 = (L("mod_learn"), L("mod_learn_d")) }
                    } else {
                        Label(L("live_taken_down"), systemImage: "shield.lefthalf.filled").foregroundStyle(c.danger)
                    }
                    Button(L("ok")) { takenDown = false }
                }
            }
            if let mine {
                Section {
                    Text(L("live_host_t") + " · " + L("live_badge")).font(.caption).foregroundStyle(c.danger)
                    Text(mine.title).font(.headline)
                    Text(String(format: L("live_listeners"), members.filter { !$0.isMe && !removedViewers.contains($0.peerId) }.count))
                        .foregroundStyle(c.ink2)
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
                        Button(L("live_report")) { report = s.streamIdHex }.buttonStyle(.borderless).frame(minHeight: 44)
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
            if !hostingAny {
                Section(L("live_start_title")) {
                    Text(L("community_live_body")).font(.footnote).foregroundStyle(c.ink2)
                    TextField(L("live_title_hint"), text: $title).disabled(removed)
                    Button(L("live_go")) { goLive() }.disabled(removed)
                }
            }
            Section(String(format: L("community_members"), members.count)) {
                ForEach(members) { m in
                    HStack {
                        Text(m.isMe ? L("community_you") : m.name)
                        Spacer()
                        if mine != nil && m.isMe { Text(L("live_host_label")).foregroundStyle(c.ink3) }
                        if mine != nil && !m.isMe && !removedViewers.contains(m.peerId) {
                            // §5.122: visible button + ⋯ menu, no swipe-only path (WCAG 2.5.7).
                            Button(L("live_remove_viewer")) { confirmRemove = m }
                                .buttonStyle(.bordered).frame(minHeight: 44)
                            Menu {
                                Button(L("live_remove_viewer"), role: .destructive) { confirmRemove = m }
                            } label: {
                                Image(systemName: "ellipsis").frame(width: 44, height: 44)
                            }.accessibilityLabel(String(format: L("live_more_for"), m.name))
                        }
                    }
                }
            }
            if info?.founderIsMe == true && !removed {
                let ids = Set(members.map(\.peerId))
                let invitable = model.core.contacts().filter { !ids.contains($0.id) }
                let full = CommunityLiveRules.isFull(members: members.count)
                Section(L("community_invite_h")) {
                    Text(String(format: L("community_counter"), members.count)).foregroundStyle(full ? c.danger : c.ink2)
                    ProgressView(value: Double(members.count), total: Double(CommunityLiveRules.maxMembers))
                        .tint(full ? c.danger : nil)
                        .accessibilityValue(String(format: L("community_counter"), members.count))
                    Text(L("community_founder_only")).font(.footnote).foregroundStyle(c.ink3)
                    if full {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(L("community_full_t")).font(.headline)
                            Text(L("community_full_d"))
                        }.foregroundStyle(c.danger)
                    }
                    ForEach(invitable) { ct in
                        HStack {
                            Text(ct.displayName)
                            Spacer()
                            // Full: still focusable so VoiceOver reads why (aria-disabled, §5.121).
                            Button(L(invited.contains(ct.id) ? "community_invited" : "community_invite")) {
                                guard !full else { return }
                                if model.core.communityInvite(communityId: communityId, contactId: ct.id) { invited.insert(ct.id) }
                            }.buttonStyle(.borderless).frame(minHeight: 44).disabled(invited.contains(ct.id))
                                .opacity(full ? 0.4 : 1)
                                .accessibilityHint(full ? L("community_full_d") : "")
                        }
                    }
                }
            }
        }
        .navigationTitle(info?.name ?? "")
        .confirmationDialog(confirmRemove.map { String(format: L("live_rm_t"), $0.name) } ?? "",
                            isPresented: Binding(get: { confirmRemove != nil }, set: { if !$0 { confirmRemove = nil } }),
                            titleVisibility: .visible, presenting: confirmRemove) { m in
            Button(L("live_remove_viewer"), role: .destructive) { removeFromLive(m) }
            Button(L("cancel"), role: .cancel) { confirmRemove = nil }
        } message: { _ in Text(L("live_rm_d")) }
        .alert(info2?.0 ?? "", isPresented: Binding(get: { info2 != nil }, set: { if !$0 { info2 = nil } })) {
            Button(L("ok")) { info2 = nil }
        } message: { Text(info2?.1 ?? "") }
        .sheet(isPresented: Binding(get: { report != nil }, set: { if !$0 { report = nil } })) {
            if let r = report { ReportLiveView(streamIdHex: r) { report = nil } }
        }
        .overlay(alignment: .bottom) {
            if let toast {
                Text(toast).padding().background(.thinMaterial, in: Capsule()).padding()
                    .accessibilityAddTraits(.updatesFrequently)
                    .task { try? await Task.sleep(nanoseconds: 4_000_000_000); self.toast = nil }
            }
        }
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

    private func removeFromLive(_ m: CommunityMemberModel) {
        model.core.liveRemoveListener(contactId: m.peerId)
        removedViewers.insert(m.peerId)
        let n = CommunityLiveRules.rekeyRecipients(members: members.map(\.peerId), me: members.first { $0.isMe }?.peerId,
                                                   removed: removedViewers)
        toast = String(format: L("live_rm_done"), m.name, n)
        UIAccessibility.post(notification: .announcement, argument: toast)
        confirmRemove = nil
        reload()
    }

    private func reload() {
        // Team takedown: stop the mic and camera, close every WebRTC peer.
        if let e = model.core.liveTakeEnded().first {
            session?.stop(); takenDown = true; takenDownMine = e.mine; removedViewers = []
            takedownReason = e.mine ? model.core.moderationLiveRemovedReason(streamIdHex: e.streamIdHex) : nil
        }
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

/// §5.126 Report this live: stream id (LTR, monospace, selectable), Copy, reason, Send.
/// No report inbox in the protocol yet: Send opens the share sheet with id + reason.
struct ReportLiveView: View {
    let streamIdHex: String
    let onDone: () -> Void
    @State private var reason = 0
    @State private var copied = false

    var body: some View {
        NavigationStack {
            Form {
                Section { Text(L("live_report_d")) }
                Section(L("live_report_sid")) {
                    Text(CommunityLiveRules.groupStreamId(streamIdHex)).font(.body.monospaced())
                        .textSelection(.enabled).environment(\.layoutDirection, .leftToRight)
                    Button(L(copied ? "live_report_copied" : "live_report_copy")) {
                        UIPasteboard.general.string = streamIdHex.lowercased(); copied = true
                    }.frame(minHeight: 44)
                }
                Section(L("live_report_why")) {
                    Picker(L("live_report_why"), selection: $reason) {
                        ForEach(0..<CommunityLiveRules.reportReasonKeys.count, id: \.self) { i in
                            Text(L(CommunityLiveRules.reportReasonKeys[i])).tag(i)
                        }
                    }.pickerStyle(.inline).labelsHidden()
                }
                Section {
                    ShareLink(item: CommunityLiveRules.reportText(streamIdHex: streamIdHex,
                                                                  reason: L(CommunityLiveRules.reportReasonKeys[reason]))) {
                        Text(L("live_report_send"))
                    }
                    Text(L("live_report_note")).font(.footnote)
                }
            }
            .navigationTitle(L("live_report"))
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button(L("cancel"), action: onDone) } }
        }
    }
}

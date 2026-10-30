import HoplaKit
import PhotosUI
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

struct ChatsListView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c

    var body: some View {
        let _ = model.revision
        let contacts = model.repo.contacts
        let status = model.core.pathStatus()
        NavigationStack {
            VStack(spacing: 0) {
                if model.repo.backupGate() == .banner { BackupReminderBanner() }
                HStack(spacing: 8) {
                    PathChip(kind: status.aggregatePath, label: L("status_nearby", status.nearbyPeers))
                    if status.queuedCount > 0 { PathChip(kind: .queued, label: L("status_queued", status.queuedCount)) }
                }
                .padding(.horizontal, 16).padding(.vertical, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                if contacts.isEmpty {
                    Spacer()
                    VStack(spacing: 12) {
                        Text(L("chats_empty_title")).font(.title2.weight(.semibold))
                        Text(L("chats_empty_body")).font(.subheadline).foregroundStyle(c.ink2).multilineTextAlignment(.center)
                    }.padding(.horizontal, 32)
                    Spacer()
                } else {
                    List(contacts) { contact in
                        NavigationLink(value: contact.id) { ChatRowView(contact: contact, last: model.repo.messages[contact.id]?.last) }
                    }
                    .listStyle(.plain)
                }
            }
            .background(c.bg)
            .navigationTitle(L("tab_chats"))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink { ContactAddView() } label: { Image(systemName: "plus") }
                        .accessibilityLabel(L("contact_add_title"))
                }
            }
            .navigationDestination(for: String.self) { ChatThreadView(contactId: $0) }
        }
    }
}

/// Verified seal only for verified contacts; unverified rows show nothing (design §5C).
struct VerifiedBadge: View {
    @Environment(\.omcpColors) private var c
    let contact: Contact
    var body: some View {
        if contact.showsVerifiedBadge {
            Image(systemName: "checkmark.seal.fill").foregroundStyle(c.ok).font(.caption)
                .accessibilityLabel(L("verified_contact"))
        } else if contact.restoredUnverified {
            // RS2 (§5.98): neutral pill with a sync glyph until re-verified in person.
            Label(L("restored_pill"), systemImage: "arrow.triangle.2.circlepath")
                .font(.caption2).foregroundStyle(c.ink2)
                .padding(.horizontal, 6).padding(.vertical, 2)
                .background(c.surface2).clipShape(Capsule())
                .overlay(Capsule().stroke(c.line))
        }
    }
}

struct ChatRowView: View {
    @Environment(\.omcpColors) private var c
    let contact: Contact
    let last: ChatMessage?

    private var preview: String {
        guard let last else { return "" }
        if last.status == .system { return contact.restoredUnverified ? L("restored_preview", contact.displayName) : "" }
        if last.burn != nil { return L("burn_preview") }
        if let f = last.file { return f.name }
        return last.text
    }

    var body: some View {
        HStack(spacing: 12) {
            Text(String(contact.displayName.prefix(2)).uppercased())
                .font(.headline).frame(width: 48, height: 48).background(c.surface2).clipShape(Circle())
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) { Text(contact.displayName).font(.headline); VerifiedBadge(contact: contact) }
                Text(preview).font(.subheadline).foregroundStyle(c.ink2).lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .frame(minHeight: 56)
    }
}

struct ChatThreadView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let contactId: String
    @State private var draft = ""
    @State private var burnNext = false
    @State private var viewing: (id: String, content: BurnContent)?
    @State private var fileCandidate: (name: String, size: UInt64, mime: String, url: URL)?
    @State private var previewURL: URL?
    @State private var showImporter = false
    @State private var error: String?

    var body: some View {
        let _ = model.revision
        let contact = model.repo.contact(contactId)
        let msgs = model.repo.messages[contactId] ?? []
        let queued = model.core.queuedFor(contactId)
        VStack(spacing: 0) {
            PathBanner(kind: queued > 0 ? .queued : .mesh, text: L(queued > 0 ? "path_queued" : "path_mesh"))
            restoreBanners(contact: contact, msgs: msgs)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(msgs) { m in bubble(m) }
                }.padding(16)
            }
            if let error { Text(error).font(.caption).foregroundStyle(c.danger).padding(.horizontal) }
            composer
        }
        .background(c.bg)
        .navigationTitle(contact?.displayName ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 4) { Text(contact?.displayName ?? "").font(.headline); if let contact { VerifiedBadge(contact: contact) } }
            }
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink { ChatInfoView(contactId: contactId) } label: { Image(systemName: "info.circle") }
                    .accessibilityLabel(L("chat_info"))
            }
        }
        .fileImporter(isPresented: $showImporter, allowedContentTypes: [.item]) { r in
            guard case .success(let url) = r else { return }
            let ok = url.startAccessingSecurityScopedResource()
            defer { if ok { url.stopAccessingSecurityScopedResource() } }
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize).map(UInt64.init) ?? 0
            let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
            fileCandidate = (url.lastPathComponent, size, mime, url)
        }
        .sheet(isPresented: Binding(get: { fileCandidate != nil }, set: { if !$0 { fileCandidate = nil } })) {
            if let f = fileCandidate {
                FileConfirmSheet(name: f.name, size: f.size, contactName: contact?.displayName ?? "", queued: queued > 0,
                                 onSend: { send(file: f) }, onChooseAnother: { fileCandidate = nil; showImporter = true })
                    .presentationDetents([.medium, .large])
            }
        }
        .quickLookPreview($previewURL)
        .fullScreenCover(isPresented: Binding(get: { viewing != nil }, set: { if !$0 { closeViewer() } })) {
            if let v = viewing { BurnViewer(content: v.content, onClose: closeViewer) }
        }
    }

    private var composer: some View {
        HStack {
            Button { showImporter = true } label: { Image(systemName: "paperclip") }
                .accessibilityLabel(L("attach_file"))
            TextField(L("composer_placeholder"), text: $draft)
                .padding(10).background(c.surface2, in: RoundedRectangle(cornerRadius: 10))
            Button { send() } label: { Image(systemName: burnNext ? "flame.fill" : "paperplane.fill") }
                .foregroundStyle(burnNext ? .orange : c.brand)
                .contextMenu {
                    Button { burnNext.toggle() } label: { Label(L(burnNext ? "burn_toggle_off" : "burn_toggle_on"), systemImage: "flame") }
                }
                .accessibilityLabel(L(burnNext ? "send_burn" : "send"))
                .accessibilityHint(L("burn_long_press_hint"))
        }
        .padding(8).background(c.surface)
    }

    private func send() {
        do {
            try model.repo.send(contactId, text: draft, burnOnce: burnNext ? .onOpen() : nil)
            draft = ""
            burnNext = false
            error = nil
        } catch { self.error = L("send_failed") }
    }

    /// Size is checked against the Rust cap before the file is read or encrypted (§5.23);
    /// reading happens off the main thread, encryption into 64 KiB chunks in Rust.
    private func send(file f: (name: String, size: UInt64, mime: String, url: URL)) {
        fileCandidate = nil
        do { try model.repo.checkFile(size: f.size) } catch { self.error = L("file_too_large_title"); return }
        let repo = model.repo, cid = contactId
        Task.detached(priority: .userInitiated) {
            let ok = f.url.startAccessingSecurityScopedResource()
            defer { if ok { f.url.stopAccessingSecurityScopedResource() } }
            let data = try? Data(contentsOf: f.url, options: .mappedIfSafe)
            await MainActor.run {
                guard let data else { self.error = L("file_read_error"); return }
                do { try repo.sendFile(cid, name: f.name, mime: f.mime, bytes: data) } catch CoreError.fileTooLarge {
                    self.error = L("file_too_large_title")
                } catch { self.error = L("file_read_error") }
            }
        }
    }

    private func closeViewer() {
        if let v = viewing { try? model.repo.closeBurn(contactId, msgId: v.id) }
        viewing = nil
    }

    @State private var hideSelfBanner = false
    @State private var hideMediaBanner = false
    @State private var resendFor: ChatMessage?
    @State private var resendText = ""
    @State private var resendSent: Set<String> = []

    @ViewBuilder private func restoreBanners(contact: Contact?, msgs: [ChatMessage]) -> some View {
        let peer = contact?.displayName ?? ""
        if model.repo.restoredAtMs != nil && !hideSelfBanner {
            Text(L("restored_self_banner")).font(.footnote).padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2).onTapGesture { hideSelfBanner = true }
                .accessibilityAddTraits(.updatesFrequently)
        }
        let waiting = msgs.filter { $0.file?.isPlaceholder == true }.count
        if waiting > 0 && !hideMediaBanner {
            Text(L("media_banner", waiting, peer)).font(.footnote).padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2).onTapGesture { hideMediaBanner = true }
        }
        ForEach(model.repo.resendAsks.filter { $0.contactId == contactId }, id: \.msgId) { a in
            Text(a.note.map { L("media_resend_asked_note", peer, a.name, $0) } ?? L("media_resend_asked", peer, a.name)).font(.footnote).padding(10).frame(maxWidth: .infinity, alignment: .leading)
                .background(c.surface2).onTapGesture { model.repo.dismissResendAsk(a.msgId) }
        }
    }

    /// RS1 notice (§5.95/§5.98): centred, neutral; tap → Security centre to verify by QR.
    @ViewBuilder private func systemNotice(_ m: ChatMessage, peer: String) -> some View {
        if let at = m.restoredAtMs {
            let date = Date(timeIntervalSince1970: Double(at > 0 ? at : m.timeMs) / 1000).formatted(date: .abbreviated, time: .omitted)
            NavigationLink { ChatInfoView(contactId: contactId) } label: {
                VStack(spacing: 2) {
                    Text(L("restored_notice", peer, date)).font(.caption).foregroundStyle(c.ink2)
                    Text(L("restored_notice_link")).font(.caption.bold()).foregroundStyle(c.brand)
                }
                .padding(.horizontal, 14).padding(.vertical, 8).background(c.surface2).clipShape(RoundedRectangle(cornerRadius: 12))
            }
            .frame(maxWidth: .infinity)
        }
    }

    /// Missing-media placeholder (§5.92): no red, "NOT RESTORED YET", Request again / Ask to resend.
    @ViewBuilder private func placeholder(_ m: ChatMessage, _ f: FileAttachment, peer: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(L("media_not_restored")).font(.caption2.bold()).foregroundStyle(c.ink2)
            Text(f.name).font(.subheadline.bold())
            Text(m.id.isEmpty ? "" : (resendSent.contains(m.id) ? L("media_request_sent", peer)
                 : f.state == .requested ? L("media_requested")
                 : f.state == .unavailable ? L("media_gone", peer) : L("media_waiting", peer)))
                .font(.caption).foregroundStyle(c.ink2)
            if !resendSent.contains(m.id) {
                if f.state == .missing {
                    Button(L("media_request_again")) { model.repo.requestMedia(contactId, msgId: m.id) }.font(.caption.bold())
                } else if f.state == .unavailable {
                    Button(L("media_ask_resend")) { resendText = ""; resendFor = m }.font(.caption.bold())
                }
            }
        }
        .padding(12).background(c.surface2).clipShape(RoundedRectangle(cornerRadius: 14))
        .alert(L("media_resend_title", peer), isPresented: Binding(get: { resendFor?.id == m.id }, set: { if !$0 { resendFor = nil } })) {
            TextField(L("media_resend_label"), text: $resendText)
            Button(L("media_resend_send")) {
                try? model.repo.askToResend(contactId, msgId: m.id, note: resendText)
                resendSent.insert(m.id); resendFor = nil
            }
            Button(L("cancel_action"), role: .cancel) { resendFor = nil }
        } message: { Text(L("media_resend_note")) }
    }

    @ViewBuilder private func bubble(_ m: ChatMessage) -> some View {
        let peer = model.repo.contact(contactId)?.displayName ?? ""
        if m.status == .system { systemNotice(m, peer: peer) }
        else if let f = m.file, f.isPlaceholder {
            placeholder(m, f, peer: peer).frame(maxWidth: .infinity, alignment: m.mine ? .trailing : .leading)
        }
        else { regularBubble(m) }
    }

    @ViewBuilder private func regularBubble(_ m: ChatMessage) -> some View {
        VStack(alignment: m.mine ? .trailing : .leading, spacing: 4) {
            Group {
                if m.burn != nil { burnBubble(m) }
                else if let f = m.file {
                    FileBubble(file: f, mine: m.mine, peerName: model.repo.contact(contactId)?.displayName ?? "",
                               onCancel: { model.repo.cancelFile(contactId, transferId: m.id) },
                               onOpen: { if let p = f.localPath { previewURL = URL(fileURLWithPath: p) } })
                }
                else { Text(m.text) }
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .background(m.mine ? c.brandSoft : c.surface)
            .clipShape(RoundedRectangle(cornerRadius: 14))
            Text(L(m.mine ? (m.status == .queued ? "msg_queued" : m.status == .delivered ? "msg_delivered" : "msg_sent") : "msg_received")).font(.caption2).foregroundStyle(c.ink3)
        }
        .frame(maxWidth: .infinity, alignment: m.mine ? .trailing : .leading)
        // No copy / share / forward / quote for burn content (B2).
        .contextMenu { if m.canShare && !m.text.isEmpty { Button(L("copy")) { UIPasteboard.general.string = m.text } } }
    }

    @ViewBuilder private func burnBubble(_ m: ChatMessage) -> some View {
        switch m.burnState {
        case .opened: Text(L("burn_photo_opened"))
        case .open where !m.mine:
            if let at = m.burnAtMs {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    Text(L("burn_countdown", max(0, Int((at - model.repo.now()) / 1000))))
                }
            } else { Text(L("burn_open")) }
        default:
            if m.mine { Label(L("burn_sent"), systemImage: "flame") }
            else {
                Button {
                    if let content = try? model.repo.openBurn(contactId, msgId: m.id) { viewing = (m.id, content) }
                } label: { Label(L(m.isViewOnceMedia ? "burn_tap_view_once" : "burn_tap_to_read"), systemImage: "flame") }
                    .blur(radius: 0)
            }
        }
    }
}

/// File bubble (§5.24, Designy v0.1.1 `file-transfer`): queued / sending / receiving / paused /
/// sent / received (integrity checked in Rust) / failed / integrity fail, with Cancel, Open, Delete.
struct FileBubble: View {
    let file: FileAttachment
    let mine: Bool
    let peerName: String
    let onCancel: () -> Void
    let onOpen: () -> Void
    @Environment(\.omcpColors) private var c

    private func mb(_ b: UInt64) -> String { String(format: "%.1f", Double(b) / 1_000_000) }

    private var line: String {
        let done = mb(file.doneBytes), total = mb(file.sizeBytes)
        switch file.state {
        case .queued: return L("file_state_queued", total)
        case .sending: return L("file_state_sending", done, total)
        case .sent: return L("file_state_sent", total)
        case .receiving: return L("file_state_receiving", done, total)
        case .paused: return L("file_state_paused", done, total)
        case .received: return L("file_state_received", total)
        case .failed: return L("file_state_failed")
        case .integrityFailed: return L("file_state_integrity", peerName)
        case .missing, .requested, .unavailable: return L("media_waiting", peerName)
        }
    }

    private var tint: Color {
        switch file.state {
        case .paused, .queued: return c.pathQueue
        case .failed, .integrityFailed: return c.danger
        default: return c.brand
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Image(systemName: "doc").accessibilityHidden(true)
                Text(file.name).font(.subheadline.bold()).lineLimit(1)
                Spacer()
                switch file.state {
                case .queued, .sending, .paused, .receiving:
                    Button(action: onCancel) { Image(systemName: "xmark.circle") }
                        .accessibilityLabel(L(mine ? "file_cancel_a11y" : "file_cancel_receive_a11y", file.name))
                case .integrityFailed:
                    Button(L("file_delete"), role: .destructive, action: onCancel)
                case .received where file.localPath != nil:
                    Button(action: onOpen) { Image(systemName: "arrow.up.forward.square") }
                        .accessibilityLabel(L("file_open_a11y", file.name))
                default:
                    EmptyView()
                }
            }
            Text(line).font(.caption).foregroundStyle(file.state == .failed || file.state == .integrityFailed ? c.danger : c.ink2)
            ProgressView(value: file.fraction).tint(tint)
                .accessibilityValue(L("file_progress_a11y", Int(file.fraction * 100)))
            if file.state == .received { Text(L("file_integrity_checked")).font(.caption2).foregroundStyle(c.ink3) }
            if file.state == .paused { Text(L("file_paused_meta")).font(.caption2).foregroundStyle(c.ink3) }
        }
        .frame(minWidth: 220)
        .contentShape(Rectangle())
        .onTapGesture { if file.state == .received { onOpen() } }
    }
}

/// Full-screen burn viewer: shown once; closing burns it. Secure layer is window-wide.
struct BurnViewer: View {
    let content: BurnContent
    let onClose: () -> Void
    var body: some View {
        VStack(spacing: 16) {
            HStack { Spacer(); Button(L("close"), action: onClose) }.padding()
            Spacer()
            if content.mime?.hasPrefix("image/") == true, let img = UIImage(data: content.bytes) {
                Image(uiImage: img).resizable().scaledToFit()
            } else {
                Text(content.text).font(.title3).padding()
            }
            if let at = content.burnAtMs {
                TimelineView(.periodic(from: .now, by: 1)) { _ in
                    let left = max(0, Int((Double(at) / 1000 - Date().timeIntervalSince1970).rounded(.up)))
                    Text(L("burn_countdown", left)).font(.caption).onChange(of: left) { _, v in if v == 0 { onClose() } }
                }
            }
            Spacer()
            Text(L("burn_honest")).font(.caption).foregroundStyle(.secondary).padding()
        }
    }
}

/// Design §5.23 file-picker-confirm (+ too-large). The cap comes from the Rust core.
struct FileConfirmSheet: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    let name: String
    let size: UInt64
    let contactName: String
    let queued: Bool
    let onSend: () -> Void
    let onChooseAnother: () -> Void

    var body: some View {
        let max = model.core.maxFileBytes()
        let check = FilePolicy.check(size: size, core: model.core)
        let mb = String(format: "%.1f", Double(size) / 1_000_000)
        NavigationStack {
            Form {
                Section {
                    Label(name, systemImage: "doc").foregroundStyle(check == .ok ? c.ink : c.danger)
                    LabeledContent(L("file_size"), value: L("file_size_of_max", mb, FilePolicy.label(max)))
                    LabeledContent(L("file_route")) { PathChip(kind: queued ? .queued : .mesh, label: L(queued ? "path_queued" : "path_mesh")) }
                }
                if check == .ok {
                    Section { Text(queued ? L("file_queue_hint", contactName) : L("file_mesh_hint", contactName)).font(.footnote) }
                    Button(L(queued ? "file_queue" : "send"), action: onSend).bold()
                } else {
                    Section(header: Text(L("file_too_large_title"))) {
                        Text(L("file_too_large_body", mb, FilePolicy.label(max), FilePolicy.label(max))).font(.footnote)
                    }
                    Button(L("file_choose_another"), action: onChooseAnother).bold()
                }
            }
            .navigationTitle(L("file_send_title", contactName))
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

/// Chat info: verification, per-chat Private notifications override (both ways), burn after read.
struct ChatInfoView: View {
    @EnvironmentObject private var model: AppModel
    let contactId: String

    var body: some View {
        let _ = model.revision
        let store = model.store
        Form {
            if let contact = model.repo.contact(contactId) {
                Section { NavigationLink(L("security_centre")) { SecurityCentreView(contact: contact) } }
            }
            Section(header: Text(L("notif_title")), footer: Text(L("notif_private_footer"))) {
                Picker(L("notif_private"), selection: Binding<Int>(
                    get: { store.chatPrivateOverride[contactId].map { $0 ? 1 : 0 } ?? -1 },
                    set: { v in NotificationPolicy.setChatPrivate(store, chatId: contactId, v == -1 ? nil : v == 1); model.touch() })) {
                    Text(L(store.privateNotifications ? "notif_follow_global_on" : "notif_follow_global_off")).tag(-1)
                    Text(L("notif_private_on")).tag(1)
                    Text(L("notif_private_off")).tag(0)
                }
            }
            Section(header: Text(L("burn_title")), footer: Text(L("burn_honest"))) {
                Picker(L("burn_title"), selection: Binding<Int>(
                    get: { store.burnChats[contactId] ?? -1 },
                    set: { v in try? model.repo.setChatBurn(contactId, option: v, unreadTtl: store.burnUnreadTtl[contactId] ?? BurnSpec.unread7d) })) {
                    ForEach(BurnSpec.chatOptions, id: \.self) { Text(L("burn_opt_\($0)")).tag($0) }
                }
                Picker(L("burn_unread"), selection: Binding<Int>(
                    get: { store.burnUnreadTtl[contactId] ?? BurnSpec.unread7d },
                    set: { v in try? model.repo.setChatBurn(contactId, option: store.burnChats[contactId] ?? -1, unreadTtl: v) })) {
                    Text(L("burn_ttl_1h")).tag(BurnSpec.unread1h)
                    Text(L("burn_ttl_24h")).tag(BurnSpec.unread24h)
                    Text(L("burn_ttl_7d")).tag(BurnSpec.unread7d)
                }
            }
        }
        .navigationTitle(L("chat_info"))
    }
}

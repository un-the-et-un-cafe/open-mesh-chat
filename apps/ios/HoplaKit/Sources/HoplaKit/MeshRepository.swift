import Foundation

/// File attachment state (DESIGN_SPEC §5.24, Designy v0.1.1 `file-transfer`).
public struct FileAttachment: Equatable, Sendable {
    public enum State: Sendable {
        case queued, sending, receiving, paused, sent, received, failed, integrityFailed
        /// Restored from the chat backup without bytes (§5.92), re-requested, or gone at the partner.
        case missing, requested, unavailable
    }
    public var isPlaceholder: Bool { state == .missing || state == .requested || state == .unavailable }
    public var name: String
    public var sizeBytes: UInt64
    public var mime: String
    public var state: State
    /// Bytes sent (sealed onto the link) or received so far.
    public var doneBytes: UInt64 = 0
    /// Received files: where the verified plaintext was saved (app container).
    public var localPath: String?

    public init(name: String, sizeBytes: UInt64, mime: String, state: State, doneBytes: UInt64 = 0, localPath: String? = nil) {
        self.name = name
        self.sizeBytes = sizeBytes
        self.mime = mime
        self.state = state
        self.doneBytes = doneBytes
        self.localPath = localPath
    }

    public var fraction: Double {
        switch state {
        case .queued: return 0
        case .sent, .received, .integrityFailed: return 1
        default: return sizeBytes == 0 ? 0 : min(1, Double(doneBytes) / Double(sizeBytes))
        }
    }
}

/// Stores a verified received file; returns its local path (nil = couldn't save).
public protocol FileSink: AnyObject {
    func save(_ file: ReceivedFileInfo) -> String?
}

public struct ChatMessage: Equatable, Identifiable, Sendable {
    /// `.system`: in-chat notice; `text` = `restored:<ms>` (RS1, §5.95).
    /// `.delivered`: end-to-end delivery ack received over the mesh (PROTOCOL §22.8(3)).
    public enum Status: Sendable { case queued, sent, delivered, received, system }
    public enum BurnState: Sendable { case none, unopened, open, opened }
    public var id: String
    public var contactId: String
    /// Always empty for burn messages: their content lives only in the Rust shredding store.
    public var text: String
    public var mine: Bool
    public var timeMs: Int64
    public var status: Status
    public var file: FileAttachment?
    public var burn: BurnSpec?
    public var burnState: BurnState = .none
    public var burnAtMs: Int64?

    public var isViewOnceMedia: Bool {
        guard burn != nil, let f = file else { return false }
        return f.mime.hasPrefix("image/") || f.mime.hasPrefix("video/")
    }
    /// Burnable content can't be saved, forwarded, quoted, shared or searched (B2/B5).
    public var canShare: Bool { burn == nil }
    /// RS1 notice: when the contact restored (0 = unknown).
    public var restoredAtMs: Int64? {
        guard status == .system, text.hasPrefix("restored:") else { return nil }
        return Int64(text.dropFirst("restored:".count))
    }
}

/// Something that raises local notifications (UNUserNotificationCenter on iOS).
public protocol Notifier: AnyObject {
    func post(_ content: NotificationContent, id: String)
}

/// Something that can push queued messages out now (the BLE mesh transport).
public protocol MeshKick: AnyObject {
    func kick(_ contactId: String)
}

/// App state + core glue, the iOS twin of Android `MeshRepository`. Not thread-safe:
/// call from the main actor.
public final class MeshRepository {
    public let core: OmcpCore
    public let store: AppStore
    public weak var kick: MeshKick?
    public var notifier: Notifier?
    public var fileSink: FileSink?
    /// Encrypted chat backup (iCloud / Drive app-private space); nil when not configured.
    public var chatBackup: ChatBackupSync?
    /// Partners asking us to resend media we no longer have.
    public private(set) var resendAsks: [MediaRequestInfo] = []
    /// Set after a restore on this phone: "You restored this account…" banner (§5.98).
    public private(set) var restoredAtMs: Int64?
    public var onChange: (() -> Void)?
    public var genericBody = "New message"
    private let clock: () -> Int64
    /// Encrypted local store (Rust core). nil = everything in memory (tests, previews).
    private let dbPath: String?
    public private(set) var messages: [String: [ChatMessage]] = [:]

    public init(core: OmcpCore, store: AppStore, dbPath: String? = nil,
                clock: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1000) }) {
        self.core = core
        self.store = store
        self.dbPath = dbPath
        self.clock = clock
    }

    public func now() -> Int64 { clock() }

    /// Load identity + contacts from the store on launch. Returns false if there is no identity yet.
    @discardableResult public func restore() -> Bool {
        guard let phrase = store.recoveryPhrase else { return false }
        guard (try? loadCore(phrase: phrase, name: store.displayName ?? "Me")) != nil else { return false }
        for c in store.contacts { try? core.restoreContact(c) }
        // Contacts used to live in plain UserDefaults: once in the encrypted store, drop them.
        if dbPath != nil, !store.contacts.isEmpty { store.contacts = [] }
        loadMessages()
        restoredAtMs = core.deviceInfo()?.restoredAtMs
        return true
    }

    private func loadCore(phrase: String, name: String) throws {
        guard let path = dbPath else {
            try core.load(phrase: phrase, displayName: name)
            return
        }
        let key: Data
        if let k = store.storageKey { key = k } else {
            key = try core.generateStorageKey()
            store.storageKey = key
        }
        do {
            try core.open(phrase: phrase, displayName: name, dbPath: path, storageKey: key)
        } catch CoreError.storageUnavailable {
            // Unreadable with this key (Keychain reset, other identity, newer app): its data is
            // unrecoverable here. Keep the file aside and start a fresh store rather than lock out.
            let fm = FileManager.default
            try? fm.moveItem(atPath: path, toPath: "\(path).unreadable-\(clock())")
            try? fm.removeItem(atPath: "\(path)-journal")
            try core.open(phrase: phrase, displayName: name, dbPath: path, storageKey: key)
        }
    }

    /// Reload chats from the core's encrypted store (after a restart or a restore).
    private func loadMessages() {
        var all: [String: [ChatMessage]] = [:]
        for c in core.contacts() {
            let ms = core.storedMessages(c.id).map(Self.chatMessage)
            if !ms.isEmpty { all[c.id] = ms }
        }
        messages = all
    }

    private static func chatMessage(_ m: StoredMessageInfo) -> ChatMessage {
        var file = m.file.map {
            FileAttachment(name: $0.name, sizeBytes: $0.size, mime: $0.mime, state: $0.state, doneBytes: $0.doneBytes, localPath: $0.localPath)
        }
        if file == nil, let mime = m.mime {
            file = FileAttachment(name: mime.hasPrefix("video/") ? "video" : "photo", sizeBytes: 0, mime: mime, state: .received)
        }
        return ChatMessage(id: m.id, contactId: m.contactId, text: m.text, mine: m.mine, timeMs: m.timeMs, status: m.status,
                           file: file, burn: m.burn, burnState: m.burnState, burnAtMs: m.burnAtMs)
    }

    public func saveIdentity(phrase: String, name: String) throws {
        try loadCore(phrase: phrase, name: name)
        store.recoveryPhrase = phrase
        store.displayName = name
    }

    public func restoreFromBackup(_ r: RestoredBackup, name: String, method: String) throws {
        try saveIdentity(phrase: r.phrase, name: name)
        for c in r.contacts { try core.restoreContact(c) }
        // Legacy blobs only: history now lives in the separate chat backup.
        if let h = r.history { try core.restoreHistory(h) }
        markRestored()
        store.backupMethods.insert(method)
        persistContacts()
        loadMessages()
        changed()
    }

    /// 24-word restore with no cloud blob: identity + new device key (RS1).
    public func restoreFromPhrase(_ phrase: String, name: String) throws {
        try saveIdentity(phrase: phrase, name: name)
        markRestored()
        changed()
    }

    private func markRestored() {
        restoredAtMs = (try? core.markRestored(nowMs: clock()))??.restoredAtMs
    }

    /// Restore chat text (+ backed-up media) from the chat backup. Blocking (network).
    @discardableResult public func restoreChats() throws -> ChatRestoreResult? {
        guard let sync = chatBackup else { return nil }
        let sink = fileSink
        let r = try sync.restore { msgId, mime, bytes in
            sink?.save(ReceivedFileInfo(transferId: msgId, contactId: "", name: "media", mime: mime, bytes: bytes))
        }
        loadMessages()
        changed()
        return r
    }

    // MARK: missing media (CB3) + restore notices (RS1)

    public func onRestoreNotice(_ n: RestoreNoticeInfo) {
        append(ChatMessage(id: n.msgId, contactId: n.contactId, text: "restored:\(n.restoredAtMs ?? 0)", mine: false,
                           timeMs: clock(), status: .system))
    }

    public func onMediaRequest(_ r: MediaRequestInfo) {
        let bytes = r.localPath.flatMap { FileManager.default.contents(atPath: $0) }
        try? core.answerMediaRequest(contactId: r.contactId, msgId: r.msgId, bytes: bytes)
        kick?.kick(r.contactId)
    }

    public func onResendAsk(_ r: MediaRequestInfo) {
        resendAsks.removeAll { $0.msgId == r.msgId }
        resendAsks.append(r)
        changed()
    }

    public func dismissResendAsk(_ msgId: String) {
        resendAsks.removeAll { $0.msgId == msgId }
        changed()
    }

    public func onMediaUpdate(_ u: MediaUpdateInfo) {
        guard var m = find(u.contactId, u.msgId), var f = m.file, f.isPlaceholder else { return }
        f.state = u.state
        m.file = f
        replace(m)
    }

    /// Tap on a placeholder: request again now.
    public func requestMedia(_ contactId: String, msgId: String) {
        guard var m = find(contactId, msgId), var f = m.file, f.state == .missing else { return }
        try? core.requestMedia(msgId)
        f.state = .requested
        m.file = f
        replace(m)
        kick?.kick(contactId)
    }

    /// "Ask to resend" after the confirm sheet: ONE request carrying the optional note.
    public func askToResend(_ contactId: String, msgId: String, note: String?) throws {
        try core.askToResend(msgId, note: Self.resendNote(note))
        kick?.kick(contactId)
    }

    /// Trimmed note, nil when empty, cut to ≤ 280 UTF-8 bytes on a character boundary.
    public static func resendNote(_ note: String?) -> String? {
        guard var n = note?.trimmingCharacters(in: .whitespacesAndNewlines), !n.isEmpty else { return nil }
        while n.utf8.count > 280 { n.removeLast() }
        return n
    }

    public func waitingMedia(_ contactId: String) -> Int { messages[contactId]?.filter { $0.file?.isPlaceholder == true }.count ?? 0 }

    public var contacts: [Contact] { core.contacts() }
    public func contact(_ id: String) -> Contact? { contacts.first { $0.id == id } }

    public func backupGate() -> BackupGate { HoplaKit.backupGate(store, now: clock()) }

    public func remindBackupLater() {
        if store.backupRemindDeadlineMs == 0 { store.backupRemindDeadlineMs = clock() + remindWindowMs }
    }

    public func markBackupMethod(_ m: String, add: Bool = true) {
        if add { store.backupMethods.insert(m) } else { store.backupMethods.remove(m) }
        changed()
    }

    public func completeOnboarding() {
        store.onboarded = true
        changed()
    }

    // MARK: contacts

    /// "Verify & chat" → verified = true; "Chat without verifying" → false (no badge).
    @discardableResult
    public func addContact(qr: String, action: ContactAddAction) throws -> Contact {
        let known = (try? core.previewContact(qr).alreadyKnown) ?? false
        if known && action == .verifyAndChat {
            // Re-verify in person: identity + *current* device key (RS2). Throws on a stale QR.
            let c = try core.verifyContactFromQr(qr)
            persistContacts()
            return c
        }
        let c = try core.addContactFromQr(qr)
        if action == .verifyAndChat { _ = try core.verifyContactFromQr(qr) }
        persistContacts()
        return contact(c.id) ?? c
    }

    public func setVerified(_ id: String, _ v: Bool) throws {
        try core.setVerified(id, v)
        persistContacts()
    }

    private func persistContacts() {
        // With a local store the Rust core persists contacts (encrypted); defaults only without one.
        if dbPath == nil { store.contacts = core.contacts() }
        changed()
    }

    // MARK: burn settings

    public func chatBurn(_ contactId: String) -> BurnSpec? {
        BurnSpec.fromOption(store.burnChats[contactId] ?? -1, ttl: store.burnUnreadTtl[contactId] ?? BurnSpec.unread7d)
    }

    public func setChatBurn(_ contactId: String, option: Int, unreadTtl: Int = BurnSpec.unread7d) throws {
        if let s = BurnSpec.fromOption(option, ttl: unreadTtl), !core.burnSpecValid(s) { throw CoreError.invalid("burn option") }
        store.burnChats[contactId] = option == -1 ? nil : option
        store.burnUnreadTtl[contactId] = unreadTtl
        changed()
    }

    // MARK: messages

    /// Send text. `burnOnce` = the composer's per-message 🔥 toggle; otherwise the chat default.
    public func send(_ contactId: String, text: String, burnOnce: BurnSpec? = nil) throws {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return }
        let burn = burnOnce ?? chatBurn(contactId)
        let id = try core.queueMessage(contactId, text: t, burn: burn)
        append(ChatMessage(id: id, contactId: contactId, text: burn == nil ? t : "", mine: true, timeMs: clock(),
                           status: .queued, burn: burn, burnState: burn == nil ? .none : .unopened))
        kick?.kick(contactId)
    }

    /// Check a picked file against the core's cap (50 MB) before reading or encrypting it.
    public func checkFile(size: UInt64) throws {
        if case .tooLarge(let max) = FilePolicy.check(size: size, core: core) {
            throw CoreError.fileTooLarge(size: size, max: max)
        }
    }

    /// Send a file: Rust encrypts it into 64 KiB chunks; the BLE transport streams them when a
    /// link to `contactId` is up. Returns the transfer id.
    @discardableResult
    public func sendFile(_ contactId: String, name: String, mime: String, bytes: Data) throws -> String {
        try checkFile(size: UInt64(bytes.count))
        let o = try core.queueFile(contactId, name: name, mime: mime, bytes: bytes)
        append(ChatMessage(id: o.transferId, contactId: contactId, text: "", mine: true, timeMs: clock(), status: .queued,
                           file: FileAttachment(name: name, sizeBytes: o.size, mime: mime, state: .queued)))
        kick?.kick(contactId)
        return o.transferId
    }

    /// Cancel an outgoing (queued/sending) or incoming transfer and drop its bubble.
    public func cancelFile(_ contactId: String, transferId: String) {
        core.cancelFile(transferId)
        messages[contactId]?.removeAll { $0.id == transferId }
        changed()
    }

    /// From the link: chunks sealed (outgoing) or received (incoming).
    public func onFileProgress(_ p: FileProgressInfo) {
        guard let m = find(p.contactId, p.transferId) else {
            guard p.incoming else { return } // cancelled locally
            append(ChatMessage(id: p.transferId, contactId: p.contactId, text: "", mine: false, timeMs: clock(), status: .received,
                               file: FileAttachment(name: p.name, sizeBytes: p.size, mime: "", state: .receiving, doneBytes: p.doneBytes)))
            return
        }
        guard var f = m.file, f.state != .received, f.state != .integrityFailed else { return }
        let done = p.doneChunks >= p.totalChunks
        f.state = p.incoming ? .receiving : (done ? .sent : .sending)
        f.doneBytes = p.doneBytes
        var n = m
        n.file = f
        if !p.incoming && done { n.status = .sent }
        replace(n)
    }

    /// From the link: a file arrived and its BLAKE3 hash matched (checked in Rust).
    public func onFileReceived(_ r: ReceivedFileInfo) {
        let path = fileSink?.save(r)
        if fileSink != nil { try? core.setReceivedFilePath(r.transferId, path: path) }
        let ok = path != nil || fileSink == nil
        let f = FileAttachment(name: r.name, sizeBytes: UInt64(r.bytes.count), mime: r.mime, state: ok ? .received : .failed,
                               doneBytes: UInt64(r.bytes.count), localPath: path)
        if var m = find(r.contactId, r.transferId) {
            m.file = f
            replace(m)
        } else {
            append(ChatMessage(id: r.transferId, contactId: r.contactId, text: "", mine: false, timeMs: clock(), status: .received, file: f))
        }
    }

    public func onFileFailed(_ e: FileFailureInfo) {
        guard var m = find(e.contactId, e.transferId), var f = m.file else { return }
        f.state = e.reason == "integrity" ? .integrityFailed : .failed
        m.file = f
        replace(m)
    }

    /// Link dropped mid-transfer: transfers wait for the next link and resume automatically.
    public func pauseTransfers(_ contactId: String) {
        messages[contactId] = messages[contactId]?.map { m in
            guard var f = m.file, f.state == .sending || f.state == .receiving else { return m }
            f.state = .paused
            var n = m
            n.file = f
            return n
        }
        changed()
    }

    /// Apply every file event from one link step.
    public func applyFileEvents(_ r: LinkResult) {
        r.fileProgress.forEach(onFileProgress)
        r.files.forEach(onFileReceived)
        r.fileFailures.forEach(onFileFailed)
        r.restoreNotices.forEach(onRestoreNotice)
        r.mediaRequests.forEach(onMediaRequest)
        r.resendAsks.forEach(onResendAsk)
        r.mediaUpdates.forEach(onMediaUpdate)
        notifyLiveStarts()
    }

    /// Community lives announced since the last step: generic lock-screen notification.
    public func notifyLiveStarts() {
        for (sid, _) in core.liveTakeStarted() {
            notifier?.post(NotificationPolicy.liveStartedContent(store, genericBody: genericBody), id: "live-" + sid)
        }
    }

    /// Mesh status changes (handed to a relay → sent, ack → delivered); never goes backwards.
    public func applyStatusUpdates(_ updates: [MeshStatusUpdate]) {
        guard !updates.isEmpty else { return }
        func rank(_ s: ChatMessage.Status) -> Int {
            switch s {
            case .queued: return 0
            case .sent: return 1
            case .delivered: return 2
            case .received, .system: return 3
            }
        }
        for u in updates {
            messages[u.contactId] = messages[u.contactId]?.map { m in
                guard m.id == u.msgId, m.mine, rank(u.status) > rank(m.status) else { return m }
                var n = m
                n.status = u.status
                return n
            }
        }
        changed()
    }

    public func markFlushed(_ contactId: String, count: Int) {
        guard count > 0 else { return }
        var left = count
        messages[contactId] = messages[contactId]?.map { m in
            guard left > 0, m.mine, m.file == nil, m.status == .queued else { return m }
            left -= 1
            var n = m
            n.status = .sent
            return n
        }
        changed()
    }

    /// Mesh/local delivery: store and notify on-device (sender + preview unless private/burn).
    public func onReceived(_ msg: IncomingText) {
        let file = msg.mime.map { FileAttachment(name: $0.hasPrefix("video/") ? "video" : "photo", sizeBytes: 0, mime: $0, state: .received) }
        append(ChatMessage(id: msg.envelopeId, contactId: msg.contactId, text: msg.burn == nil ? msg.text : "", mine: false,
                           timeMs: msg.createdMs, status: .received, file: file, burn: msg.burn,
                           burnState: msg.burn == nil ? .none : .unopened))
        let sender = contact(msg.contactId)?.displayName ?? "?"
        let content = NotificationPolicy.content(store, chatId: msg.contactId, sender: sender, text: msg.text,
                                                 genericBody: genericBody, burn: msg.burn != nil)
        notifier?.post(content, id: msg.envelopeId)
    }

    public func openBurn(_ contactId: String, msgId: String) throws -> BurnContent? {
        guard let m = find(contactId, msgId), m.burn != nil, !m.mine, m.burnState == .unopened else { return nil }
        guard let c = try core.openBurn(msgId, nowMs: clock()) else { return nil }
        var n = m
        n.burnState = .open
        n.burnAtMs = c.burnAtMs
        replace(n)
        kick?.kick(contactId) // deliver the OPENED receipt
        return c
    }

    public func closeBurn(_ contactId: String, msgId: String) throws {
        guard let m = find(contactId, msgId), m.burnState == .open else { return }
        try core.closeBurn(msgId)
        burnNow(m)
    }

    public func onOpened(_ o: OpenedInfo) {
        guard let m = find(o.contactId, o.msgId) else { return }
        if o.shredded { burnNow(m) } else {
            var n = m
            n.burnState = .open
            n.burnAtMs = o.burnAtMs
            replace(n)
        }
    }

    /// Timers + unread expiry, decided in Rust. Call every ≤ 250 ms. Returns how many burned.
    @discardableResult public func tickBurn() -> Int {
        let ids = Set(core.tickBurn(nowMs: clock()))
        let due = messages.values.flatMap { $0 }.filter { ids.contains($0.id) && $0.burnState != .opened }
        due.forEach(burnNow)
        return due.count
    }

    /// Search index: burnable content is never indexed (B5).
    public func search(_ q: String) -> [ChatMessage] {
        let q = q.lowercased()
        guard !q.isEmpty else { return [] }
        return messages.values.flatMap { $0 }.filter { $0.burn == nil && $0.text.lowercased().contains(q) }
    }

    private func burnNow(_ m: ChatMessage) {
        if m.isViewOnceMedia {
            var n = m
            n.burnState = .opened // "🔥 Photo opened"
            n.burnAtMs = nil
            replace(n)
        } else {
            messages[m.contactId]?.removeAll { $0.id == m.id }
            changed()
        }
    }

    private func find(_ contactId: String, _ id: String) -> ChatMessage? { messages[contactId]?.first { $0.id == id } }

    private func replace(_ m: ChatMessage) {
        messages[m.contactId] = messages[m.contactId]?.map { $0.id == m.id ? m : $0 }
        changed()
    }

    private func append(_ m: ChatMessage) {
        messages[m.contactId, default: []].append(m)
        changed()
    }

    private func changed() { onChange?() }
}

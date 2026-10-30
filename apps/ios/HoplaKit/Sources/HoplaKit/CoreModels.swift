import Foundation

/// Aggregate path for chips/banners: mesh / gateway / mixed / queued only.
public enum PathKind: String, Sendable { case mesh, gateway, mixed, queued }

public struct MeshStatus: Equatable, Sendable {
    public var nearbyPeers: Int
    public var queuedCount: Int
    public var aggregatePath: PathKind
    public init(nearbyPeers: Int, queuedCount: Int, aggregatePath: PathKind) {
        self.nearbyPeers = nearbyPeers
        self.queuedCount = queuedCount
        self.aggregatePath = aggregatePath
    }
}

public struct Identity: Equatable, Sendable {
    public var peerIdHex: String
    public var displayName: String
    public init(peerIdHex: String, displayName: String) {
        self.peerIdHex = peerIdHex
        self.displayName = displayName
    }
}

public struct Contact: Equatable, Identifiable, Codable, Sendable {
    public var id: String
    public var displayName: String
    /// 12 groups of 5 digits.
    public var safetyNumber: [String]
    public var verified: Bool
    /// RS1: the "Restored" emphasis window (7 days) after the contact restored on a new device.
    public var restoredBadge: Bool = false
    public var restoredAtMs: Int64?
    /// RS2: restored and not re-verified in person yet: "Restored, not verified" pill (§5.98).
    public var restoredUnverified: Bool = false
    /// Their device's X25519 key from an identity backup (v0.1.4a); nil for legacy contacts.
    public var dhHex: String?
    public init(id: String, displayName: String, safetyNumber: [String], verified: Bool,
                restoredBadge: Bool = false, restoredAtMs: Int64? = nil, restoredUnverified: Bool = false,
                dhHex: String? = nil) {
        self.dhHex = dhHex
        self.id = id
        self.displayName = displayName
        self.safetyNumber = safetyNumber
        self.verified = verified
        self.restoredBadge = restoredBadge
        self.restoredAtMs = restoredAtMs
        self.restoredUnverified = restoredUnverified
    }

    /// Verified check badge only on verified contacts; unverified contacts show nothing.
    public var showsVerifiedBadge: Bool { verified }
}

public struct ContactPreview: Equatable, Sendable {
    public var contact: Contact
    public var alreadyKnown: Bool
    public var mismatchWith: Contact?
    public init(contact: Contact, alreadyKnown: Bool, mismatchWith: Contact?) {
        self.contact = contact
        self.alreadyKnown = alreadyKnown
        self.mismatchWith = mismatchWith
    }
}

/// UI-side value for the encrypted `burn` header (PROTOCOL §19.1). All burn rules live in Rust.
public struct BurnSpec: Equatable, Codable, Sendable {
    public enum Mode: String, Codable, Sendable { case onOpen, timer }
    public var mode: Mode
    public var secs: Int
    public var unreadTtlSecs: Int

    public static let unread1h = 3_600
    public static let unread24h = 86_400
    public static let unread7d = 604_800

    public init(mode: Mode, secs: Int, unreadTtlSecs: Int = BurnSpec.unread7d) {
        self.mode = mode
        self.secs = secs
        self.unreadTtlSecs = unreadTtlSecs
    }
    public static func onOpen(ttl: Int = unread7d) -> BurnSpec { BurnSpec(mode: .onOpen, secs: 0, unreadTtlSecs: ttl) }
    public static func timer(_ secs: Int, ttl: Int = unread7d) -> BurnSpec { BurnSpec(mode: .timer, secs: secs, unreadTtlSecs: ttl) }

    /// Per-chat option encoding (same as Android): -1 off, 0 on open, else timer secs.
    public static func fromOption(_ opt: Int, ttl: Int) -> BurnSpec? {
        switch opt {
        case -1: return nil
        case 0: return onOpen(ttl: ttl)
        default: return timer(opt, ttl: ttl)
        }
    }
    /// Chat info choices: on open, 10 s, 1 min, 1 h, off.
    public static let chatOptions = [0, 10, 60, 3_600, -1]
}

/// Burn content handed out exactly once by the core.
public struct BurnContent: Equatable, Sendable {
    public var text: String
    public var bytes: Data
    public var mime: String?
    public var burnAtMs: Int64?
    public init(text: String, bytes: Data, mime: String?, burnAtMs: Int64?) {
        self.text = text
        self.bytes = bytes
        self.mime = mime
        self.burnAtMs = burnAtMs
    }
}

/// Sender side: the peer opened our burn message (encrypted OPENED receipt).
public struct OpenedInfo: Equatable, Sendable {
    public var contactId: String
    public var msgId: String
    public var openedMs: Int64
    public var shredded: Bool
    public var burnAtMs: Int64?
    public init(contactId: String, msgId: String, openedMs: Int64, shredded: Bool, burnAtMs: Int64?) {
        self.contactId = contactId
        self.msgId = msgId
        self.openedMs = openedMs
        self.shredded = shredded
        self.burnAtMs = burnAtMs
    }
}

public struct IncomingText: Equatable, Sendable {
    public var contactId: String
    /// Empty for burn messages: content only via `OmcpCore.openBurn`.
    public var text: String
    public var createdMs: Int64
    public var envelopeId: String
    public var burn: BurnSpec?
    public var mime: String?
    public init(contactId: String, text: String, createdMs: Int64, envelopeId: String, burn: BurnSpec? = nil, mime: String? = nil) {
        self.contactId = contactId
        self.text = text
        self.createdMs = createdMs
        self.envelopeId = envelopeId
        self.burn = burn
        self.mime = mime
    }
}

/// A queued outbound file (encrypted into 64 KiB chunks by the Rust core).
public struct FileOfferInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var name: String
    public var mime: String
    public var size: UInt64
    public var chunks: Int
    public init(transferId: String, contactId: String, name: String, mime: String, size: UInt64, chunks: Int) {
        self.transferId = transferId
        self.contactId = contactId
        self.name = name
        self.mime = mime
        self.size = size
        self.chunks = chunks
    }
}

/// Transfer progress: sender = chunks sealed onto the link, receiver = chunks received.
public struct FileProgressInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var incoming: Bool
    public var name: String
    public var size: UInt64
    public var doneChunks: Int
    public var totalChunks: Int
    public init(transferId: String, contactId: String, incoming: Bool, name: String, size: UInt64, doneChunks: Int, totalChunks: Int) {
        self.transferId = transferId
        self.contactId = contactId
        self.incoming = incoming
        self.name = name
        self.size = size
        self.doneChunks = doneChunks
        self.totalChunks = totalChunks
    }
    public var fraction: Double { totalChunks == 0 ? 1 : Double(doneChunks) / Double(totalChunks) }
    public var doneBytes: UInt64 { UInt64(Double(size) * fraction) }
}

/// A fully received file whose BLAKE3 hash was verified in Rust.
public struct ReceivedFileInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var name: String
    public var mime: String
    public var bytes: Data
    public init(transferId: String, contactId: String, name: String, mime: String, bytes: Data) {
        self.transferId = transferId
        self.contactId = contactId
        self.name = name
        self.mime = mime
        self.bytes = bytes
    }
}

/// `integrity` (hash/AEAD failed), `invalid` or `busy`.
public struct FileFailureInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var reason: String
    public init(transferId: String, contactId: String, reason: String) {
        self.transferId = transferId
        self.contactId = contactId
        self.reason = reason
    }
}

/// PDUs to transmit (in order) + decrypted messages from one link step.
public struct LinkResult: Sendable {
    public var outgoing: [Data]
    public var received: [IncomingText]
    public var established: Bool
    public var peerContactId: String?
    public var flushed: Int
    public var opened: [OpenedInfo]
    public var fileProgress: [FileProgressInfo]
    public var files: [ReceivedFileInfo]
    public var fileFailures: [FileFailureInfo]
    /// More file chunks wait: call `flush()` again once the BLE write/notify queue is empty.
    public var morePending: Bool
    /// RS1: contacts now on a new (restored) device; the core already stored the system notice.
    public var restoreNotices: [RestoreNoticeInfo] = []
    /// CB3: partners asking for media again (answer with `answerMediaRequest`).
    public var mediaRequests: [MediaRequestInfo] = []
    /// CB3: partners asking us to resend media they no longer have.
    public var resendAsks: [MediaRequestInfo] = []
    /// CB3: placeholder state changes (requested / unavailable).
    public var mediaUpdates: [MediaUpdateInfo] = []
    /// Other links (by `CoreLink.handle`) with mesh frames waiting: flush them (multi-hop relay, §22).
    public var relayLinks: [UInt64] = []
    public init(outgoing: [Data], received: [IncomingText], established: Bool, peerContactId: String?, flushed: Int,
                opened: [OpenedInfo] = [], fileProgress: [FileProgressInfo] = [], files: [ReceivedFileInfo] = [],
                fileFailures: [FileFailureInfo] = [], morePending: Bool = false,
                restoreNotices: [RestoreNoticeInfo] = [], mediaRequests: [MediaRequestInfo] = [],
                resendAsks: [MediaRequestInfo] = [], mediaUpdates: [MediaUpdateInfo] = [],
                relayLinks: [UInt64] = []) {
        self.relayLinks = relayLinks
        self.restoreNotices = restoreNotices
        self.mediaRequests = mediaRequests
        self.resendAsks = resendAsks
        self.mediaUpdates = mediaUpdates
        self.outgoing = outgoing
        self.received = received
        self.established = established
        self.peerContactId = peerContactId
        self.flushed = flushed
        self.opened = opened
        self.fileProgress = fileProgress
        self.files = files
        self.fileFailures = fileFailures
        self.morePending = morePending
    }
}

public enum NetworkType: Sendable { case offline, wifi, cellular }

public struct GatewayDecision: Equatable, Sendable {
    public var bridge: Bool
    public var meteredWarning: Bool
    public init(bridge: Bool, meteredWarning: Bool) {
        self.bridge = bridge
        self.meteredWarning = meteredWarning
    }
}

public struct PasswordCheck: Equatable, Sendable {
    public var ok: Bool
    public var score: Int
    public var tooShort: Bool
    public init(ok: Bool, score: Int, tooShort: Bool) {
        self.ok = ok
        self.score = score
        self.tooShort = tooShort
    }
}

public struct BackupMeta: Equatable, Sendable {
    public var createdAtMs: Int64
    public var deviceHint: String
    public var needsPassword: Bool
    public init(createdAtMs: Int64, deviceHint: String, needsPassword: Bool) {
        self.createdAtMs = createdAtMs
        self.deviceHint = deviceHint
        self.needsPassword = needsPassword
    }
}

public struct RestoredBackup: Equatable, Sendable {
    public var phrase: String
    public var contacts: [Contact]
    /// Chats, messages, file metadata and pending queue (never burn content); nil for older backups.
    public var history: Data?
    public init(phrase: String, contacts: [Contact], history: Data? = nil) {
        self.phrase = phrase
        self.contacts = contacts
        self.history = history
    }
}

/// File-transfer metadata as persisted by the Rust local store.
public struct StoredFileInfo: Equatable, Sendable {
    public var name: String
    public var mime: String
    public var size: UInt64
    public var state: FileAttachment.State
    public var doneBytes: UInt64
    public var localPath: String?
    public init(name: String, mime: String, size: UInt64, state: FileAttachment.State, doneBytes: UInt64, localPath: String?) {
        self.name = name
        self.mime = mime
        self.size = size
        self.state = state
        self.doneBytes = doneBytes
        self.localPath = localPath
    }
}

/// A message as persisted by the Rust local store. Burn messages never carry text here.
public struct StoredMessageInfo: Equatable, Sendable {
    public var id: String
    public var contactId: String
    public var text: String
    public var mine: Bool
    public var timeMs: Int64
    public var status: ChatMessage.Status
    public var mime: String?
    public var burn: BurnSpec?
    public var burnState: ChatMessage.BurnState
    public var burnAtMs: Int64?
    public var file: StoredFileInfo?
    public init(id: String, contactId: String, text: String, mine: Bool, timeMs: Int64, status: ChatMessage.Status,
                mime: String?, burn: BurnSpec?, burnState: ChatMessage.BurnState, burnAtMs: Int64?, file: StoredFileInfo?) {
        self.id = id
        self.contactId = contactId
        self.text = text
        self.mine = mine
        self.timeMs = timeMs
        self.status = status
        self.mime = mime
        self.burn = burn
        self.burnState = burnState
        self.burnAtMs = burnAtMs
        self.file = file
    }
}

public enum CoreError: Error, Equatable {
    case wrongBackupPassword
    case invalid(String)
    case fileTooLarge(size: UInt64, max: UInt64)
    case notLoaded
    /// The encrypted local store can't be opened (wrong/lost storage key, other identity, newer schema).
    case storageUnavailable(String)
    /// RS2: the QR isn't the contact's current identity + device key (e.g. a pre-restore code).
    case verificationFailed
    case other(String)
}

// MARK: encrypted chat backup + restore indicator (CB1–CB5, RS1–RS2)

public struct RestoreNoticeInfo: Equatable, Sendable {
    public var contactId: String
    public var restoredAtMs: Int64?
    public var msgId: String
    public init(contactId: String, restoredAtMs: Int64?, msgId: String) {
        self.contactId = contactId; self.restoredAtMs = restoredAtMs; self.msgId = msgId
    }
}

public struct MediaRequestInfo: Equatable, Sendable {
    public var contactId: String
    public var msgId: String
    public var hash: String
    public var name: String
    public var mime: String
    public var localPath: String?
    /// "Ask to resend" only: the requester's optional note, carried in the same message.
    public var note: String?
    public init(contactId: String, msgId: String, hash: String, name: String, mime: String, localPath: String?, note: String? = nil) {
        self.contactId = contactId; self.msgId = msgId; self.hash = hash; self.name = name; self.mime = mime; self.localPath = localPath
        self.note = note
    }
}

/// Settings > Storage > Keep sent media (defaults 90 days / 2 GB, oldest deleted first).
public struct KeptMediaPrefs: Equatable, Sendable {
    public var maxDays: Int
    public var maxBytes: Int64
    public init(maxDays: Int = 90, maxBytes: Int64 = 2_000_000_000) { self.maxDays = maxDays; self.maxBytes = maxBytes }
}

public struct KeptMediaUsage: Equatable, Sendable {
    public var count: Int
    public var bytes: Int64
    public init(count: Int, bytes: Int64) { self.count = count; self.bytes = bytes }
}

/// Choices and formatting for the Storage screen (decimal units, like the 50 MB file cap).
public enum StorageSettings {
    public static let dayOptions = [7, 30, 90, 180, 365]
    public static let sizeOptions: [Int64] = [500_000_000, 1_000_000_000, 2_000_000_000, 5_000_000_000, 10_000_000_000]
    public static func formatBytes(_ b: Int64) -> String {
        func t(_ v: Double) -> String { v == v.rounded() ? String(Int64(v)) : String(format: "%.1f", v) }
        if b >= 1_000_000_000 { return t(Double(b) / 1e9) + " GB" }
        if b >= 1_000_000 { return t(Double(b) / 1e6) + " MB" }
        if b >= 1_000 { return t(Double(b) / 1e3) + " KB" }
        return "\(b) B"
    }
}

public struct MediaUpdateInfo: Equatable, Sendable {
    public var contactId: String
    public var msgId: String
    public var state: FileAttachment.State
    public init(contactId: String, msgId: String, state: FileAttachment.State) {
        self.contactId = contactId; self.msgId = msgId; self.state = state
    }
}

/// One encrypted, 64 KiB-padded chat-backup blob (kind 0 snapshot, 1 log, 2 media).
public struct ChatBackupBlob: Equatable, Sendable {
    public var name: String
    public var kind: Int
    public var seq: UInt64
    public var blob: Data
    public var deleteBelowSeq: UInt64?
    public init(name: String, kind: Int, seq: UInt64, blob: Data, deleteBelowSeq: UInt64?) {
        self.name = name; self.kind = kind; self.seq = seq; self.blob = blob; self.deleteBelowSeq = deleteBelowSeq
    }
}

public struct ChatBackupSummary: Equatable, Sendable {
    public var messages: Int
    public var mediaPlaceholders: Int
    public init(messages: Int, mediaPlaceholders: Int) { self.messages = messages; self.mediaPlaceholders = mediaPlaceholders }
}

/// Settings › Backup › "Also back up photos, videos and files" (CB5). Off by default; videos off.
public struct MediaBackupPrefs: Equatable, Sendable {
    public var enabled = false
    public var photos = true
    public var videos = false
    public var voice = true
    public var files = true
    public var mobileData = false
    public init(enabled: Bool = false, photos: Bool = true, videos: Bool = false, voice: Bool = true, files: Bool = true, mobileData: Bool = false) {
        self.enabled = enabled; self.photos = photos; self.videos = videos; self.voice = voice; self.files = files; self.mobileData = mobileData
    }
}

public struct MediaBackupItem: Equatable, Sendable {
    public var msgId: String
    public var hash: String
    public var mime: String
    public var localPath: String?
    public var blobName: String
    public init(msgId: String, hash: String, mime: String, localPath: String?, blobName: String) {
        self.msgId = msgId; self.hash = hash; self.mime = mime; self.localPath = localPath; self.blobName = blobName
    }
}

public struct DeviceKeyInfo: Equatable, Sendable {
    public var deviceKeyHex: String
    public var fingerprint: String
    public var restoredAtMs: Int64?
    public init(deviceKeyHex: String, fingerprint: String, restoredAtMs: Int64?) {
        self.deviceKeyHex = deviceKeyHex; self.fingerprint = fingerprint; self.restoredAtMs = restoredAtMs
    }
}

/// One BLE connection's secure session (Noise hybrid + M2 fragments).
public protocol CoreLink: AnyObject {
    func start() throws -> LinkResult
    func onPdu(_ pdu: Data) throws -> LinkResult
    func sendText(_ text: String) throws -> LinkResult
    func flush() throws -> LinkResult
    var isEstablished: Bool { get }
    func close()
    /// Opaque Rust link handle, matched against `LinkResult.relayLinks`.
    var handle: UInt64 { get }
}

public extension CoreLink {
    var handle: UInt64 { 0 }
}

/// Mesh relay counters from Rust. The relay is always on with an automatic cap
/// (REQUIREMENTS RL1, RL2); Settings shows only `storageUsedBytes` (RL4).
public struct MeshRelayStats: Equatable, Sendable {
    public var carried: Int
    public var carriedBytes: Int64
    public var ownPending: Int
    public var neighbours: Int
    /// RL4 "Storage used for relaying".
    public var storageUsedBytes: Int64
    /// RL2 automatic cap in force (500 MB unless storage is low).
    public var storageCapBytes: Int64
    public init(carried: Int = 0, carriedBytes: Int64 = 0, ownPending: Int = 0, neighbours: Int = 0,
                storageUsedBytes: Int64 = 0, storageCapBytes: Int64 = 500_000_000) {
        self.carried = carried
        self.carriedBytes = carriedBytes
        self.ownPending = ownPending
        self.neighbours = neighbours
        self.storageUsedBytes = storageUsedBytes
        self.storageCapBytes = storageCapBytes
    }
}

/// Mesh-caused status change (PROTOCOL §22.8(3)): `.sent` = handed to a relay,
/// `.delivered` = end-to-end ack received.
public struct MeshStatusUpdate: Equatable, Sendable {
    public var msgId: String
    public var contactId: String
    public var status: ChatMessage.Status
    public init(msgId: String, contactId: String, status: ChatMessage.Status) {
        self.msgId = msgId
        self.contactId = contactId
        self.status = status
    }
}

/// App-facing protocol core. Production: `RustOmcpCore` (omcp-ffi via UniFFI, in the app
/// target). Previews/tests: `FakeOmcpCore`. The UI layer never invents crypto results.
/// Hopla News story (REQUIREMENTS "Hopla News", v0.4). Read-only: users cannot publish.
public struct NewsStory: Equatable, Sendable, Identifiable {
    public var id: String
    public var topic: String
    public var title: String
    public var summary: String
    public var publisher: String
    public var sourceName: String
    public var link: String
    public var via: String?
    public var emergency: Bool
    public init(id: String, topic: String, title: String, summary: String, publisher: String, sourceName: String, link: String, via: String?, emergency: Bool) {
        self.id = id; self.topic = topic; self.title = title; self.summary = summary
        self.publisher = publisher; self.sourceName = sourceName; self.link = link; self.via = via; self.emergency = emergency
    }
}

/// What a phone may fetch now under the gateway rules (Rust `news_fetch_plan`).
public struct NewsFetchPlan: Equatable, Sendable {
    public var feeds: [String]
    public var meteredWarning: Bool
    public var blocked: String?
    public var usedTodayBytes: Int64
    public var dailyCapBytes: Int64
    public init(feeds: [String] = [], meteredWarning: Bool = false, blocked: String? = "offline", usedTodayBytes: Int64 = 0, dailyCapBytes: Int64 = 500_000_000) {
        self.feeds = feeds; self.meteredWarning = meteredWarning; self.blocked = blocked
        self.usedTodayBytes = usedTodayBytes; self.dailyCapBytes = dailyCapBytes
    }
}

/// News fetch rules shared by the app (mirrors Android `NewsViewModel.fetchNow`): only phones
/// allowed by the core's role flag (any phone with internet), then the gateway network rules
/// (Wi-Fi-only unless mobile data is allowed, metered warning, 500 MB/day cap).
public enum NewsFetchRules {
    public static func plan(core: OmcpCore, store: AppStore, network: NetworkType, metered: Bool, nowMs: Int64) -> NewsFetchPlan? {
        guard core.newsMayFetch(gatewayEnabled: store.gatewayEnabled) else { return nil }
        return core.newsFetchPlan(wifiOnly: !store.gatewayAllowMobile, network: network, metered: metered, nowMs: nowMs)
    }

    /// Run one round with a synchronous GET (tests, background tasks). Returns stories added.
    @discardableResult
    public static func run(core: OmcpCore, store: AppStore, network: NetworkType, metered: Bool, nowMs: Int64, get: (String) -> String?) -> Int {
        guard let p = plan(core: core, store: store, network: network, metered: metered, nowMs: nowMs) else { return 0 }
        var added = 0
        for url in p.feeds {
            guard let body = get(url) else { continue }
            added += (try? core.newsIngestRss(feedUrl: url, body: body, nowMs: nowMs)) ?? 0
        }
        return added
    }
}

/// News tab chips (design §5.108).
public let newsTabs = ["top", "local", "world", "politics", "tech", "sport", "health", "emergencies"]

public protocol OmcpCore: AnyObject {
    var linked: Bool { get }
    var suiteLabel: String { get }

    func generateRecoveryPhrase() throws -> String
    func validateRecoveryPhrase(_ phrase: String) -> Bool
    func suggestRecoveryWords(_ prefix: String) -> [String]

    @discardableResult func load(phrase: String, displayName: String) throws -> Identity
    /// Load the identity and open its encrypted local store at `dbPath` (contacts, chats,
    /// messages, file metadata, queue and burn state survive restarts). `storageKey`
    /// (32 bytes) lives in the iOS Keychain. Throws `CoreError.storageUnavailable`.
    @discardableResult func open(phrase: String, displayName: String, dbPath: String, storageKey: Data) throws -> Identity
    func generateStorageKey() throws -> Data
    /// Persisted messages of one chat, oldest first (empty for in-memory cores).
    func storedMessages(_ contactId: String) -> [StoredMessageInfo]
    /// Record where a verified received file was saved (nil = couldn't save).
    func setReceivedFilePath(_ transferId: String, path: String?) throws
    /// Import `RestoredBackup.history` into the local store.
    func restoreHistory(_ history: Data) throws

    // Encrypted chat backup (CB1–CB5): Rust seals, pads (64 KiB) and plans; apps move blobs.
    func chatBackupDue(nowMs: Int64, onWifi: Bool, allowMobile: Bool) -> Bool
    func chatBackupNext(nowMs: Int64, forceSnapshot: Bool) throws -> ChatBackupBlob?
    func chatBackupCommit(seq: UInt64) throws
    func chatBackupRestore(_ blobs: [Data]) throws -> ChatBackupSummary
    func mediaBackupSettings() -> MediaBackupPrefs
    /// Returns media blob names to delete (when turned off).
    func setMediaBackupSettings(_ p: MediaBackupPrefs) throws -> [String]
    func mediaBackupDue(nowMs: Int64, onWifi: Bool) -> Bool
    func mediaBackupTodo() -> [MediaBackupItem]
    func sealMediaBackup(hash: String, bytes: Data) throws -> ChatBackupBlob
    func mediaBackupCommit(hash: String, nowMs: Int64) throws
    func mediaPlaceholders() -> [MediaBackupItem]
    func restoreMediaFromBackup(msgId: String, blob: Data) throws -> Data
    // Missing media re-fetch (CB3).
    func requestMedia(_ msgId: String) throws
    /// ONE message carrying the optional note (≤ 280 UTF-8 bytes).
    func askToResend(_ msgId: String, note: String?) throws
    // Multi-hop mesh relay (PROTOCOL §22).
    func meshStats() -> MeshRelayStats
    func newsFeed(tab: String) -> [NewsStory]
    func newsCardText(id: String) -> String?
    func newsFetchPlan(wifiOnly: Bool, network: NetworkType, metered: Bool, nowMs: Int64) -> NewsFetchPlan
    func newsIngestRss(feedUrl: String, body: String, nowMs: Int64) throws -> Int
    /// Gateway-only vs any phone with internet: one flag in Rust (`NEWS_FETCH_GATEWAY_ONLY`).
    func newsMayFetch(gatewayEnabled: Bool) -> Bool
    /// Team moderation (PROTOCOL §23.9, TM1–TM4).
    func moderationHasTeam() -> Bool
    func moderationLog() -> [TeamLogEntryModel]
    /// Verify, apply and gossip a signed TAKEDOWN/RESTORE (hex); returns an outcome code.
    func moderationPublish(actionHex: String) -> String
    func moderationRemovedReason(communityIdHex: String) -> Int?
    /// News settings (§5.115) → Rust decides which channel tags to subscribe (persisted in the store).
    func newsSetPrefs(topics: [String], langs: [String], city: String?, muted: [String], alertsNotify: Bool)
    /// RL2: report free/total storage; under 10 % free Rust lowers the relay cap.
    func meshSetFreeStorage(freeBytes: Int64, totalBytes: Int64)
    /// Periodic relay work; returns link handles with frames waiting.
    func meshTick(nowMs: Int64) -> [UInt64]
    func meshPendingLinks() -> [UInt64]
    /// Status changes caused by the mesh since the last call (Queued → Sent → Delivered).
    func meshTakeStatusUpdates() -> [MeshStatusUpdate]
    /// 6-byte HELLO beacon (§22.7(12)).
    func meshBeacon() -> Data
    func meshMaxLinks() -> Int
    func keptMediaSettings() -> KeptMediaPrefs
    /// Applies at once; returns how many kept copies were evicted (oldest first).
    func setKeptMediaSettings(_ p: KeptMediaPrefs, nowMs: Int64) throws -> Int
    func keptMediaUsage() -> KeptMediaUsage
    func evictKeptMedia(nowMs: Int64) -> Int
    func answerMediaRequest(contactId: String, msgId: String, bytes: Data?) throws
    // Restore indicator (RS1/RS2).
    func markRestored(nowMs: Int64) throws -> DeviceKeyInfo?
    func deviceInfo() -> DeviceKeyInfo?
    /// Throws `CoreError.verificationFailed` on a stale or wrong code.
    func verifyContactFromQr(_ payload: String) throws -> Contact

    func identity() -> Identity?
    func qrPayload() throws -> String
    func advertisement() throws -> Data

    func previewContact(_ payload: String) throws -> ContactPreview
    func addContactFromQr(_ payload: String) throws -> Contact
    func restoreContact(_ contact: Contact) throws
    func contacts() -> [Contact]
    func setVerified(_ contactId: String, _ verified: Bool) throws
    func contactForAdvertisement(_ adv: Data) -> String?

    func queueMessage(_ contactId: String, text: String, burn: BurnSpec?) throws -> String
    func queueMedia(_ contactId: String, mime: String, bytes: Data, burn: BurnSpec?) throws -> String
    func openBurn(_ msgId: String, nowMs: Int64) throws -> BurnContent?
    func closeBurn(_ msgId: String) throws
    func tickBurn(nowMs: Int64) -> [String]
    func burnIsLive(_ msgId: String) -> Bool
    func burnSpecValid(_ spec: BurnSpec) -> Bool
    /// Queue a file (≤ `maxFileBytes()`, checked in Rust before encryption). Throws `.fileTooLarge`.
    func queueFile(_ contactId: String, name: String, mime: String, bytes: Data) throws -> FileOfferInfo
    @discardableResult func cancelFile(_ transferId: String) -> Bool
    func queuedFor(_ contactId: String) -> Int
    func pathStatus() -> MeshStatus

    func openInitiatorLink(_ contactId: String) throws -> CoreLink
    func openResponderLink() throws -> CoreLink

    func checkBackupPassword(_ password: String) -> PasswordCheck
    func createBackup(password: String, deviceHint: String, nowMs: Int64) throws -> Data
    /// iCloud "No password" mode: sealed under a 256-bit key kept in iCloud Keychain.
    func createBackup(key: Data, deviceHint: String, nowMs: Int64) throws -> Data
    func generateBackupKey() throws -> Data
    func backupInfo(_ blob: Data) -> BackupMeta?
    func openBackup(_ blob: Data, password: String) throws -> RestoredBackup
    func openBackup(_ blob: Data, key: Data) throws -> RestoredBackup
    func recoveryPhrase() throws -> String

    /// Core file cap (decimal bytes, 50 MB). UI must read this, never hard-code.
    func maxFileBytes() -> UInt64
    func gatewayDailyCapBytes() -> UInt64
    func gatewayDecision(wifiOnly: Bool, network: NetworkType, metered: Bool) -> GatewayDecision
}

/// In-memory defaults (fake core, previews): no local store.
public extension OmcpCore {
    @discardableResult func open(phrase: String, displayName: String, dbPath: String, storageKey: Data) throws -> Identity {
        try load(phrase: phrase, displayName: displayName)
    }
    func generateStorageKey() throws -> Data {
        var g = SystemRandomNumberGenerator()
        return Data((0..<32).map { _ in UInt8.random(in: 0...255, using: &g) })
    }
    func storedMessages(_ contactId: String) -> [StoredMessageInfo] { [] }
    func setReceivedFilePath(_ transferId: String, path: String?) throws {}
    func restoreHistory(_ history: Data) throws {}

    func chatBackupDue(nowMs: Int64, onWifi: Bool, allowMobile: Bool) -> Bool { false }
    func chatBackupNext(nowMs: Int64, forceSnapshot: Bool) throws -> ChatBackupBlob? { nil }
    func chatBackupCommit(seq: UInt64) throws {}
    func chatBackupRestore(_ blobs: [Data]) throws -> ChatBackupSummary { ChatBackupSummary(messages: 0, mediaPlaceholders: 0) }
    func mediaBackupSettings() -> MediaBackupPrefs { MediaBackupPrefs() }
    func setMediaBackupSettings(_ p: MediaBackupPrefs) throws -> [String] { [] }
    func mediaBackupDue(nowMs: Int64, onWifi: Bool) -> Bool { false }
    func mediaBackupTodo() -> [MediaBackupItem] { [] }
    func sealMediaBackup(hash: String, bytes: Data) throws -> ChatBackupBlob { throw CoreError.other("no media backup") }
    func mediaBackupCommit(hash: String, nowMs: Int64) throws {}
    func mediaPlaceholders() -> [MediaBackupItem] { [] }
    func restoreMediaFromBackup(msgId: String, blob: Data) throws -> Data { throw CoreError.other("no media backup") }
    func requestMedia(_ msgId: String) throws {}
    func askToResend(_ msgId: String, note: String?) throws {}
    func meshStats() -> MeshRelayStats { MeshRelayStats() }
    func newsFeed(tab: String) -> [NewsStory] { [] }
    func newsCardText(id: String) -> String? { nil }
    func newsFetchPlan(wifiOnly: Bool, network: NetworkType, metered: Bool, nowMs: Int64) -> NewsFetchPlan { NewsFetchPlan() }
    func newsIngestRss(feedUrl: String, body: String, nowMs: Int64) throws -> Int { 0 }
    func newsMayFetch(gatewayEnabled: Bool) -> Bool { true }
    func moderationHasTeam() -> Bool { false }
    func moderationLog() -> [TeamLogEntryModel] { [] }
    func moderationPublish(actionHex: String) -> String { "no_team" }
    func moderationRemovedReason(communityIdHex: String) -> Int? { nil }
    func newsSetPrefs(topics: [String], langs: [String], city: String?, muted: [String], alertsNotify: Bool) {}
    func meshSetFreeStorage(freeBytes: Int64, totalBytes: Int64) {}
    func meshTick(nowMs: Int64) -> [UInt64] { [] }
    func meshPendingLinks() -> [UInt64] { [] }
    func meshTakeStatusUpdates() -> [MeshStatusUpdate] { [] }
    func meshBeacon() -> Data { Data() }
    func meshMaxLinks() -> Int { 4 }
    func keptMediaSettings() -> KeptMediaPrefs { KeptMediaPrefs() }
    func setKeptMediaSettings(_ p: KeptMediaPrefs, nowMs: Int64) throws -> Int { 0 }
    func keptMediaUsage() -> KeptMediaUsage { KeptMediaUsage(count: 0, bytes: 0) }
    func evictKeptMedia(nowMs: Int64) -> Int { 0 }
    func answerMediaRequest(contactId: String, msgId: String, bytes: Data?) throws {}
    func markRestored(nowMs: Int64) throws -> DeviceKeyInfo? { nil }
    func deviceInfo() -> DeviceKeyInfo? { nil }
    func verifyContactFromQr(_ payload: String) throws -> Contact {
        let id = try previewContact(payload).contact.id
        try setVerified(id, true)
        if let c = contacts().first(where: { $0.id == id }) { return c }
        return try previewContact(payload).contact
    }
}

/// One line of the public "Removed communities" log (§23.9(5)).
public struct TeamLogEntryModel: Equatable, Identifiable {
    public var id: String { "\(communityIdHex)-\(epoch)" }
    public var kind: String          // "takedown" | "restore"
    public var communityIdHex: String
    public var reasonCode: Int       // 0 for restore
    public var note: String
    public var epoch: UInt64
    public var signedBy: [String]
    public var appliedMs: Int64
    public init(kind: String, communityIdHex: String, reasonCode: Int, note: String, epoch: UInt64, signedBy: [String], appliedMs: Int64) {
        self.kind = kind; self.communityIdHex = communityIdHex; self.reasonCode = reasonCode
        self.note = note; self.epoch = epoch; self.signedBy = signedBy; self.appliedMs = appliedMs
    }
}

package chat.omcp.app.core

/** Aggregate path for chips/banners — mesh / gateway / mixed / queued only. */
enum class PathKind { Mesh, Gateway, Mixed, Queued }

data class MeshStatus(val nearbyPeers: Int, val queuedCount: Int, val aggregatePath: PathKind)

data class Identity(val peerIdHex: String, val displayName: String)

data class Contact(
    val id: String,
    val displayName: String,
    /** 12 groups of 5 digits. */
    val safetyNumber: List<String>,
    val verified: Boolean,
    /** "Restored" badge: the contact restored Hopla on a new device in the last 7 days (RS1). */
    val restoredBadge: Boolean = false,
    val restoredAtMs: Long? = null,
    /** "Restored, not verified" until re-verified by QR in person (RS2, design §5.98). */
    val restoredUnverified: Boolean = false,
    /** Their device's X25519 key from an identity backup (v0.1.4a); null for legacy contacts. */
    val dhHex: String? = null,
)

/** RS1: the contact is on a new (restored) device; a system notice was added to the chat. */
data class RestoreNoticeInfo(val contactId: String, val restoredAtMs: Long?, val msgId: String)

/** A partner asks for a media item again ([localPath] = our copy, if any). */
data class MediaRequestInfo(
    val contactId: String, val msgId: String, val hash: String, val name: String, val mime: String, val localPath: String?,
    /** "Ask to resend" only: the requester's optional note, carried in the same message. */
    val note: String? = null,
)

/** Settings > Storage > Keep sent media (defaults 90 days / 2 GB, oldest deleted first). */
data class KeptMediaPrefs(val maxDays: Int = 90, val maxBytes: Long = 2_000_000_000L)

data class KeptMediaUsageInfo(val count: Int, val bytes: Long)

/** Placeholder state change; [state] = a `FileAttachment.State` name. */
data class MediaUpdateInfo(val contactId: String, val msgId: String, val state: String)

/** One encrypted, 64 KiB-padded chat-backup blob (kind 0 snapshot, 1 log, 2 media). */
class ChatBackupBlobInfo(val name: String, val kind: Int, val seq: Long, val blob: ByteArray, val deleteBelowSeq: Long?)

data class ChatBackupSummaryInfo(val messages: Int, val mediaPlaceholders: Int)

/** Settings › Backup › "Also back up photos, videos and files" (CB5). Off by default. */
data class MediaBackupPrefs(
    val enabled: Boolean = false,
    val photos: Boolean = true,
    val videos: Boolean = false,
    val voice: Boolean = true,
    val files: Boolean = true,
    val mobileData: Boolean = false,
)

data class MediaBackupItemInfo(val msgId: String, val hash: String, val mime: String, val localPath: String?, val blobName: String)

data class DeviceKeyInfo(val deviceKeyHex: String, val fingerprint: String, val restoredAtMs: Long?)

/** QR verification failed: not the contact's current identity + device key (e.g. a pre-restore QR). */
class VerificationFailedException : Exception("safety numbers don't match")

data class ContactPreview(val contact: Contact, val alreadyKnown: Boolean, val mismatchWith: Contact?)

data class IncomingText(
    val contactId: String,
    /** Empty for burn messages: content only via [OmcpCore.openBurn]. */
    val text: String,
    val createdMs: Long,
    val envelopeId: String,
    val burn: chat.omcp.app.burn.BurnSpec? = null,
    val mime: String? = null,
)

/** A queued outbound file (encrypted into 64 KiB chunks by the Rust core). */
data class FileOfferInfo(val transferId: String, val contactId: String, val name: String, val mime: String, val size: Long, val chunks: Int)

/** Transfer progress: sender = chunks sealed onto the link, receiver = chunks received. */
data class FileProgressInfo(
    val transferId: String,
    val contactId: String,
    val incoming: Boolean,
    val name: String,
    val size: Long,
    val doneChunks: Int,
    val totalChunks: Int,
) {
    val fraction: Float get() = if (totalChunks == 0) 1f else doneChunks.toFloat() / totalChunks
    val doneBytes: Long get() = (size * fraction).toLong()
}

/** A fully received file whose BLAKE3 hash was verified in Rust. */
class ReceivedFileInfo(val transferId: String, val contactId: String, val name: String, val mime: String, val bytes: ByteArray)

/** `integrity` (hash/AEAD failed), `invalid` or `busy`. */
data class FileFailureInfo(val transferId: String, val contactId: String, val reason: String)

/** PDUs to transmit (in order) + decrypted messages from one link step. */
data class LinkResult(
    val outgoing: List<ByteArray>,
    val received: List<IncomingText>,
    val established: Boolean,
    val peerContactId: String?,
    val flushed: Int,
    val opened: List<chat.omcp.app.burn.OpenedInfo> = emptyList(),
    val fileProgress: List<FileProgressInfo> = emptyList(),
    val files: List<ReceivedFileInfo> = emptyList(),
    val fileFailures: List<FileFailureInfo> = emptyList(),
    /** More file chunks wait: call [CoreLink.flush] again once the GATT queue is empty. */
    val morePending: Boolean = false,
    val restoreNotices: List<RestoreNoticeInfo> = emptyList(),
    val mediaRequests: List<MediaRequestInfo> = emptyList(),
    val resendAsks: List<MediaRequestInfo> = emptyList(),
    val mediaUpdates: List<MediaUpdateInfo> = emptyList(),
    /** Other links (by [CoreLink.handle]) with mesh frames waiting: flush them (multi-hop relay). */
    val relayLinks: List<Long> = emptyList(),
)

/**
 * Mesh relay counters from Rust. The relay is always on with an automatic cap
 * (REQUIREMENTS RL1, RL2); Settings shows only [storageUsedBytes] (RL4).
 */
data class MeshRelayStats(
    val carried: Int = 0,
    val carriedBytes: Long = 0,
    val ownPending: Int = 0,
    val neighbours: Int = 0,
    val storageUsedBytes: Long = 0,
    val storageCapBytes: Long = 500_000_000L,
)

/** Mesh-caused status change (PROTOCOL §22.8(3)): [status] is "Sent" (handed to a relay) or "Delivered" (end-to-end ack). */
data class MeshStatusUpdate(val msgId: String, val contactId: String, val status: String)

enum class NetworkType { Offline, Wifi, Cellular }

data class GatewayDecision(val bridge: Boolean, val meteredWarning: Boolean)

/** Hopla News story (REQUIREMENTS "Hopla News", v0.4). */
data class NewsStoryUi(
    val idHex: String, val topic: String, val title: String, val summary: String,
    val publisher: String, val sourceName: String, val link: String, val publishedMs: Long,
    val via: String?, val fetchedHere: Boolean, val emergency: Boolean,
)

/** What a gateway phone may fetch now (gateway rules: Wi-Fi-only default, metered warns, 500 MB/day). */
data class NewsFetchPlanUi(
    val feeds: List<String> = emptyList(), val meteredWarning: Boolean = false,
    val blocked: String? = "offline", val usedTodayBytes: Long = 0, val dailyCapBytes: Long = 500_000_000,
)

data class NewsCardUi(val idHex: String, val publisher: String, val link: String, val title: String)

data class PasswordCheck(val ok: Boolean, val score: Int, val tooShort: Boolean)

data class BackupMeta(val createdAtMs: Long, val deviceHint: String, val needsPassword: Boolean)

/** [history]: legacy history snapshot from pre-chat-backup blobs (never burn content); null otherwise. */
class RestoredBackup(
    val phrase: String,
    val contacts: List<Contact>,
    val displayName: String? = null,
    val history: ByteArray? = null,
)

/** The encrypted local store can't be opened (wrong/lost storage key, other identity, newer schema). */
class StorageUnavailableException(message: String?, cause: Throwable? = null) : Exception(message, cause)

/** File-transfer metadata as persisted by the Rust store; [state] = a `FileAttachment.State` name. */
data class StoredFileInfo(val name: String, val mime: String, val size: Long, val state: String, val doneBytes: Long, val localPath: String?)

/**
 * A message as persisted by the Rust store. [status] = `Queued|Sent|Received`,
 * [burnState] = `None|Unopened|Open|Opened`. Burn messages never carry text here.
 */
data class StoredMessageInfo(
    val id: String,
    val contactId: String,
    val text: String,
    val mine: Boolean,
    val timeMs: Long,
    val status: String,
    val mime: String?,
    val burn: chat.omcp.app.burn.BurnSpec?,
    val burnState: String,
    val burnAtMs: Long?,
    val file: StoredFileInfo?,
)

class WrongBackupPasswordException : Exception("wrong backup password")

/** Blob format `v` is newer than this build understands (§5.88 "Update Hopla"). */
class BackupTooNewException : Exception("backup format too new")

/** Blob can't be parsed or decrypted for a reason other than the password. */
class BackupCorruptedException(msg: String) : Exception(msg)

/** One BLE connection's secure session (Noise hybrid + M2 fragments). */
interface CoreLink {
    fun start(): LinkResult
    fun onPdu(pdu: ByteArray): LinkResult
    fun sendText(text: String): LinkResult
    fun flush(): LinkResult
    fun isEstablished(): Boolean
    fun close()
    /** Opaque Rust link handle, matched against [LinkResult.relayLinks]. */
    fun handle(): Long = 0L
}

/**
 * App-facing protocol core. Production: [RustOmcpCore] (omcp-ffi via UniFFI).
 * Previews/tests: [FakeOmcpCore]. Never invents crypto results in the UI layer.
 */
interface OmcpCore {
    val linked: Boolean
    val suiteLabel: String

    fun generateRecoveryPhrase(): String
    fun validateRecoveryPhrase(phrase: String): Boolean
    fun isRecoveryWord(word: String): Boolean
    /** Up to 3 prefix matches from the recovery word list. */
    fun suggestRecoveryWords(prefix: String): List<String>

    /** Load identity from phrase (in memory only). Must be called before the node APIs below. */
    fun load(phrase: String, displayName: String): Identity

    /**
     * Load identity and open its encrypted local store at [dbPath] (contacts, chats, messages,
     * file metadata, queue and burn state survive restarts). [storageKey] (32 bytes) is kept
     * Keystore-wrapped by the app and never hard-coded. Throws [StorageUnavailableException].
     */
    fun open(phrase: String, displayName: String, dbPath: String, storageKey: ByteArray): Identity = load(phrase, displayName)

    /** Fresh 256-bit storage key (wrap it with the Android Keystore before saving). */
    fun generateStorageKey(): ByteArray = ByteArray(32).also { java.security.SecureRandom().nextBytes(it) }

    /** Persisted messages of one chat, oldest first (empty for in-memory cores). */
    fun storedMessages(contactId: String): List<StoredMessageInfo> = emptyList()

    /** Record where a verified received file was saved (null = couldn't save). */
    fun setReceivedFilePath(transferId: String, path: String?) {}

    /** Import [RestoredBackup.history] into the local store. */
    fun restoreHistory(history: ByteArray) {}
    fun identity(): Identity?
    fun qrPayload(): String
    fun advertisement(): ByteArray

    /** Parse + safety number without saving. Throws [IllegalArgumentException] on a bad QR. */
    fun previewContact(payload: String): ContactPreview
    /** Throws [IllegalArgumentException] on a bad QR. */
    fun addContactFromQr(payload: String): Contact
    fun restoreContact(contact: Contact)
    fun contacts(): List<Contact>
    fun setVerified(contactId: String, verified: Boolean)
    fun contactForAdvertisement(adv: ByteArray): String?

    fun queueText(contactId: String, text: String)
    /** Queue text with an optional encrypted burn header. Returns the message id. */
    fun queueMessage(contactId: String, text: String, burn: chat.omcp.app.burn.BurnSpec?): String
    /** Small view-once media (≤ 16 KiB in the beta). Returns the message id. */
    fun queueMedia(contactId: String, mime: String, bytes: ByteArray, burn: chat.omcp.app.burn.BurnSpec?): String

    /** Burn after read, all enforced in Rust. Content is returned exactly once; queues OPENED. */
    fun openBurn(msgId: String, nowMs: Long): chat.omcp.app.burn.BurnContent?
    fun closeBurn(msgId: String)
    /** Timers + unread expiry on sent and received copies; returns shredded ids. */
    fun tickBurn(nowMs: Long): List<String>
    fun burnIsLive(msgId: String): Boolean
    fun burnSpecValid(spec: chat.omcp.app.burn.BurnSpec): Boolean
    /** Queue a file (≤ [maxFileBytes], checked before encryption). Throws [IllegalArgumentException] if too large. */
    fun queueFile(contactId: String, name: String, mime: String, bytes: ByteArray): FileOfferInfo
    fun cancelFile(transferId: String): Boolean
    fun queuedFor(contactId: String): Int
    fun pathStatus(): MeshStatus

    fun openInitiatorLink(contactId: String): CoreLink
    fun openResponderLink(): CoreLink

    /** Backup password policy (≥ 8 chars, zxcvbn ≥ 3) — computed in Rust core. */
    fun checkBackupPassword(password: String): PasswordCheck
    /** Encrypted blob of identity + contacts. Never contains the plaintext phrase. */
    fun createBackup(password: String, deviceHint: String, nowMs: Long): ByteArray
    /** Null if unreadable; throws [BackupTooNewException] if the format is newer than supported. */
    fun backupInfo(blob: ByteArray): BackupMeta?
    /**
     * Throws [WrongBackupPasswordException] on a wrong password, [BackupTooNewException] or
     * [BackupCorruptedException] otherwise.
     */
    fun openBackup(blob: ByteArray, password: String): RestoredBackup
    /** The loaded identity's phrase (gate behind device auth in UI). */
    fun recoveryPhrase(): String

    // ---- encrypted chat backup (CB1–CB5) and restore indicator (RS1–RS2) ----

    fun chatBackupDue(nowMs: Long, onWifi: Boolean, allowMobile: Boolean): Boolean = false
    /** Next upload (null = nothing changed); call [chatBackupCommit] after it uploaded. */
    fun chatBackupNext(nowMs: Long, forceSnapshot: Boolean): ChatBackupBlobInfo? = null
    fun chatBackupCommit(seq: Long) {}
    /** Restore text from every downloaded chat blob; media become placeholders. */
    fun chatBackupRestore(blobs: List<ByteArray>): ChatBackupSummaryInfo = ChatBackupSummaryInfo(0, 0)
    fun mediaBackupSettings(): MediaBackupPrefs = MediaBackupPrefs()
    /** Returns media blob names to delete when media backup was turned off (after confirmation). */
    fun setMediaBackupSettings(p: MediaBackupPrefs): List<String> = emptyList()
    fun mediaBackupDue(nowMs: Long, onWifi: Boolean): Boolean = false
    fun mediaBackupTodo(): List<MediaBackupItemInfo> = emptyList()
    fun sealMediaBackup(hash: String, bytes: ByteArray): ChatBackupBlobInfo = throw UnsupportedOperationException("no media backup")
    fun mediaBackupCommit(hash: String, nowMs: Long) {}
    fun mediaPlaceholders(): List<MediaBackupItemInfo> = emptyList()
    /** Decrypts a media blob for a placeholder and verifies its hash (throws otherwise). */
    fun restoreMediaFromBackup(msgId: String, blob: ByteArray): ByteArray = throw UnsupportedOperationException("no media backup")
    /** Tap on a placeholder: request it again from the partner. */
    fun requestMedia(msgId: String) {}
    /** User-confirmed "Ask to resend": ONE message with the optional [note] (≤ 280 bytes). */
    fun askToResend(msgId: String, note: String?) {}
    // ---- multi-hop mesh relay (PROTOCOL §22) ----
    fun meshStats(): MeshRelayStats = MeshRelayStats()
    // ---- Hopla News (v0.4): read-only, users cannot publish ----
    fun newsFeed(tab: String): List<NewsStoryUi> = emptyList()
    fun newsFetchPlan(wifiOnly: Boolean, network: NetworkType, metered: Boolean, nowMs: Long): NewsFetchPlanUi = NewsFetchPlanUi()
    fun newsIngestRss(feedUrl: String, body: String, nowMs: Long): Int = 0
    fun newsTakeAlerts(): List<NewsStoryUi> = emptyList()
    fun newsCardText(story: NewsStoryUi): String = "hopla-news/1\n${story.idHex}\ngn\n${story.publisher}\n${story.link}\n${story.title}"
    fun parseNewsCard(text: String): NewsCardUi? = null
    /** RL2: report free/total storage; under 10 % free Rust lowers the relay cap. */
    fun meshSetFreeStorage(freeBytes: Long, totalBytes: Long) {}
    /** Periodic relay work; returns link handles with frames waiting. */
    fun meshTick(nowMs: Long): List<Long> = emptyList()
    fun meshPendingLinks(): List<Long> = emptyList()
    /** Status changes caused by the mesh since the last call (Queued → Sent → Delivered). */
    fun meshTakeStatusUpdates(): List<MeshStatusUpdate> = emptyList()
    /** 6-byte HELLO beacon (§22.7(12)) for advertisement data. */
    fun meshBeacon(): ByteArray = ByteArray(0)
    fun meshMaxLinks(): Int = 4

    fun keptMediaSettings(): KeptMediaPrefs = KeptMediaPrefs()
    /** Applies at once; returns how many kept copies were evicted (oldest first). */
    fun setKeptMediaSettings(p: KeptMediaPrefs, nowMs: Long): Int = 0
    fun keptMediaUsage(): KeptMediaUsageInfo = KeptMediaUsageInfo(0, 0)
    fun evictKeptMedia(nowMs: Long): Int = 0
    /** Answer a partner's [MediaRequestInfo] with our copy, or null = we no longer have it. */
    fun answerMediaRequest(contactId: String, msgId: String, bytes: ByteArray?) {}
    /** After restoring on a new install: new device key stamped `restored_at` (RS1). */
    fun markRestored(nowMs: Long): DeviceKeyInfo? = null
    fun deviceInfo(): DeviceKeyInfo? = null
    /** Verify in person by QR (RS2). Throws [VerificationFailedException] on a stale or wrong code. */
    fun verifyContactFromQr(payload: String): Contact {
        val id = previewContact(payload).contact.id
        setVerified(id, true)
        return contacts().first { it.id == id }
    }

    /** Core file cap (decimal bytes, 50 MB). UI must read this, not hard-code. */
    fun maxFileBytes(): Long
    fun gatewayDailyCapBytes(): Long
    fun gatewayDecision(wifiOnly: Boolean, network: NetworkType, metered: Boolean): GatewayDecision
}

/** BLE GATT layout. PROTOCOL §8.1's placeholder UUID is not valid hex, so the
 * beta uses `4f4d4350-…` ("OMCP" in ASCII hex). Finalize before ship. */
object OmcpGatt {
    const val SERVICE = "4f4d4350-0001-4c01-9e00-000000000001"
    const val INBOX = "4f4d4350-0002-4c01-9e00-000000000001"
    const val OUTBOX = "4f4d4350-0003-4c01-9e00-000000000001"
    const val CONTROL = "4f4d4350-0004-4c01-9e00-000000000001"
    const val CCCD = "00002902-0000-1000-8000-00805f9b34fb"
    /** Manufacturer-data company id for the 13-byte advertisement (0xFFFF = testing). */
    const val MANUFACTURER_ID = 0xFFFF
    /** Local name iPhones put in the (foreground) advertisement; Android scans for it too. */
    const val LOCAL_NAME = "OMCP"
}

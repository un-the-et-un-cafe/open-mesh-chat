package chat.omcp.app.data

import chat.omcp.app.core.Contact
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.MeshStatus
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.PathKind
import chat.omcp.app.burn.BurnContent
import chat.omcp.app.burn.OpenedInfo
import chat.omcp.app.burn.BurnSpec
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/** File attachment state (DESIGN_SPEC §5.24, Designy v0.1.1 `file-transfer`). */
data class FileAttachment(
    val name: String,
    val sizeBytes: Long,
    val mime: String,
    val state: State,
    /** Bytes sent (sealed onto the link) or received so far. */
    val doneBytes: Long = 0,
    /** Received files: where the verified plaintext was saved (app-private storage). */
    val localPath: String? = null,
) {
    enum class State {
        Queued, Sending, Receiving, Paused, Sent, Received, Failed, IntegrityFailed,
        /** Restored from the chat backup without its bytes: missing-media placeholder (§5.92). */
        Missing,
        /** Re-requested from the partner. */
        Requested,
        /** The partner no longer has it: "Ask to resend". */
        Unavailable,
    }
    val isPlaceholder get() = state == State.Missing || state == State.Requested || state == State.Unavailable
    val fraction: Float get() = if (sizeBytes <= 0) (if (state == State.Received || state == State.Sent) 1f else 0f) else doneBytes.toFloat() / sizeBytes
}

/** Stores a verified received file; returns its local path (null = couldn't save). */
fun interface FileSink {
    fun save(file: chat.omcp.app.core.ReceivedFileInfo): String?
}

data class ChatMessage(
    val id: String,
    val contactId: String,
    val text: String,
    val mine: Boolean,
    val timeMs: Long,
    val status: Status,
    val file: FileAttachment? = null,
    /** Burn after read (v0.1.3). Content lives only in the Rust shredding store; [text] stays empty. */
    val burn: BurnSpec? = null,
    val burnState: BurnState = BurnState.None,
    /** When the opened content is deleted (timer); null = on close. */
    val burnAtMs: Long? = null,
) {
    enum class Status {
        Queued, Sent,
        /** End-to-end delivery ack received over the mesh (PROTOCOL §22.8(3)). */
        Delivered,
        Received,
        /** In-chat system notice; [text] = `restored:<ms>` (RS1, design §5.95). */
        System,
    }

    /** RS1 notice: when the contact restored (0 = unknown). */
    val restoredAtMs: Long? get() = if (status == Status.System && text.startsWith("restored:")) text.removePrefix("restored:").toLongOrNull() else null
    enum class BurnState { None, Unopened, Open, Opened }

    val isViewOnceMedia get() = burn != null && file != null && (file.mime.startsWith("image/") || file.mime.startsWith("video/"))

    /** Burnable content can't be saved, forwarded, quoted, shared or searched. */
    val canShare get() = burn == null
}

enum class BackupGate { Ok, Banner, Required }

const val BACKUP_DRIVE = "drive"
const val BACKUP_WRITTEN = "written"
const val REMIND_WINDOW_MS = 24 * 60 * 60 * 1000L

/** Something that can push queued texts out now (the BLE mesh service). */
fun interface MeshKick {
    fun kick(contactId: String)
}

/**
 * Single source of truth for identity, contacts, messages and path status.
 *
 * With [dbPath] set, the Rust core keeps contacts, verification state, chats, messages,
 * file metadata, the outbound queue and burn state in its encrypted local store at that
 * path (key: [AppStore.storageKey], Keystore-sealed) and they are reloaded on [restore].
 * Without it (tests, previews) everything is in memory.
 */
class MeshRepository(
    val core: OmcpCore,
    private val store: AppStore,
    private val dbPath: String? = null,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val _contacts = MutableStateFlow<List<Contact>>(emptyList())
    val contacts: StateFlow<List<Contact>> = _contacts.asStateFlow()
    private val _messages = MutableStateFlow<Map<String, List<ChatMessage>>>(emptyMap())
    val messages: StateFlow<Map<String, List<ChatMessage>>> = _messages.asStateFlow()
    private val _status = MutableStateFlow(MeshStatus(0, 0, PathKind.Queued))
    val status: StateFlow<MeshStatus> = _status.asStateFlow()
    private val _onboarded = MutableStateFlow(false)
    val onboarded: StateFlow<Boolean> = _onboarded.asStateFlow()

    val appStore: AppStore get() = store

    @Volatile var kick: MeshKick? = null
    @Volatile var fileSink: FileSink? = null
    /** Encrypted chat backup (Drive appDataFolder); null when not configured. */
    @Volatile var chatBackup: chat.omcp.app.backup.ChatBackupSync? = null

    /** Partners asking us to resend media we no longer have (shown as a prompt). */
    private val _resendAsks = MutableStateFlow<List<chat.omcp.app.core.MediaRequestInfo>>(emptyList())
    val resendAsks: StateFlow<List<chat.omcp.app.core.MediaRequestInfo>> = _resendAsks.asStateFlow()

    /** Set after a restore on this phone: banner "You restored this account…" (§5.95/§5.98). */
    private val _restoredAtMs = MutableStateFlow<Long?>(null)
    val restoredAtMs: StateFlow<Long?> = _restoredAtMs.asStateFlow()

    /** Load an existing identity from the store; returns true if onboarded. */
    fun restore(): Boolean {
        val phrase = store.recoveryPhrase ?: return false
        loadCore(phrase, store.displayName ?: "Me")
        store.contacts.forEach { core.restoreContact(it) }
        // Contacts used to live in plain SharedPreferences: once in the encrypted store, drop them.
        if (dbPath != null && store.contacts.isNotEmpty()) store.contacts = emptyList()
        loadMessages()
        refresh()
        _restoredAtMs.value = runCatching { core.deviceInfo()?.restoredAtMs }.getOrNull()
        _onboarded.value = true
        return true
    }

    /** Create or restore identity from [phrase] and persist it (sealed). */
    fun saveIdentity(phrase: String, name: String) {
        loadCore(phrase, name)
        store.recoveryPhrase = phrase
        store.displayName = name
        refresh()
    }

    /** Restore identity + contacts + verification state from a decrypted backup (O3). */
    fun restoreFromBackup(r: chat.omcp.app.core.RestoredBackup, name: String) {
        saveIdentity(r.phrase, r.displayName?.takeIf { it.isNotBlank() } ?: name)
        r.contacts.forEach { core.restoreContact(it) }
        // Legacy blobs only: history now lives in the separate chat backup.
        r.history?.let { core.restoreHistory(it) }
        markRestored()
        store.backupMethods = store.backupMethods + BACKUP_DRIVE
        persistContacts()
        loadMessages()
    }

    /** 24-word restore on a new install (no cloud blob): identity + new device key (RS1). */
    fun restoreFromPhrase(phrase: String, name: String) {
        saveIdentity(phrase, name)
        markRestored()
        refresh()
    }

    /** RS1: this install gets a new device key stamped `restored_at`; contacts are told on the next link. */
    private fun markRestored() {
        _restoredAtMs.value = runCatching { core.markRestored(clock())?.restoredAtMs }.getOrNull()
    }

    /**
     * Restore chat text (and backed-up media) from the encrypted chat backup. Blocking (network):
     * call from the restore job. Media not in the backup stay placeholders that refill from partners.
     */
    fun restoreChats(): chat.omcp.app.backup.ChatRestoreResult? {
        val sync = chatBackup ?: return null
        val sink = fileSink
        val r = sync.restore { msgId, mime, bytes ->
            sink?.save(chat.omcp.app.core.ReceivedFileInfo(msgId, "", "media", mime, bytes))
        }
        loadMessages()
        refresh()
        return r
    }


    // ---- missing media (CB3) and restore notices (RS1) ----

    /** From the link: a contact is on a new device; the core already stored the system notice. */
    fun onRestoreNotice(n: chat.omcp.app.core.RestoreNoticeInfo) {
        append(ChatMessage(n.msgId, n.contactId, "restored:${n.restoredAtMs ?: 0}", false, clock(), ChatMessage.Status.System))
        refresh()
    }

    /** From the link: the partner asks for media again; answer with our saved copy (or "no longer have it"). */
    fun onMediaRequest(r: chat.omcp.app.core.MediaRequestInfo) {
        val bytes = r.localPath?.let { p -> runCatching { java.io.File(p).readBytes() }.getOrNull() }
        runCatching { core.answerMediaRequest(r.contactId, r.msgId, bytes) }
        kick?.kick(r.contactId)
    }

    fun onResendAsk(r: chat.omcp.app.core.MediaRequestInfo) {
        _resendAsks.update { list -> list.filterNot { it.msgId == r.msgId } + r }
    }

    fun dismissResendAsk(msgId: String) = _resendAsks.update { list -> list.filterNot { it.msgId == msgId } }

    fun onMediaUpdate(u: chat.omcp.app.core.MediaUpdateInfo) {
        val m = find(u.contactId, u.msgId) ?: return
        val f = m.file ?: return
        if (!f.isPlaceholder) return
        replace(m.copy(file = f.copy(state = FileAttachment.State.valueOf(u.state))))
    }

    /** Tap on a placeholder: request again now (a no-op while already requested). */
    fun requestMedia(contactId: String, msgId: String) {
        val m = find(contactId, msgId) ?: return
        val f = m.file ?: return
        if (f.state != FileAttachment.State.Missing) return
        runCatching { core.requestMedia(msgId) }
        replace(m.copy(file = f.copy(state = FileAttachment.State.Requested)))
        kick?.kick(contactId)
    }

    /** "Ask to resend", after the confirm sheet: ONE request carrying the optional note. */
    fun askToResend(contactId: String, msgId: String, note: String?) {
        runCatching { core.askToResend(msgId, note?.trim()?.takeIf { it.isNotEmpty() }) }
        kick?.kick(contactId)
    }

    /** How many media placeholders wait in a chat (chat banner, §5.92). */
    fun waitingMedia(contactId: String): Int =
        _messages.value[contactId].orEmpty().count { it.file?.isPlaceholder == true }

    private fun loadCore(phrase: String, name: String) {
        val path = dbPath ?: run {
            core.load(phrase, name)
            return
        }
        val key = store.storageKey ?: core.generateStorageKey().also { store.storageKey = it }
        try {
            core.open(phrase, name, path, key)
        } catch (e: chat.omcp.app.core.StorageUnavailableException) {
            // Unreadable with this key (Keystore reset, other identity, newer app): its data is
            // unrecoverable here. Keep the file aside and start a fresh store rather than lock out.
            java.io.File(path).renameTo(java.io.File("$path.unreadable-${clock()}"))
            java.io.File("$path-journal").delete()
            core.open(phrase, name, path, key)
        }
        // Keep sent media: age limit is applied on start too (size limit on every insert).
        runCatching { core.evictKeptMedia(clock()) }
    }

    /** Reload chats from the core's encrypted store (after a restart or a restore). */
    private fun loadMessages() {
        _messages.value = core.contacts()
            .associate { c -> c.id to core.storedMessages(c.id).map(::toChatMessage) }
            .filterValues { it.isNotEmpty() }
    }

    private fun toChatMessage(m: chat.omcp.app.core.StoredMessageInfo): ChatMessage {
        val file = m.file?.let {
            FileAttachment(it.name, it.size, it.mime, FileAttachment.State.valueOf(it.state), it.doneBytes, it.localPath)
        } ?: m.mime?.let { FileAttachment(if (it.startsWith("video/")) "video" else "photo", 0, it, FileAttachment.State.Received) }
        return ChatMessage(
            m.id, m.contactId, m.text, m.mine, m.timeMs, ChatMessage.Status.valueOf(m.status), file,
            burn = m.burn, burnState = ChatMessage.BurnState.valueOf(m.burnState), burnAtMs = m.burnAtMs,
        )
    }

    /** Backup requirement (REQUIREMENTS "Onboarding & recovery backup", O6). */
    fun backupGate(now: Long = clock()): BackupGate = when {
        store.backupMethods.isNotEmpty() -> BackupGate.Ok
        store.backupRemindDeadlineMs == 0L -> BackupGate.Required
        now < store.backupRemindDeadlineMs -> BackupGate.Banner
        else -> BackupGate.Required
    }

    fun remindBackupLater(now: Long = clock()) {
        if (store.backupRemindDeadlineMs == 0L) store.backupRemindDeadlineMs = now + REMIND_WINDOW_MS
    }

    fun markBackupMethod(method: String, add: Boolean = true) {
        store.backupMethods = if (add) store.backupMethods + method else store.backupMethods - method
    }

    /** Onboarding finished (after permissions step). */
    fun completeOnboarding() {
        _onboarded.value = true
    }

    /** Throws [chat.omcp.app.core.VerificationFailedException] when re-verifying with a stale QR (RS2). */
    fun addContact(qr: String, verified: Boolean): Contact {
        val known = runCatching { core.previewContact(qr).alreadyKnown }.getOrDefault(false)
        if (known && verified) {
            // Re-verify an existing contact in person: must be identity + *current* device key.
            val c = core.verifyContactFromQr(qr)
            persistContacts()
            return c
        }
        val c = core.addContactFromQr(qr)
        if (verified) core.verifyContactFromQr(qr)
        persistContacts()
        return core.contacts().first { it.id == c.id }
    }

    fun setVerified(id: String, verified: Boolean) {
        core.setVerified(id, verified)
        persistContacts()
    }

    /** Per-chat burn default (Chat info). */
    fun chatBurn(contactId: String): BurnSpec? =
        BurnSpec.fromOption(store.burnChats[contactId] ?: -1, store.burnUnreadTtl[contactId] ?: BurnSpec.UNREAD_7D)

    fun setChatBurn(contactId: String, option: Int, unreadTtl: Int = BurnSpec.UNREAD_7D) {
        BurnSpec.fromOption(option, unreadTtl)?.let { require(core.burnSpecValid(it)) { "invalid burn option" } }
        store.burnChats = if (option == -1) store.burnChats - contactId else store.burnChats + (contactId to option)
        store.burnUnreadTtl = store.burnUnreadTtl + (contactId to unreadTtl)
    }

    /**
     * Send text. [burnOnce] = the composer's per-message 🔥 toggle; otherwise the chat default.
     * The burn header travels encrypted inside the message (Rust, PROTOCOL §19.1); the
     * sender's copy lives in the Rust shredding store, never in [ChatMessage.text].
     */
    fun send(contactId: String, text: String, burnOnce: BurnSpec? = null) {
        val t = text.trim()
        if (t.isEmpty()) return
        val burn = burnOnce ?: chatBurn(contactId)
        val id = core.queueMessage(contactId, t, burn)
        if (burn != null) {
            append(ChatMessage(id, contactId, "", true, clock(), ChatMessage.Status.Queued, burn = burn, burnState = ChatMessage.BurnState.Unopened))
        } else {
            append(ChatMessage(id, contactId, t, true, clock(), ChatMessage.Status.Queued))
        }
        refresh()
        kick?.kick(contactId)
    }

    /**
     * Send a file. Size is checked against [OmcpCore.maxFileBytes] before reading/encrypting
     * (callers check first too); Rust encrypts it into 64 KiB chunks and the BLE transport
     * streams them when a link to [contactId] is up. Returns the transfer id.
     */
    fun sendFile(contactId: String, name: String, mime: String, bytes: ByteArray): String {
        require(bytes.size <= core.maxFileBytes()) { "file too large" }
        val o = core.queueFile(contactId, name, mime, bytes)
        append(
            ChatMessage(
                o.transferId, contactId, "", true, clock(), ChatMessage.Status.Queued,
                FileAttachment(name, o.size, mime, FileAttachment.State.Queued),
            ),
        )
        refresh()
        kick?.kick(contactId)
        return o.transferId
    }

    /** Cancel an outgoing (queued/sending) or incoming transfer and drop its bubble. */
    fun cancelFile(contactId: String, transferId: String) {
        core.cancelFile(transferId)
        _messages.update { all -> all + (contactId to all[contactId].orEmpty().filterNot { it.id == transferId }) }
    }

    /** From the link: chunks sealed (outgoing) or received (incoming). */
    fun onFileProgress(p: chat.omcp.app.core.FileProgressInfo) {
        val existing = find(p.contactId, p.transferId)
        if (existing == null) {
            if (!p.incoming) return // cancelled locally
            append(
                ChatMessage(
                    p.transferId, p.contactId, "", false, clock(), ChatMessage.Status.Received,
                    FileAttachment(p.name, p.size, "", FileAttachment.State.Receiving, p.doneBytes),
                ),
            )
            return
        }
        val f = existing.file ?: return
        if (f.state == FileAttachment.State.Received || f.state == FileAttachment.State.IntegrityFailed) return
        val done = p.doneChunks >= p.totalChunks
        val state = when {
            p.incoming -> FileAttachment.State.Receiving
            done -> FileAttachment.State.Sent
            else -> FileAttachment.State.Sending
        }
        replace(
            existing.copy(
                status = if (!p.incoming && done) ChatMessage.Status.Sent else existing.status,
                file = f.copy(state = state, doneBytes = p.doneBytes),
            ),
        )
    }

    /** From the link: a file arrived and its BLAKE3 hash matched (checked in Rust). */
    fun onFileReceived(r: chat.omcp.app.core.ReceivedFileInfo) {
        val path = fileSink?.save(r)
        if (fileSink != null) runCatching { core.setReceivedFilePath(r.transferId, path) }
        val f = FileAttachment(
            r.name, r.bytes.size.toLong(), r.mime,
            if (path != null || fileSink == null) FileAttachment.State.Received else FileAttachment.State.Failed,
            r.bytes.size.toLong(), path,
        )
        val existing = find(r.contactId, r.transferId)
        if (existing == null) {
            append(ChatMessage(r.transferId, r.contactId, "", false, clock(), ChatMessage.Status.Received, f))
        } else {
            replace(existing.copy(file = f))
        }
    }

    fun onFileFailed(e: chat.omcp.app.core.FileFailureInfo) {
        val m = find(e.contactId, e.transferId) ?: return
        val f = m.file ?: return
        val state = if (e.reason == "integrity") FileAttachment.State.IntegrityFailed else FileAttachment.State.Failed
        replace(m.copy(file = f.copy(state = state)))
    }

    /** Link dropped mid-transfer: outgoing/incoming transfers wait for the next link (auto-resume). */
    fun pauseTransfers(contactId: String) {
        _messages.update { all ->
            all + (
                contactId to all[contactId].orEmpty().map { m ->
                    val f = m.file
                    if (f != null && (f.state == FileAttachment.State.Sending || f.state == FileAttachment.State.Receiving)) {
                        m.copy(file = f.copy(state = FileAttachment.State.Paused))
                    } else {
                        m
                    }
                }
                )
        }
    }

    /** Mesh status changes (handed to a relay → Sent, ack → Delivered); never goes backwards. */
    fun onStatusUpdates(updates: List<chat.omcp.app.core.MeshStatusUpdate>) {
        if (updates.isEmpty()) return
        val rank = { s: ChatMessage.Status -> when (s) { ChatMessage.Status.Queued -> 0; ChatMessage.Status.Sent -> 1; ChatMessage.Status.Delivered -> 2; else -> 3 } }
        _messages.update { all ->
            var next = all
            for (u in updates) {
                val st = runCatching { ChatMessage.Status.valueOf(u.status) }.getOrNull() ?: continue
                val list = next[u.contactId] ?: continue
                next = next + (u.contactId to list.map { m ->
                    if (m.id == u.msgId && m.mine && rank(st) > rank(m.status)) m.copy(status = st) else m
                })
            }
            next
        }
    }

    /** Called by the transport when [count] queued texts to [contactId] went out. */
    fun markFlushed(contactId: String, count: Int) {
        if (count > 0) {
            _messages.update { all ->
                var left = count
                val list = all[contactId].orEmpty().map {
                    if (left > 0 && it.mine && it.file == null && it.status == ChatMessage.Status.Queued) {
                        left--
                        it.copy(status = ChatMessage.Status.Sent)
                    } else {
                        it
                    }
                }
                all + (contactId to list)
            }
        }
        refresh()
    }

    fun onReceived(msg: IncomingText) {
        val file = msg.mime?.let { FileAttachment(if (it.startsWith("video/")) "video" else "photo", 0, it, FileAttachment.State.Received) }
        if (msg.burn == null) {
            append(ChatMessage(msg.envelopeId, msg.contactId, msg.text, false, msg.createdMs, ChatMessage.Status.Received, file))
            return
        }
        append(
            ChatMessage(
                msg.envelopeId, msg.contactId, "", false, msg.createdMs, ChatMessage.Status.Received, file,
                burn = msg.burn, burnState = ChatMessage.BurnState.Unopened,
            ),
        )
    }

    /** Receiver opens a burn message: Rust returns it exactly once and queues OPENED. */
    fun openBurn(contactId: String, msgId: String, now: Long = clock()): BurnContent? {
        val m = find(contactId, msgId) ?: return null
        if (m.burn == null || m.mine || m.burnState != ChatMessage.BurnState.Unopened) return null
        val c = core.openBurn(msgId, now) ?: return null
        replace(m.copy(burnState = ChatMessage.BurnState.Open, burnAtMs = c.burnAtMs))
        kick?.kick(contactId) // deliver the OPENED receipt
        return c
    }

    /** Viewer closed: burn now. */
    fun closeBurn(contactId: String, msgId: String) {
        val m = find(contactId, msgId) ?: return
        if (m.burnState != ChatMessage.BurnState.Open) return
        core.closeBurn(msgId)
        burnNow(m)
    }

    /** From the link: the peer opened our message (Rust already shredded or started the timer). */
    fun onOpened(o: OpenedInfo) {
        val m = find(o.contactId, o.msgId) ?: return
        if (o.shredded) burnNow(m) else replace(m.copy(burnState = ChatMessage.BurnState.Open, burnAtMs = o.burnAtMs))
    }

    /** Timers + unread expiry, decided in Rust. Call often (≤ 250 ms). Returns how many burned. */
    fun tickBurn(now: Long = clock()): Int {
        val ids = core.tickBurn(now).toSet()
        val due = _messages.value.values.flatten().filter { it.id in ids && it.burnState != ChatMessage.BurnState.Opened }
        due.forEach(::burnNow)
        return due.size
    }

    private fun burnNow(m: ChatMessage) {
        if (m.isViewOnceMedia) {
            replace(m.copy(burnState = ChatMessage.BurnState.Opened, burnAtMs = null)) // "🔥 Photo opened"
        } else {
            _messages.update { all -> all + (m.contactId to all[m.contactId].orEmpty().filterNot { it.id == m.id }) }
        }
    }

    private fun find(contactId: String, id: String) = _messages.value[contactId]?.firstOrNull { it.id == id }

    private fun replace(m: ChatMessage) {
        _messages.update { all -> all + (m.contactId to all[m.contactId].orEmpty().map { if (it.id == m.id) m else it }) }
    }

    fun refresh() {
        if (core.identity() == null) return
        _contacts.value = core.contacts()
        _status.value = core.pathStatus()
    }

    fun contact(id: String) = _contacts.value.firstOrNull { it.id == id }

    private fun persistContacts() {
        // With a local store the Rust core persists contacts (encrypted); prefs only without one.
        if (dbPath == null) store.contacts = core.contacts()
        refresh()
    }

    private fun append(m: ChatMessage) {
        _messages.update { it + (m.contactId to (it[m.contactId].orEmpty() + m)) }
    }
}

package chat.omcp.app.data

import chat.omcp.app.core.Contact

/** Persistent app state. Android impl: [PrefsAppStore]; tests: [InMemoryAppStore]. */
interface AppStore {
    /** Recovery phrase (the identity master secret). Sealed with a Keystore key on device. */
    var recoveryPhrase: String?
    var displayName: String?
    var contacts: List<Contact>
    /** Global "Private notifications" (REQUIREMENTS Notifications v0.1.2). Off by default: sender + preview shown. */
    var privateNotifications: Boolean
    /** Per-chat Private notifications override: true = private, false = off even if global is on. */
    var chatPrivateOverride: Map<String, Boolean>
    /** Lock screen shows only "Hopla · New message" (VISIBILITY_PRIVATE + public version). Off by default. */
    var hideOnLockScreen: Boolean
    /** Per-chat burn option: chatId -> -1 off, 0 on open, else timer seconds. */
    var burnChats: Map<String, Int>
    /** Per-chat unread TTL (seconds) for burn messages. */
    var burnUnreadTtl: Map<String, Int>
    var gatewayEnabled: Boolean
    /** Off = Wi-Fi only (default). Metered cellular warns, never blocks. */
    var gatewayAllowMobile: Boolean
    /** Completed backup methods: "drive", "written". */
    var backupMethods: Set<String>
    var lastCloudBackupMs: Long
    /** Last successful News fetch on this phone (shown on the empty News screen). */
    var newsLastFetchMs: Long
    /** "Remind me later" deadline (24 h); 0 = not deferred. */
    var backupRemindDeadlineMs: Long
    /** Restore lockout (§5.85): wrong tries in the current round, rounds so far, lock end. */
    var restoreFailures: Int
    var restoreRound: Int
    var restoreLockedUntilMs: Long
    /** 256-bit key of the encrypted local store. On device it is sealed with a Keystore key. */
    var storageKey: ByteArray?
}

class InMemoryAppStore : AppStore {
    override var recoveryPhrase: String? = null
    override var displayName: String? = null
    override var contacts: List<Contact> = emptyList()
    override var privateNotifications = false
    override var chatPrivateOverride: Map<String, Boolean> = emptyMap()
    override var hideOnLockScreen = false
    override var burnChats: Map<String, Int> = emptyMap()
    override var burnUnreadTtl: Map<String, Int> = emptyMap()
    override var gatewayEnabled = false
    override var gatewayAllowMobile = false
    override var backupMethods: Set<String> = emptySet()
    override var lastCloudBackupMs = 0L
    override var newsLastFetchMs = 0L
    override var backupRemindDeadlineMs = 0L
    override var restoreFailures = 0
    override var restoreRound = 0
    override var restoreLockedUntilMs = 0L
    override var storageKey: ByteArray? = null
}

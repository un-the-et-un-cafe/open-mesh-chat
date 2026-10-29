package chat.omcp.app.notify

import chat.omcp.app.data.AppStore

/** Lock-screen visibility, mapped to NotificationCompat.VISIBILITY_* by the Android layer. */
/**
 * PUBLIC: lock screen shows the notification as is (already generic when private).
 * PRIVATE: lock screen shows the public version "Hopla · New message" (never blank).
 */
enum class LockVisibility { PUBLIC, PRIVATE }

/**
 * What a message notification shows. When [private] is true there is no sender,
 * no preview and no avatar: title = app name, body = generic "New message".
 * [publicVersion] is what the lock screen shows for VISIBILITY_PRIVATE.
 */
data class NotificationContent(
    val title: String,
    val body: String,
    val showAvatar: Boolean,
    val private: Boolean,
    val visibility: LockVisibility,
    val publicTitle: String,
    val publicBody: String,
)

/**
 * Pure notification policy (REQUIREMENTS "Notifications (v0.1.2)", PROTOCOL §privacy 8).
 * Default: sender + preview. Private (global or per chat): generic only.
 */
object NotificationPolicy {
    /** Per-chat override wins in both directions; otherwise the global setting. */
    fun isPrivate(store: AppStore, chatId: String): Boolean =
        store.chatPrivateOverride[chatId] ?: store.privateNotifications

    fun content(
        store: AppStore,
        chatId: String,
        sender: String,
        text: String,
        appName: String,
        genericBody: String,
        burn: Boolean = false,
    ): NotificationContent {
        // Burnable content never appears in notification previews (REQUIREMENTS v0.1.3, B5).
        val private = burn || isPrivate(store, chatId)
        val vis = if (store.hideOnLockScreen) LockVisibility.PRIVATE else LockVisibility.PUBLIC
        return if (private) {
            NotificationContent(appName, genericBody, false, true, vis, appName, genericBody)
        } else {
            NotificationContent(sender, text, true, false, vis, appName, genericBody)
        }
    }

    /** Set this chat's override (on or off, regardless of global). null = follow global. */
    fun setChatPrivate(store: AppStore, chatId: String, on: Boolean?) {
        store.chatPrivateOverride = if (on == null) store.chatPrivateOverride - chatId else store.chatPrivateOverride + (chatId to on)
    }

    /**
     * Incoming call (decision item 1). Private: "Hopla · Incoming call", no caller name.
     * No calls exist yet; this is the policy the call stack will use.
     */
    fun callContent(store: AppStore, chatId: String, caller: String, appName: String, genericCall: String): NotificationContent {
        val private = isPrivate(store, chatId)
        val vis = if (store.hideOnLockScreen) LockVisibility.PRIVATE else LockVisibility.PUBLIC
        return if (private) {
            NotificationContent(appName, genericCall, false, true, vis, appName, genericCall)
        } else {
            NotificationContent(caller, genericCall, true, false, vis, appName, genericCall)
        }
    }

    /**
     * A community live started (DESIGN §5.124). Default mode: the unlocked shade
     * shows "{host} is live in {community}" + title. The lock screen ALWAYS gets
     * the generic public version "Hopla · New message" (visibility PRIVATE),
     * whatever the lock-screen setting. Private mode (global, or per community
     * keyed by its id): generic everywhere (§5.36).
     */
    fun liveContent(
        store: AppStore,
        communityId: String,
        host: String,
        community: String,
        liveTitle: String,
        appName: String,
        genericBody: String,
        titleFmt: (String, String) -> String,
        bodyFmt: (String) -> String,
    ): NotificationContent {
        val private = isPrivate(store, communityId)
        return if (private) {
            NotificationContent(appName, genericBody, false, true, LockVisibility.PRIVATE, appName, genericBody)
        } else {
            NotificationContent(titleFmt(host, community), bodyFmt(liveTitle), false, false, LockVisibility.PRIVATE, appName, genericBody)
        }
    }
}

/// What a notification shows. Private: no sender, no preview, no avatar.
/// `hiddenPreviewsPlaceholder` is what iOS shows when previews are hidden on the lock screen.
public struct NotificationContent: Equatable {
    public var title: String
    public var body: String
    public var showAvatar: Bool
    public var isPrivate: Bool
    /// UNNotificationCategory hiddenPreviewsBodyPlaceholder; never blank ("New message").
    public var hiddenPreviewsPlaceholder: String
    /// Hide on lock screen: register the category with .hiddenPreviewsShowTitle off.
    public var hideOnLockScreen: Bool
    /// Thread grouping; generic when private so the history never groups by sender.
    public var threadId: String
    /// "message", or "live" (category with a Listen action, previews hidden on the lock screen).
    public var kind: String = "message"
}

/// Pure notification policy (REQUIREMENTS "Notifications (v0.1.2)" + product decisions 1–3).
/// Same rules as Android `NotificationPolicy`.
public enum NotificationPolicy {
    public static let appName = "Hopla"

    /// Per-chat override wins in both directions; otherwise the global setting.
    public static func isPrivate(_ store: AppStore, chatId: String) -> Bool {
        store.chatPrivateOverride[chatId] ?? store.privateNotifications
    }

    /// nil = follow global.
    public static func setChatPrivate(_ store: AppStore, chatId: String, _ on: Bool?) {
        if let on { store.chatPrivateOverride[chatId] = on } else { store.chatPrivateOverride[chatId] = nil }
    }

    public static func content(
        _ store: AppStore, chatId: String, sender: String, text: String,
        genericBody: String, burn: Bool = false
    ) -> NotificationContent {
        // Burnable content never appears in notification previews (B5).
        let priv = burn || isPrivate(store, chatId: chatId)
        if priv {
            return NotificationContent(
                title: appName, body: genericBody, showAvatar: false, isPrivate: true,
                hiddenPreviewsPlaceholder: genericBody, hideOnLockScreen: store.hideOnLockScreen,
                threadId: "hopla")
        }
        return NotificationContent(
            title: sender, body: text, showAvatar: true, isPrivate: false,
            hiddenPreviewsPlaceholder: genericBody, hideOnLockScreen: store.hideOnLockScreen,
            threadId: chatId)
    }

    /// A community live started (DESIGN §5.124). Default mode: "{host} is live in
    /// {community}" + "{title} · Tap to listen" in the unlocked list. The lock
    /// screen stays generic: the content always uses the hidden-previews category
    /// (no title, placeholder "New message"). That only holds while iOS previews
    /// are "When Unlocked" (the default) or "Never"; with "Always" iOS shows the
    /// content on the lock screen. Private mode (global or per community id):
    /// generic everywhere (§5.36).
    public static func liveStartedContent(
        _ store: AppStore, communityId: String, title: String, body: String, genericBody: String
    ) -> NotificationContent {
        let priv = isPrivate(store, chatId: communityId)
        var n = NotificationContent(
            title: priv ? appName : title, body: priv ? genericBody : body, showAvatar: false, isPrivate: priv,
            hiddenPreviewsPlaceholder: genericBody, hideOnLockScreen: true,
            threadId: "hopla")
        n.kind = "live"
        return n
    }

    /// Incoming call (decision 1). Private: "Hopla · Incoming call", no caller name.
    public static func callContent(
        _ store: AppStore, chatId: String, caller: String, genericCall: String
    ) -> NotificationContent {
        let priv = isPrivate(store, chatId: chatId)
        return NotificationContent(
            title: priv ? appName : caller, body: genericCall, showAvatar: !priv, isPrivate: priv,
            hiddenPreviewsPlaceholder: genericCall, hideOnLockScreen: store.hideOnLockScreen,
            threadId: priv ? "hopla" : chatId)
    }
}

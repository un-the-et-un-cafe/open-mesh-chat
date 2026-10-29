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

    /// A community member went live: always generic ("Hopla" / "New message"),
    /// never the title, host or community, whatever the privacy setting.
    public static func liveStartedContent(_ store: AppStore, genericBody: String) -> NotificationContent {
        NotificationContent(
            title: appName, body: genericBody, showAvatar: false, isPrivate: true,
            hiddenPreviewsPlaceholder: genericBody, hideOnLockScreen: store.hideOnLockScreen,
            threadId: "hopla")
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

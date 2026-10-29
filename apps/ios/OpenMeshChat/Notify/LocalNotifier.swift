import Foundation
import HoplaKit
import UserNotifications

/// Raises on-device notifications (mesh/local delivery) per `NotificationPolicy`.
/// Default ON with sender + preview. Private (global or per-chat override, both ways): only
/// "Hopla" / "New message", no avatar. Hide on lock screen: category with
/// `hiddenPreviewsBodyPlaceholder` = "New message" and no title shown when hidden.
final class LocalNotifier: Notifier {
    static let categoryMessage = "omcp.message"
    static let categoryMessageHidden = "omcp.message.hidden"
    static let categoryCall = "omcp.call"
    static let categoryLive = "omcp.live"

    static func registerCategories() {
        let placeholder = L("notif_new_message")
        let normal = UNNotificationCategory(identifier: categoryMessage, actions: [], intentIdentifiers: [],
                                            hiddenPreviewsBodyPlaceholder: placeholder, options: [.hiddenPreviewsShowTitle])
        // Lock-screen hiding: never show the title (sender) when previews are hidden.
        let hidden = UNNotificationCategory(identifier: categoryMessageHidden, actions: [], intentIdentifiers: [],
                                            hiddenPreviewsBodyPlaceholder: placeholder, options: [])
        let call = UNNotificationCategory(identifier: categoryCall, actions: [], intentIdentifiers: [],
                                          hiddenPreviewsBodyPlaceholder: L("notif_incoming_call"), options: [])
        // Live start (§5.124): Listen opens the app; no title when previews are hidden,
        // so the lock screen reads "Hopla · New message". No Mute action (provisional).
        let listen = UNNotificationAction(identifier: "omcp.live.listen", title: L("live_notif_listen"), options: [.foreground])
        let live = UNNotificationCategory(identifier: categoryLive, actions: [listen], intentIdentifiers: [],
                                          hiddenPreviewsBodyPlaceholder: placeholder, options: [])
        UNUserNotificationCenter.current().setNotificationCategories([normal, hidden, call, live])
    }

    static func request(_ c: NotificationContent, id: String) -> UNNotificationRequest {
        let n = UNMutableNotificationContent()
        n.title = c.title
        n.body = c.body
        n.threadIdentifier = c.threadId
        n.categoryIdentifier = c.kind == "live" ? categoryLive : (c.hideOnLockScreen ? categoryMessageHidden : categoryMessage)
        n.sound = .default
        // No avatar attachment / communication intent when private.
        return UNNotificationRequest(identifier: id, content: n, trigger: nil)
    }

    func post(_ content: NotificationContent, id: String) {
        UNUserNotificationCenter.current().add(Self.request(content, id: id))
    }
}

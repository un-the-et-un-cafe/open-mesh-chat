import HoplaKit
import SwiftUI
import UIKit
import UserNotifications

/// Hopla (protocol: OMCP). Root: onboarding until done, then tabs; the whole window is under
/// the secure-capture layer with app-switcher and recording covers.
@main
struct OpenMeshChatApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel()
    @AppStorage("omcp.themeMode") private var themeMode = ThemeMode.system.rawValue

    var body: some Scene {
        WindowGroup {
            Group {
                if model.onboarded {
                    OmcpRootView()
                } else {
                    OnboardingView()
                }
            }
            .environmentObject(model)
            .environment(\.omcpColors, OmcpColors.current())
            .preferredColorScheme(ThemeMode(rawValue: themeMode)?.colorScheme)
            .screenSecurity()
            .onChange(of: model.onboarded) { _, on in if on { model.startMesh() } }
        }
    }
}

/// Notifications default ON (asked after onboarding) and content-free APNs registration.
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(_ app: UIApplication, didFinishLaunchingWithOptions _: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        LocalNotifier.registerCategories()
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { ok, _ in
            if ok { DispatchQueue.main.async { app.registerForRemoteNotifications() } }
        }
        return true
    }

    /// STUB: the APNs token would be registered with the user's gateway mailbox. The gateway
    /// only ever sends `PushPayload.apnsPayload()` (opaque mailbox id, generic alert).
    func application(_ app: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken token: Data) {
        UserDefaults.standard.set(token.map { String(format: "%02x", $0) }.joined(), forKey: "omcp.apnsToken")
    }

    func application(_ app: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {}

    func userNotificationCenter(_ c: UNUserNotificationCenter, willPresent n: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound, .list]
    }
}

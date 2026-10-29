import Combine
import Foundation
import HoplaKit
import UIKit

/// App-wide state: the repository over the real Rust core, cloud backup stores, BLE and
/// notifications. SwiftUI views observe `revision` to refresh.
@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var revision = 0
    @Published var onboarded: Bool
    let repo: MeshRepository
    let iCloud = ICloudBackupStore()
    let drive = GoogleDrive.makeStore()
    let backupKeys = ICloudKeychainBackupKeyStore()
    private(set) var ble: BleMeshTransport?
    private var burnTimer: Timer?

    init(core: OmcpCore = RustOmcpCore(), store: AppStore = DefaultsAppStore()) {
        repo = MeshRepository(core: core, store: store, dbPath: Self.localStorePath())
        onboarded = store.onboarded && store.recoveryPhrase != nil
        repo.notifier = LocalNotifier()
        repo.fileSink = ReceivedFiles()
        // Chat backup (CB1–CB5) in iCloud: private DB, zone HoplaBackupV1 (REQUIREMENTS v0.1.4a).
        repo.chatBackup = ChatBackupSync(core: core, blobs: RecordChatBlobStore(backend: CloudKitChatRecordBackend()))
        repo.genericBody = L("notif_new_message")
        repo.liveTitleFormat = { String(format: L("live_notif_title"), $0, $1) }
        repo.liveBodyFormat = { String(format: L("live_notif_body"), $0) }
        // Flow steps may run off the main thread (cloud I/O); always publish on main.
        repo.onChange = { [weak self] in
            Task { @MainActor in
                guard let self else { return }
                self.revision &+= 1
                self.onboarded = self.repo.store.onboarded && self.repo.store.recoveryPhrase != nil
            }
        }
        repo.restore()
        // Keep sent media: age limit on start (the size limit applies on every insert).
        _ = repo.core.evictKeptMedia(nowMs: Int64(Date().timeIntervalSince1970 * 1000))
        // Burn timers + unread expiry, decided in Rust; ≤ 250 ms so B1 (≤ 1 s) holds.
        burnTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.repo.tickBurn() }
        }
        if onboarded { startMesh() }
    }

    /// Application Support/omcp-store.db, excluded from iCloud/iTunes backup (its key is
    /// this-device-only anyway) and protected until first unlock.
    static func localStorePath() -> String? {
        let fm = FileManager.default
        guard var dir = try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true) else {
            return nil
        }
        try? fm.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: dir.path)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? dir.setResourceValues(values)
        return dir.appendingPathComponent("omcp-store.db").path
    }

    var core: OmcpCore { repo.core }
    var store: AppStore { repo.store }

    func makeOnboarding() -> OnboardingFlow {
        OnboardingFlow(repo: repo, iCloud: iCloud, drive: drive, keyStore: backupKeys, deviceHint: UIDevice.current.model)
    }

    func makeBackupSettings() -> BackupSettings {
        BackupSettings(repo: repo, stores: [iCloud, drive], keyStore: backupKeys, deviceHint: UIDevice.current.model)
    }

    func startMesh() {
        guard ble == nil, core.identity() != nil else { return }
        let t = BleMeshTransport(repo: repo)
        repo.kick = t
        ble = t
        t.start()
    }

    func touch() { revision &+= 1 }
}

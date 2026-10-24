import HoplaKit
import SwiftUI

struct YouView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c

    var body: some View {
        let _ = model.revision
        let id = model.core.identity()
        NavigationStack {
            List {
                if model.repo.backupGate() == .banner { BackupReminderBanner().listRowInsets(EdgeInsets()) }
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(id?.displayName ?? "").font(.title2.weight(.semibold))
                        if let p = id?.peerIdHex { Text("omcp1:\(p.prefix(4))…\(p.suffix(4))").font(.footnote.monospaced()).foregroundStyle(c.ink2) }
                        Text(model.core.suiteLabel).font(.caption).foregroundStyle(c.ink3)
                    }
                    NavigationLink(L("my_qr_title")) { MyQrView() }
                }
                Section {
                    NavigationLink { BackupSettingsView() } label: { row(L("backup_title"), L("backup_row_desc")) }
                    NavigationLink { StorageSettingsView() } label: { row(L("storage_title"), L("storage_keep_title")) }
                    NavigationLink { MeshRelaySettingsView() } label: { row(L("relay_title"), L("relay_row_desc")) }
                    NavigationLink { NotificationSettingsView() } label: { row(L("notif_title"), L("notif_row_desc")) }
                    NavigationLink { PrivacySettingsView() } label: { row(L("privacy_title"), L("privacy_row_desc")) }
                    NavigationLink { GatewaySettingsView() } label: { row(L("gateway_title"), L("gateway_row_desc")) }
                }
                Section { row(L("about"), L("about_desc")) }
            }
            .navigationTitle(L("tab_you"))
        }
    }

    private func row(_ title: String, _ support: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
            Text(support).font(.caption).foregroundStyle(c.ink2)
        }
    }
}

/// Settings › Notifications: on by default with sender + preview; global Private notifications;
/// Hide on lock screen. Per-chat overrides live in Chat info.
struct NotificationSettingsView: View {
    @EnvironmentObject private var model: AppModel
    var body: some View {
        let _ = model.revision
        let s = model.store
        Form {
            Section(footer: Text(L("notif_private_footer"))) {
                Toggle(L("notif_private"), isOn: Binding(get: { s.privateNotifications }, set: { s.privateNotifications = $0; model.touch() }))
                Toggle(L("notif_hide_lock"), isOn: Binding(get: { s.hideOnLockScreen }, set: { s.hideOnLockScreen = $0; model.touch() }))
            }
            Section(header: Text(L("notif_preview"))) {
                let p = NotificationPolicy.content(s, chatId: "", sender: "Ana", text: L("notif_sample_text"), genericBody: L("notif_new_message"))
                VStack(alignment: .leading) { Text(p.title).bold(); Text(p.body) }
            }
        }
        .navigationTitle(L("notif_title"))
    }
}

/// Settings › Privacy: screenshots are always blocked; honest copy about second cameras.
struct PrivacySettingsView: View {
    var body: some View {
        Form {
            Section(footer: Text(L("privacy_second_camera"))) {
                Label(L("privacy_screens_blocked"), systemImage: "eye.slash")
            }
        }
        .navigationTitle(L("privacy_title"))
    }
}

/// Settings > Storage > Keep sent media (REQUIREMENTS "Keep sent media"); limits live in Rust.
struct StorageSettingsView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var prefs = KeptMediaPrefs()
    @State private var usage = KeptMediaUsage(count: 0, bytes: 0)

    private func reload() { prefs = model.core.keptMediaSettings(); usage = model.core.keptMediaUsage() }
    private func apply(_ p: KeptMediaPrefs) {
        _ = try? model.core.setKeptMediaSettings(p, nowMs: Int64(Date().timeIntervalSince1970 * 1000))
        reload()
    }

    var body: some View {
        Form {
            Section(header: Text(L("storage_keep_title")), footer: Text(L("storage_keep_body"))) {
                Picker(L("storage_keep_days"), selection: Binding(get: { prefs.maxDays }, set: { var p = prefs; p.maxDays = $0; apply(p) })) {
                    ForEach(StorageSettings.dayOptions, id: \.self) { Text(L("storage_days", $0)).tag($0) }
                }
                Picker(L("storage_keep_size"), selection: Binding(get: { prefs.maxBytes }, set: { var p = prefs; p.maxBytes = $0; apply(p) })) {
                    ForEach(StorageSettings.sizeOptions, id: \.self) { Text(StorageSettings.formatBytes($0)).tag($0) }
                }
                Text(L("storage_usage", usage.count, StorageSettings.formatBytes(usage.bytes))).font(.caption).foregroundStyle(c.ink2)
            }
        }
        .navigationTitle(L("storage_title"))
        .onAppear(perform: reload)
    }
}

/// Settings > Mesh relay (REQUIREMENTS RL4): only "Storage used for relaying: X MB", for
/// information. No toggle and no cap picker: the relay is always on (RL1) and the cap is
/// automatic (RL2). No mockup exists yet (DESIGN_SPEC Q36 placeholder): plain Form row.
struct MeshRelaySettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var usedBytes: Int64 = 0

    private func reload() { usedBytes = model.core.meshStats().storageUsedBytes }

    var body: some View {
        Form {
            Text(L("relay_storage_used", MeshRelaySettings.formatMb(usedBytes)))
        }
        .navigationTitle(L("relay_title"))
        .onAppear(perform: reload)
    }
}

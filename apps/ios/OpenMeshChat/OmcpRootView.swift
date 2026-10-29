import HoplaKit
import SwiftUI

struct OmcpRootView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.colorScheme) private var scheme
    /// Optional first-launch tip; after dismiss, Gateway owns the BLE/companion callout.
    @AppStorage("omcp.iosBleTipDismissed") private var iosBleTipDismissed = false
    @State private var showIosBleTip = false

    private var gate: BackupGate {
        _ = model.revision
        return model.repo.backupGate()
    }

    var body: some View {
        TabView {
            ChatsListView()
                .tabItem { Label(L("tab_chats"), systemImage: "bubble.left.and.bubble.right") }
                .omcpOpaqueBars(scheme == .dark ? .dark : .light)
            NewsHomeView()
                .tabItem { Label(L("tab_news"), systemImage: "newspaper") }
                .omcpOpaqueBars(scheme == .dark ? .dark : .light)
            CommunitiesHomeView()
                .tabItem { Label(L("tab_communities"), systemImage: "person.3") }
                .omcpOpaqueBars(scheme == .dark ? .dark : .light)
            YouView()
                .tabItem { Label(L("tab_you"), systemImage: "person.crop.circle") }
                .omcpOpaqueBars(scheme == .dark ? .dark : .light)
        }
        .tint((scheme == .dark ? OmcpColors.dark : OmcpColors.light).brand)
        .environment(\.omcpColors, scheme == .dark ? .dark : .light)
        .onAppear { if !iosBleTipDismissed { showIosBleTip = true } }
        // O6: after the 24 h "Remind me later" window, Hopla can't be used until a backup exists.
        .fullScreenCover(isPresented: .constant(gate == .required)) { BackupRequiredView().environmentObject(model) }
        .alert(L("ios_ble_tip_title"), isPresented: $showIosBleTip) {
            Button(L("ios_ble_tip_dismiss")) { iosBleTipDismissed = true; showIosBleTip = false }
        } message: {
            Text(L("ios_ble_tip_body"))
        }
    }
}

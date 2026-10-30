import SwiftUI

/// Product decision: keep Communities + Live tabs visible with Coming in v0.2 empty states.
struct CommunitiesHomeView: View {
    var body: some View {
        NavigationStack {
            // REQUIREMENTS "Hopla News": Live ("Coming in v0.2") moves to the top of Communities.
            VStack {
                NavigationLink(String(localized: "live_entry")) { LiveTabView() }
                EmptyStateView(
                    title: String(localized: "coming_v02_title"),
                    bodyText: String(localized: "communities_empty_body")
                )
            }
            .navigationTitle(String(localized: "tab_communities"))
        }
    }
}

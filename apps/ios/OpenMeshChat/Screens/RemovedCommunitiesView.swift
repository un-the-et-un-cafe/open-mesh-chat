import SwiftUI
import HoplaKit

/// TM2 / PROTOCOL §23.9(5): the public "Removed communities" transparency log.
struct RemovedCommunitiesView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.omcpColors) private var c
    @State private var log: [TeamLogEntryModel] = []
    @State private var input = ""
    @State private var result: String?

    static func reasonKey(_ code: Int) -> String {
        [1, 2, 3, 4, 5].contains(code) ? "mod_reason_\(code)" : "mod_reason_9"
    }

    var body: some View {
        List {
            Section {
                Text(L("mod_log_desc")).font(.footnote).foregroundStyle(c.ink2)
                if log.isEmpty {
                    Text(L("mod_log_empty")).foregroundStyle(c.ink3)
                }
                ForEach(log) { e in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(e.kind == "restore" ? L("mod_restored") : L(Self.reasonKey(e.reasonCode)))
                        Text("Community \(String(e.communityIdHex.prefix(12)))… · "
                             + Date(timeIntervalSince1970: Double(e.appliedMs) / 1000)
                                .formatted(date: .abbreviated, time: .omitted))
                            .font(.footnote).foregroundStyle(c.ink2)
                        if !e.note.isEmpty { Text(e.note).font(.footnote).foregroundStyle(c.ink2) }
                        Text(L("mod_signed_by") + " " + e.signedBy.joined(separator: ", "))
                            .font(.caption2).foregroundStyle(c.ink3)
                    }
                }
            }
            if model.core.moderationHasTeam() {
                // Anyone may hand over a team-signed action; it is verified against the
                // shipped whitelist before it is applied or gossiped.
                Section {
                    TextField(L("mod_publish_label"), text: $input)
                        .textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button(L("mod_publish")) {
                        result = model.core.moderationPublish(actionHex: input.trimmingCharacters(in: .whitespaces))
                        log = model.core.moderationLog()
                    }.disabled(input.isEmpty)
                    if let result { Text(result).font(.footnote).foregroundStyle(c.ink2) }
                }
            }
        }
        .navigationTitle(L("mod_log_title"))
        .onAppear { log = model.core.moderationLog() }
    }
}

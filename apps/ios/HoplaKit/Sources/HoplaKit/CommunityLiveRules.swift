import Foundation

/// Pure rules behind the Communities + Live screens (DESIGN §5.120–5.127).
/// Same as Android `CommunityLiveParts.kt` / `LiveViewModel.removeFromLive`.
public enum CommunityLiveRules {
    /// OpenMLS small-group cap in this build (docs/LIVE.md).
    public static let maxMembers = 32

    public static func isFull(members: Int) -> Bool { members >= maxMembers }

    /// §5.126: 32 hex in groups of 4.
    public static func groupStreamId(_ hex: String) -> String {
        let h = Array(hex.lowercased())
        return stride(from: 0, to: h.count, by: 4).map { String(h[$0..<min($0 + 4, h.count)]) }.joined(separator: " ")
    }

    /// Text a report carries; the user picks where to send it (no report inbox yet).
    public static func reportText(streamIdHex: String, reason: String) -> String {
        "Hopla live report\nStream ID: \(streamIdHex.lowercased())\nReason: \(reason)"
    }

    /// Report reason string keys (§5.127 r1–r5).
    public static let reportReasonKeys = (1...5).map { "live_report_r\($0)" }

    /// Who gets the new key after Remove from live: every member except us and everyone removed so far.
    public static func rekeyRecipients(members: [String], me: String?, removed: Set<String>) -> Int {
        members.filter { $0 != me && !removed.contains($0) }.count
    }

    /// §23.9(7) reason code → string key (same as the community log).
    public static func reasonKey(_ code: Int) -> String {
        [1, 2, 3, 4, 5].contains(code) ? "mod_reason_\(code)" : "mod_reason_9"
    }
}

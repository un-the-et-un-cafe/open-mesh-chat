import Foundation

/// Battery-aware duty cycle for BLE mesh relaying (PROTOCOL §22.4; engineering defaults,
/// tunable). Same policy as Android `MeshDutyCycle`, minus scan windows: iOS schedules
/// background scanning itself, so on iOS the profile only sets the relay tick and link cap,
/// and whether to scan at all while in the foreground on low battery.
public enum MeshDutyCycle {
    public static let lowBatteryPercent = 20

    public struct Profile: Equatable, Sendable {
        public var tickMs: Int64
        public var maxLinks: Int
        /// Foreground scanning allowed continuously (low battery: only in windows).
        public var continuousScan: Bool
        public var scanWindowMs: Int64
        public var periodMs: Int64
        public func scanning(nowMs: Int64) -> Bool { continuousScan || (nowMs % periodMs) < scanWindowMs }
    }

    /// `batteryPercent` < 0 = unknown (simulator): treated as full.
    public static func profile(batteryPercent: Int, charging: Bool, lowPowerMode: Bool, coreMaxLinks: Int) -> Profile {
        if charging && !lowPowerMode {
            return Profile(tickMs: 5_000, maxLinks: coreMaxLinks, continuousScan: true, scanWindowMs: 60_000, periodMs: 60_000)
        }
        if lowPowerMode || (0..<lowBatteryPercent).contains(batteryPercent) {
            return Profile(tickMs: 60_000, maxLinks: min(2, coreMaxLinks), continuousScan: false, scanWindowMs: 30_000, periodMs: 300_000)
        }
        return Profile(tickMs: 15_000, maxLinks: coreMaxLinks, continuousScan: true, scanWindowMs: 60_000, periodMs: 60_000)
    }
}

/// Settings > Mesh relay model (REQUIREMENTS "Relay", RL1–RL4; the SwiftUI view lives in the
/// app target). The relay is always on and its 500 MB cap is automatic, so there is nothing
/// to set: the screen shows only "Storage used for relaying: X MB" (RL4).
public final class MeshRelaySettings {
    private let core: OmcpCore
    public private(set) var stats: MeshRelayStats

    public init(core: OmcpCore) {
        self.core = core
        stats = core.meshStats()
    }

    public func refresh() { stats = core.meshStats() }

    /// The "X" in "X MB" for `stats.storageUsedBytes`.
    public var storageUsedMb: String { Self.formatMb(stats.storageUsedBytes) }

    /// Decimal MB, one decimal under 10 MB (same as Android `MeshRelayViewModel.formatMb`).
    public static func formatMb(_ bytes: Int64) -> String {
        if bytes <= 0 { return "0" }
        let mb = Double(bytes) / 1e6
        if mb < 10 { return String(format: "%.1f", locale: Locale(identifier: "en_US_POSIX"), mb) }
        return String(Int64(mb.rounded()))
    }
}

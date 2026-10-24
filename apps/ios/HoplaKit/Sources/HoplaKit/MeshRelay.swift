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

/// Settings > Mesh relay model (testable; the SwiftUI view lives in the app target).
public final class MeshRelaySettings {
    /// Placeholder presets (spec gap: no design for this screen yet); 50 MB is the §22.4 default.
    public static let capOptions: [Int64] = [10_000_000, 50_000_000, 100_000_000, 250_000_000, 500_000_000]
    private let core: OmcpCore
    public private(set) var prefs: MeshRelayPrefs
    public private(set) var stats: MeshRelayStats

    public init(core: OmcpCore) {
        self.core = core
        prefs = core.meshSettings()
        stats = core.meshStats()
    }

    public func setEnabled(_ on: Bool) { var p = prefs; p.enabled = on; apply(p) }
    public func setCap(_ bytes: Int64) { var p = prefs; p.capBytes = bytes; apply(p) }
    public func refresh() { prefs = core.meshSettings(); stats = core.meshStats() }

    private func apply(_ p: MeshRelayPrefs) {
        try? core.setMeshSettings(p)
        refresh()
    }
}

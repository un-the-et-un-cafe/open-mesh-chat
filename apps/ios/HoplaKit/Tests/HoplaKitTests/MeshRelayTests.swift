import XCTest
@testable import HoplaKit

/// Multi-hop mesh relay (PROTOCOL §22): settings model, duty cycle, Android parity.
final class MeshRelayTests: XCTestCase {
    func testSettingsDefaultsToggleAndCap() {
        let core = FakeOmcpCore()
        let m = MeshRelaySettings(core: core)
        XCTAssertEqual(m.prefs, MeshRelayPrefs(enabled: true, capBytes: 50_000_000))
        m.setEnabled(false)
        m.setCap(100_000_000)
        XCTAssertEqual(core.meshSettings(), MeshRelayPrefs(enabled: false, capBytes: 100_000_000))
        m.setCap(core.meshMaxCapBytes() + 1) // rejected, unchanged
        XCTAssertEqual(m.prefs.capBytes, 100_000_000)
        XCTAssertTrue(MeshRelaySettings.capOptions.contains(50_000_000))
        XCTAssertEqual(MeshRelaySettings.capOptions.last, core.meshMaxCapBytes())
    }

    func testDutyCycleFollowsBattery() {
        let charging = MeshDutyCycle.profile(batteryPercent: 10, charging: true, lowPowerMode: false, coreMaxLinks: 4)
        XCTAssertTrue(charging.continuousScan)
        XCTAssertEqual(charging.maxLinks, 4)
        let normal = MeshDutyCycle.profile(batteryPercent: 80, charging: false, lowPowerMode: false, coreMaxLinks: 4)
        XCTAssertTrue(normal.continuousScan)
        XCTAssertGreaterThan(normal.tickMs, charging.tickMs)
        let low = MeshDutyCycle.profile(batteryPercent: 10, charging: false, lowPowerMode: false, coreMaxLinks: 4)
        XCTAssertFalse(low.continuousScan)
        XCTAssertTrue(low.scanning(nowMs: 0))
        XCTAssertFalse(low.scanning(nowMs: 31_000))
        XCTAssertEqual(low.maxLinks, 2)
        XCTAssertFalse(MeshDutyCycle.profile(batteryPercent: 90, charging: true, lowPowerMode: true, coreMaxLinks: 4).continuousScan)
        XCTAssertTrue(MeshDutyCycle.profile(batteryPercent: -1, charging: false, lowPowerMode: false, coreMaxLinks: 4).continuousScan)
    }

    func testRelayLinksDefaultEmptyAndLinkHandleDefault() {
        let r = LinkResult(outgoing: [], received: [], established: true, peerContactId: nil, flushed: 0)
        XCTAssertTrue(r.relayLinks.isEmpty)
        XCTAssertEqual(LinkResult(outgoing: [], received: [], established: true, peerContactId: nil, flushed: 0, relayLinks: [3]).relayLinks, [3])
    }

    /// Same thresholds as Android `MeshDutyCycle.kt` (reads the Kotlin source when present).
    func testDutyCycleMatchesAndroid() throws {
        let kt = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("android/app/src/main/java/chat/omcp/app/mesh/MeshDutyCycle.kt")
        guard let src = try? String(contentsOf: kt, encoding: .utf8) else { throw XCTSkip("Android source not available") }
        XCTAssertTrue(src.contains("LOW_BATTERY_PCT = \(MeshDutyCycle.lowBatteryPercent)"))
        XCTAssertTrue(src.contains("Profile(ScanMode.LowPower, 30_000, 300_000, 60_000, minOf(2, coreMaxLinks))"))
    }
}

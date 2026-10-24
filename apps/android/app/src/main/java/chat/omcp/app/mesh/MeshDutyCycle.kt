package chat.omcp.app.mesh

/**
 * Battery-aware duty cycle for BLE mesh relaying (PROTOCOL §22.4 budgets; engineering
 * defaults, tunable). Pure so it is unit-tested on the JVM.
 *
 * - Charging: balanced scan continuously, relay tick every 5 s.
 * - On battery ≥ [LOW_BATTERY_PCT] %: low-power scan continuously, tick every 15 s.
 * - Low battery (< [LOW_BATTERY_PCT] %, not charging): scan for 30 s every 5 min, tick every 60 s,
 *   at most 2 mesh links. Advertising stays on (low power) so friends can still reach us.
 * - Power saver forces the low-battery profile.
 *
 * Android throttles apps that start/stop scans more than 5 times per 30 s, so windows are ≥ 30 s.
 */
object MeshDutyCycle {
    const val LOW_BATTERY_PCT = 20

    enum class ScanMode { Balanced, LowPower }

    data class Profile(
        val scanMode: ScanMode,
        /** Scan window length; equal to [periodMs] when scanning continuously. */
        val scanWindowMs: Long,
        val periodMs: Long,
        val tickMs: Long,
        val maxLinks: Int,
    ) {
        val continuous: Boolean get() = scanWindowMs >= periodMs
        /** Should the scanner be on at [nowMs] (window starts at multiples of [periodMs])? */
        fun scanning(nowMs: Long): Boolean = continuous || (nowMs % periodMs) < scanWindowMs
    }

    fun profile(batteryPct: Int, charging: Boolean, powerSave: Boolean, coreMaxLinks: Int): Profile = when {
        charging && !powerSave -> Profile(ScanMode.Balanced, CONT, CONT, 5_000, coreMaxLinks)
        powerSave || batteryPct in 0 until LOW_BATTERY_PCT ->
            Profile(ScanMode.LowPower, 30_000, 300_000, 60_000, minOf(2, coreMaxLinks))
        else -> Profile(ScanMode.LowPower, CONT, CONT, 15_000, coreMaxLinks)
    }

    private const val CONT = 60_000L
}

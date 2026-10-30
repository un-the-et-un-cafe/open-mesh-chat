package chat.omcp.app.mesh

import java.util.UUID

/**
 * Decides what to do with one BLE scan result. Pure Kotlin so it is unit-tested on the JVM.
 *
 * Why not a strict service-UUID-only filter: iPhones can't advertise manufacturer data, and a
 * **backgrounded** iOS peripheral moves its 128-bit service UUIDs into Apple's "overflow area"
 * (Apple manufacturer data, company 0x004C, type byte 0x01 + 16-byte hash bitmask). Android never
 * sees the UUID itself then, and iOS may also drop the local name. So:
 *
 * 1. Android peer: OMCP manufacturer data with a 13-byte discovery ad → [Decision.Dial] directly.
 * 2. Foreground iPhone: OMCP service UUID or the OMCP local name in the record → [Decision.Probe].
 * 3. Backgrounded iPhone: Apple overflow-area manufacturer data → [Decision.Probe] (connect,
 *    GATT service discovery, read Control for the 13-byte ad). Rate-limited by [ProbeLimiter].
 */
object ScanClassifier {
    const val APPLE_COMPANY_ID = 0x004C
    /** Apple manufacturer-data type for the background service-UUID overflow area. */
    const val APPLE_OVERFLOW_TYPE: Byte = 0x01
    const val AD_LEN = 13

    sealed interface Decision {
        data class Dial(val advertisement: ByteArray, val beacon: ByteArray? = null) : Decision
        data class Probe(val reason: String) : Decision
        data object Ignore : Decision
    }

    fun classify(
        serviceUuids: List<UUID>,
        manufacturerData: Map<Int, ByteArray>,
        localName: String?,
        omcpService: UUID,
        omcpManufacturerId: Int,
        omcpLocalName: String,
    ): Decision {
        manufacturerData[omcpManufacturerId]?.let {
            val (ad, beacon) = MeshBeacon.split(it)
            if (ad.size == AD_LEN) return Decision.Dial(ad, beacon)
        }
        if (omcpService in serviceUuids) return Decision.Probe("service-uuid")
        if (localName != null && localName == omcpLocalName) return Decision.Probe("local-name")
        if (isAppleOverflow(manufacturerData[APPLE_COMPANY_ID])) return Decision.Probe("ios-overflow")
        return Decision.Ignore
    }

    fun isAppleOverflow(data: ByteArray?): Boolean =
        data != null && data.size >= 17 && data[0] == APPLE_OVERFLOW_TYPE
}

/**
 * Limits GATT probes: at most [maxConcurrent] at once, and an address that turned out not to be
 * a reachable OMCP contact is not probed again for [negativeTtlMs] (iOS rotates its address about
 * every 15 min, so the cache stays small).
 */
class ProbeLimiter(
    private val maxConcurrent: Int = 2,
    private val negativeTtlMs: Long = 5 * 60_000L,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val inFlight = HashSet<String>()
    private val until = HashMap<String, Long>()

    @Synchronized fun tryBegin(address: String): Boolean {
        val now = clock()
        until.entries.removeAll { it.value <= now }
        if (address in inFlight || address in until || inFlight.size >= maxConcurrent) return false
        inFlight += address
        return true
    }

    /** [useful] = it was an OMCP contact we connected to; otherwise back off. */
    @Synchronized fun end(address: String, useful: Boolean) {
        inFlight -= address
        if (!useful) until[address] = clock() + negativeTtlMs
    }

    @Synchronized fun inFlightCount() = inFlight.size
}

/** OMCP manufacturer data = 13-byte discovery ad, optionally followed by the 6-byte §22.7(12) HELLO beacon. */
object MeshBeacon {
    const val AD_LEN = 13
    const val BEACON_LEN = 6

    fun split(mfr: ByteArray): Pair<ByteArray, ByteArray?> =
        if (mfr.size >= AD_LEN + BEACON_LEN && mfr[AD_LEN] == 0x48.toByte()) {
            mfr.copyOfRange(0, AD_LEN) to mfr.copyOfRange(AD_LEN, AD_LEN + BEACON_LEN)
        } else {
            mfr to null
        }

    fun join(ad: ByteArray, beacon: ByteArray): ByteArray =
        if (ad.size == AD_LEN && beacon.size == BEACON_LEN) ad + beacon else ad
}

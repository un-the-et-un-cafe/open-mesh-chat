package chat.omcp.app.diag

import android.util.Log

/**
 * In-memory connection + News diagnostics (You › About › Connection diagnostics).
 * Never holds message content, keys or full PeerIDs: peers are short ids only.
 */
object Diag {
    const val TAG = "OmcpDiag"
    private const val MAX_LOG = 50

    data class Peer(val shortId: String, val rssi: Int, val lastSeenMs: Long, val known: Boolean)
    data class News(val lastAttemptMs: Long = 0, val network: String = "", val result: String = "never")

    @Volatile var advertising: Boolean = false
    @Volatile var advertiseError: Int? = null
    @Volatile var scanning: Boolean = false
    @Volatile var scanError: Int? = null
    @Volatile var transportRunning: Boolean = false
    @Volatile var transportNote: String = "not started"
    @Volatile var news: News = News()

    private val log = ArrayDeque<String>()
    private val peers = LinkedHashMap<String, Peer>()
    /** Per GATT peer (short id or masked address): link state, e.g. "connected mtu=517", "handshake ok". */
    private val links = LinkedHashMap<String, String>()

    @Synchronized fun log(line: String, error: Boolean = false) {
        val t = java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.ROOT).format(java.util.Date())
        log.addLast("$t ${if (error) "E" else "I"} $line")
        while (log.size > MAX_LOG) log.removeFirst()
        if (error) Log.w(TAG, line) else Log.i(TAG, line)
    }

    @Synchronized fun seen(key: String, shortId: String, rssi: Int, known: Boolean, now: Long = System.currentTimeMillis()) {
        peers[key] = Peer(shortId, rssi, now, known)
        if (peers.size > 30) peers.remove(peers.keys.first())
    }

    @Synchronized fun link(key: String, state: String) {
        if (links[key] != state) log("link $key: $state", error = state.startsWith("error") || state.startsWith("rejected"))
        links[key] = state
    }

    @Synchronized fun snapshot(): Triple<List<String>, List<Peer>, Map<String, String>> =
        Triple(log.toList(), peers.values.sortedByDescending { it.lastSeenMs }, LinkedHashMap(links))

    /** Masked device address (last 2 bytes only). */
    fun mask(address: String): String = "…" + address.takeLast(5)

    /** Plain-text report for "Copy diagnostics" (no content, no keys). */
    fun report(header: List<String>, now: Long = System.currentTimeMillis()): String {
        val (l, p, k) = snapshot()
        val sb = StringBuilder()
        header.forEach { sb.appendLine(it) }
        sb.appendLine("transport: ${if (transportRunning) "running" else "stopped"} ($transportNote)")
        sb.appendLine("advertising: ${if (advertising) "on" else "off"}${advertiseError?.let { " error=$it" } ?: ""}")
        sb.appendLine("scanning: ${if (scanning) "on" else "off"}${scanError?.let { " error=$it" } ?: ""}")
        sb.appendLine("peers seen:")
        p.forEach { sb.appendLine("  ${it.shortId} rssi=${it.rssi} ${(now - it.lastSeenMs) / 1000}s ago${if (it.known) " contact" else ""}") }
        sb.appendLine("links:")
        k.forEach { (id, s) -> sb.appendLine("  $id $s") }
        sb.appendLine("news: last=${if (news.lastAttemptMs == 0L) "never" else "${(now - news.lastAttemptMs) / 1000}s ago"} net=${news.network} result=${news.result}")
        sb.appendLine("log:")
        l.forEach { sb.appendLine("  $it") }
        return sb.toString()
    }

    @Synchronized fun reset() { log.clear(); peers.clear(); links.clear(); advertising = false; scanning = false; advertiseError = null; scanError = null }
}

package chat.omcp.app.data

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import chat.omcp.app.core.NetworkType

/** Current network for News fetch rules: (type, metered). */
object NewsNet {
    fun current(ctx: Context): Pair<NetworkType, Boolean> {
        val cm = ctx.getSystemService(ConnectivityManager::class.java)
        val caps = cm?.getNetworkCapabilities(cm.activeNetwork) ?: return NetworkType.Offline to false
        if (!caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)) return NetworkType.Offline to false
        // A VPN hides the underlying transport: look at the other networks (Wi‑Fi under a VPN is Wi‑Fi).
        val all = if (caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) {
            @Suppress("DEPRECATION") cm.allNetworks.mapNotNull { cm.getNetworkCapabilities(it) }
        } else listOf(caps)
        return classify(all.map { Caps(it.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) || it.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
            it.hasTransport(NetworkCapabilities.TRANSPORT_VPN), it.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)) })
    }

    data class Caps(val wifi: Boolean, val vpn: Boolean, val notMetered: Boolean)

    /**
     * Pure rule (unit-tested): Wi‑Fi by *transport*, never by NOT_METERED, so a Wi‑Fi the
     * phone reports as metered (hotspot, "treat as metered") still counts as Wi‑Fi (it only
     * warns). Under a VPN, the underlying non-VPN network decides.
     */
    fun classify(nets: List<Caps>): Pair<NetworkType, Boolean> {
        val real = nets.filter { !it.vpn }.ifEmpty { nets }
        val wifi = real.firstOrNull { it.wifi }
        val n = wifi ?: real.firstOrNull() ?: return NetworkType.Offline to false
        return (if (wifi != null) NetworkType.Wifi else NetworkType.Cellular) to !n.notMetered
    }

    fun describe(ctx: Context): String {
        val (t, m) = current(ctx)
        return t.name.lowercase() + if (m) " (metered)" else ""
    }
}

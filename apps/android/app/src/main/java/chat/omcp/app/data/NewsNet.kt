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
        val type = when {
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) || caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> NetworkType.Wifi
            else -> NetworkType.Cellular
        }
        return type to !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
    }
}

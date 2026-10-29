package chat.omcp.app.mesh

import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import chat.omcp.app.MainActivity
import chat.omcp.app.OmcpApplication
import chat.omcp.app.R

/** Foreground service for mesh participation (PROTOCOL §13.3), persistent notification. */
class MeshService : Service() {
    private var transport: BleMeshTransport? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        MeshNotifications.ensureChannels(this)
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(this, MeshNotifications.CHANNEL_MESH)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(getString(R.string.mesh_fg_title))
            .setContentText(getString(R.string.mesh_fg_body))
            .setOngoing(true)
            .setContentIntent(open)
            .build()
        // Android 14+: a connectedDevice FGS without the Nearby permissions throws.
        val fg = runCatching { ServiceCompat.startForeground(this, 1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE) }
        if (fg.isFailure || !MeshPermissions.hasBle(this)) {
            chat.omcp.app.diag.Diag.log("mesh service not started: " + (fg.exceptionOrNull()?.javaClass?.simpleName ?: "Nearby devices permission missing"), true)
            chat.omcp.app.diag.Diag.transportNote = "permission missing"
            stopSelf()
            return START_NOT_STICKY
        }
        if (btReceiver == null) {
            // Bluetooth turned on later: start the transport then (it can't start while off).
            btReceiver = object : android.content.BroadcastReceiver() {
                override fun onReceive(c: Context, i: Intent) {
                    when (i.getIntExtra(android.bluetooth.BluetoothAdapter.EXTRA_STATE, -1)) {
                        android.bluetooth.BluetoothAdapter.STATE_ON -> { chat.omcp.app.diag.Diag.log("Bluetooth on"); startTransport() }
                        android.bluetooth.BluetoothAdapter.STATE_OFF -> {
                            chat.omcp.app.diag.Diag.log("Bluetooth off", true)
                            transport?.stop(); transport = null
                            chat.omcp.app.diag.Diag.transportNote = "Bluetooth is off"
                        }
                    }
                }
            }
            registerReceiver(btReceiver, android.content.IntentFilter(android.bluetooth.BluetoothAdapter.ACTION_STATE_CHANGED))
        }
        startTransport()
        return START_STICKY
    }

    private var btReceiver: android.content.BroadcastReceiver? = null

    private fun startTransport() {
        val repo = (application as OmcpApplication).repository
        if (repo.core.identity() == null) { chat.omcp.app.diag.Diag.transportNote = "no identity yet"; return }
        val t = transport ?: BleMeshTransport(this, repo).also { transport = it }
        if (!t.running) t.start()
    }

    override fun onDestroy() {
        btReceiver?.let { runCatching { unregisterReceiver(it) } }
        btReceiver = null
        transport?.stop()
        transport = null
        super.onDestroy()
    }

    companion object {
        private const val ACTION_STOP = "chat.omcp.app.STOP_MESH"
        fun start(ctx: Context) = ctx.startForegroundService(Intent(ctx, MeshService::class.java))
        fun stop(ctx: Context) = ctx.startService(Intent(ctx, MeshService::class.java).setAction(ACTION_STOP))
    }
}

package chat.omcp.app.mesh

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import chat.omcp.app.R
import chat.omcp.app.core.IncomingText
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.notify.LockVisibility
import chat.omcp.app.notify.NotificationPolicy

/** Message notifications: sender + preview by default; "Private notifications" shows only a generic line. */
object MeshNotifications {
    const val CHANNEL_MESH = "mesh"
    const val CHANNEL_MESSAGES = "messages"

    fun ensureChannels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_MESH, ctx.getString(R.string.notif_channel_mesh), NotificationManager.IMPORTANCE_LOW),
        )
        nm.createNotificationChannel(
            NotificationChannel(CHANNEL_MESSAGES, ctx.getString(R.string.notif_channel_messages), NotificationManager.IMPORTANCE_DEFAULT),
        )
    }

    fun onMessage(ctx: Context, repo: MeshRepository, msg: IncomingText) {
        if (ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val sender = repo.contact(msg.contactId)?.displayName ?: "Contact"
        val c = NotificationPolicy.content(
            repo.appStore, msg.contactId, sender, msg.text,
            ctx.getString(R.string.app_name), ctx.getString(R.string.notif_new_message),
            burn = msg.burn != null,
        )
        val public = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(c.publicTitle)
            .setContentText(c.publicBody)
            .build()
        val b = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(c.title)
            .setContentText(c.body)
            .setVisibility(
                if (c.visibility == LockVisibility.PRIVATE) NotificationCompat.VISIBILITY_PRIVATE else NotificationCompat.VISIBILITY_PUBLIC,
            )
            .setPublicVersion(public)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .setAutoCancel(true)
        // TODO(avatar): setLargeIcon(contact avatar) only when c.showAvatar; no avatars stored yet.
        NotificationManagerCompat.from(ctx).notify(msg.contactId.hashCode(), b.build())
    }

    /**
     * A community member went live. Always the generic text ("Hopla · New
     * message"): no title, host or community name on the lock screen.
     */
    fun onLiveStarted(ctx: Context, streamIdHex: String) {
        if (ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val (title, body) = liveStartedText(ctx.getString(R.string.app_name), ctx.getString(R.string.notif_new_message))
        val n = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(title)
            .setContentText(body)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .setCategory(NotificationCompat.CATEGORY_SOCIAL)
            .setAutoCancel(true)
            .build()
        NotificationManagerCompat.from(ctx).notify(("live" + streamIdHex).hashCode(), n)
    }

    /** Lock-screen text for a live start: never content. */
    fun liveStartedText(appName: String, genericNewMessage: String): Pair<String, String> = appName to genericNewMessage
}

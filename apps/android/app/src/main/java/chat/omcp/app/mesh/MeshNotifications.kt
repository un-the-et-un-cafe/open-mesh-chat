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
     * A community member went live (DESIGN §5.124). Default mode: the unlocked
     * shade shows "{host} is live in {community}" and the title, with a Listen
     * action. The lock screen always shows the generic public version
     * ("Hopla · New message", VISIBILITY_PRIVATE). Private mode: generic in the
     * shade too. No "Mute community" action yet (provisional).
     */
    fun onLiveStarted(ctx: Context, repo: MeshRepository, streamIdHex: String, communityIdHex: String) {
        if (ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        val core = repo.core
        val community = runCatching { core.communities().firstOrNull { it.id == communityIdHex }?.name }.getOrNull() ?: ""
        val st = runCatching { core.liveStreams().firstOrNull { it.streamIdHex == streamIdHex } }.getOrNull()
        val host = st?.let { s ->
            runCatching { core.communityMembers(communityIdHex).firstOrNull { it.peerId == s.hostContactId }?.name }.getOrNull()
                ?: repo.contact(s.hostContactId)?.displayName
        } ?: ""
        val c = NotificationPolicy.liveContent(
            repo.appStore, communityIdHex, host, community, st?.title ?: "",
            ctx.getString(R.string.app_name), ctx.getString(R.string.notif_new_message),
            { h, k -> ctx.getString(R.string.live_notif_title, h, k) }, { t -> ctx.getString(R.string.live_notif_body, t) },
        )
        val public = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(c.publicTitle)
            .setContentText(c.publicBody)
            .build()
        val open = ctx.packageManager.getLaunchIntentForPackage(ctx.packageName)?.let {
            android.app.PendingIntent.getActivity(ctx, 0, it, android.app.PendingIntent.FLAG_IMMUTABLE or android.app.PendingIntent.FLAG_UPDATE_CURRENT)
        }
        val b = NotificationCompat.Builder(ctx, CHANNEL_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(c.title)
            .setContentText(c.body)
            .setVisibility(lockVisibility(c.visibility))
            .setPublicVersion(public)
            .setCategory(NotificationCompat.CATEGORY_SOCIAL)
            .setAutoCancel(true)
        if (open != null) {
            b.setContentIntent(open)
            if (!c.private) b.addAction(0, ctx.getString(R.string.live_notif_listen), open)
        }
        NotificationManagerCompat.from(ctx).notify(("live" + streamIdHex).hashCode(), b.build())
    }

    fun lockVisibility(v: LockVisibility): Int =
        if (v == LockVisibility.PRIVATE) NotificationCompat.VISIBILITY_PRIVATE else NotificationCompat.VISIBILITY_PUBLIC
}

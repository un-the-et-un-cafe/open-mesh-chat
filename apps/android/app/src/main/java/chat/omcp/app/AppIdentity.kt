package chat.omcp.app

import android.content.Context
import android.content.pm.PackageManager
import java.security.MessageDigest

/** Build/install identity shown in You › About and in Drive errors (no secrets). */
object AppIdentity {
    val commit: String get() = BuildConfig.GIT_COMMIT
    val version: String get() = "${BuildConfig.VERSION_NAME} (${BuildConfig.GIT_COMMIT})"

    fun sha1(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-1").digest(bytes).joinToString(":") { "%02X".format(it) }

    /** SHA-1 of the signing certificate the OAuth client must be registered with. */
    fun signingSha1(ctx: Context): String? = runCatching {
        val info = ctx.packageManager.getPackageInfo(ctx.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        info.signingInfo?.apkContentsSigners?.firstOrNull()?.toByteArray()?.let(::sha1)
    }.getOrNull()

    fun describe(ctx: Context): String = "package ${ctx.packageName}, SHA-1 ${signingSha1(ctx) ?: "unknown"}"
}

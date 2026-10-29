package chat.omcp.app.backup

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Tasks
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit

/** What the consent Activity returned: Android result code + result Intent (may be null). */
data class ConsentResult(val resultCode: Int, val data: Intent?)

/**
 * Hands a Google consent screen from a background Drive call to the visible Activity and
 * waits for the answer. [MainActivity] collects [pending], calls [take] (so a re-subscription
 * after the consent Activity returns does not launch it a second time), and calls [complete].
 */
class DriveConsentBroker {
    private val _pending = MutableStateFlow<PendingIntent?>(null)
    val pending: StateFlow<PendingIntent?> = _pending.asStateFlow()
    @Volatile private var waiter: CompletableFuture<ConsentResult?>? = null

    /** Blocking (background thread): show consent, return what came back, or null on timeout. */
    fun request(intent: PendingIntent, timeoutSec: Long = 180): ConsentResult? {
        val f = CompletableFuture<ConsentResult?>()
        waiter = f
        _pending.value = intent
        return try {
            f.get(timeoutSec, TimeUnit.SECONDS)
        } catch (_: Exception) {
            null
        } finally {
            _pending.value = null
            waiter = null
        }
    }

    /** Claims [pi] for launching exactly once; false if it was already launched or replaced. */
    fun take(pi: PendingIntent): Boolean = _pending.compareAndSet(pi, null)

    fun complete(resultCode: Int, data: Intent?) {
        _pending.value = null
        waiter?.complete(ConsentResult(resultCode, data))
    }
}

/** User-facing text for a Google Play services failure, always including Google's status code. */
object DriveAuthErrors {
    /** "package …, SHA-1 …" of the running app (set at startup) so the fix is copy-pasteable. */
    @Volatile var appIdentity: String? = null

    fun statusName(code: Int): String = when (code) {
        4 -> "SIGN_IN_REQUIRED"
        7 -> "NETWORK_ERROR"
        8 -> "INTERNAL_ERROR"
        10 -> "DEVELOPER_ERROR"
        13 -> "ERROR"
        14 -> "INTERRUPTED"
        15 -> "TIMEOUT"
        16 -> "CANCELED"
        17 -> "API_NOT_CONNECTED"
        else -> "STATUS_$code"
    }

    fun forStatus(code: Int, detail: String? = null): String {
        val hint = when (code) {
            8, 10 -> " — the OAuth client doesn't match this app (\"unregistered on API console\"). " +
                "In Google Cloud the Android client needs this package name and signing SHA-1, and " +
                "the Drive API must be enabled." + (appIdentity?.let { " This app: $it." } ?: "")
            16 -> " — the Google screen was closed before access was granted."
            7 -> " — check the Internet connection."
            else -> ""
        }
        val d = detail?.takeIf { it.isNotBlank() }?.let { " ($it)" } ?: ""
        return "Google Drive sign-in failed: ${statusName(code)} [code $code]$hint$d"
    }

    fun forResultCode(resultCode: Int): String =
        if (resultCode == 0) "Google Drive access was not granted: the Google screen returned " +
            "RESULT_CANCELED [result code 0]. If you didn't cancel, make sure this Google account " +
            "is a test user of the OAuth consent screen and the drive.appdata scope is added."
        else "Google Drive access was not granted [result code $resultCode]"
}

/**
 * Access tokens for `drive.appdata` through Google Identity Services' AuthorizationClient.
 * On Android the OAuth client is matched by package name + signing-certificate SHA-1 (an
 * "Android" client in Google Cloud), so no secret is embedded; [clientId] only marks the build
 * as configured. Must run off the main thread.
 */
class GoogleDriveTokenProvider(
    private val context: Context,
    private val consent: DriveConsentBroker,
) {
    private val client get() = Identity.getAuthorizationClient(context)
    private val request = AuthorizationRequest.builder()
        .setRequestedScopes(listOf(Scope(DriveAppDataClient.SCOPE)))
        .build()

    /** [interactive] = false never shows a consent screen (throws instead). */
    fun token(interactive: Boolean = true): String {
        val result = try {
            Tasks.await(client.authorize(request), 60, TimeUnit.SECONDS)
        } catch (e: ExecutionException) {
            throw asCloudError(e.cause ?: e)
        }
        result.accessToken?.let { return it }
        if (!interactive) throw CloudUnavailableException("Google Drive consent needed")
        val pi = result.pendingIntent ?: throw CloudUnavailableException("Google sign-in unavailable")
        val r = consent.request(pi)
            ?: throw CloudUnavailableException("Google Drive sign-in timed out")
        // Always parse the Intent: on failure it carries Google's Status even with RESULT_CANCELED.
        if (r.data != null) {
            val parsed = try {
                client.getAuthorizationResultFromIntent(r.data)
            } catch (e: ApiException) {
                throw CloudUnavailableException(DriveAuthErrors.forStatus(e.statusCode, e.status.statusMessage))
            }
            parsed.accessToken?.let { return it }
            // Consent done but no token in the Intent: ask again, now silently.
            Tasks.await(client.authorize(request), 60, TimeUnit.SECONDS).accessToken?.let { return it }
        }
        throw CloudUnavailableException(DriveAuthErrors.forResultCode(r.resultCode))
    }

    private fun asCloudError(e: Throwable): Exception =
        if (e is ApiException) CloudUnavailableException(DriveAuthErrors.forStatus(e.statusCode, e.status.statusMessage))
        else CloudUnavailableException("Google Drive sign-in failed: ${e.message ?: e.javaClass.simpleName}")

    /** Expired/revoked token: the next authorize() call returns a fresh one. */
    fun invalidate(@Suppress("UNUSED_PARAMETER") token: String) = Unit
}

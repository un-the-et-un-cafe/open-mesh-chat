package chat.omcp.app

import android.os.Bundle
import androidx.activity.result.IntentSenderRequest
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import kotlinx.coroutines.launch
import chat.omcp.app.security.ScreenSecurity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import chat.omcp.app.ui.OmcpRoot
import chat.omcp.app.ui.theme.OmcpTheme
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue

class MainActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        ScreenSecurity.secure(this)
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val app = application as OmcpApplication
        // Google consent for Drive (requested from a background backup call).
        val consentLauncher = registerForActivityResult(
            androidx.activity.result.contract.ActivityResultContracts.StartIntentSenderForResult(),
        ) { r -> app.driveConsent.complete(r.resultCode, r.data) }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                app.driveConsent.pending.collect { pi ->
                    // take(): StateFlow replays on every ON_START (e.g. when the consent Activity
                    // returns), which used to relaunch the account picker and drop the result.
                    if (pi != null && app.driveConsent.take(pi)) consentLauncher.launch(IntentSenderRequest.Builder(pi.intentSender).build())
                }
            }
        }
        val themePrefs = chat.omcp.app.ui.theme.ThemePrefs(this)
        setContent {
            val mode by themePrefs.mode.collectAsState()
            OmcpTheme(mode = mode) {
                OmcpRoot(
                    themeMode = mode,
                    onThemeMode = themePrefs::set,
                    repo = app.repository,
                    cloud = app.cloudBackup,
                    secureWindow = ::setSecure,
                    requestDeviceAuth = ::deviceAuth,
                )
            }
        }
    }

    /** FLAG_SECURE is always on app-wide (SS1); screens may only re-assert it, never clear it. */
    @Suppress("UNUSED_PARAMETER")
    private fun setSecure(on: Boolean) = ScreenSecurity.secure(window)

    /** Biometric or device PIN before showing the phrase again. */
    private fun deviceAuth(onSuccess: () -> Unit) {
        val prompt = BiometricPrompt(
            this, ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) = onSuccess()
            },
        )
        prompt.authenticate(
            BiometricPrompt.PromptInfo.Builder()
                .setTitle(getString(R.string.backup_view_phrase))
                .setAllowedAuthenticators(BIOMETRIC_STRONG or DEVICE_CREDENTIAL)
                .build(),
        )
    }
}

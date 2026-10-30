package chat.omcp.app.ui.screens

import android.Manifest
import android.bluetooth.BluetoothManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.BuildConfig
import chat.omcp.app.R
import chat.omcp.app.data.NewsNet
import chat.omcp.app.diag.Diag

/** Permissions the mesh and notifications need on Android 12+ (minSdk 31). */
fun diagPermissions(sdk: Int = Build.VERSION.SDK_INT): List<String> = buildList {
    add(Manifest.permission.BLUETOOTH_SCAN)
    add(Manifest.permission.BLUETOOTH_CONNECT)
    add(Manifest.permission.BLUETOOTH_ADVERTISE)
    if (sdk >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
}

/** Header lines of the diagnostics report: build, Bluetooth, permissions, network. No content, no keys. */
fun diagHeader(ctx: Context, myShortId: String?, contacts: Int): List<String> {
    val bt = ctx.getSystemService(BluetoothManager::class.java)?.adapter
    val loc = ctx.getSystemService(LocationManager::class.java)
    return buildList {
        add("Hopla ${BuildConfig.VERSION_NAME} commit ${BuildConfig.GIT_COMMIT} ${ctx.packageName}")
        add("Android ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT}) ${Build.MANUFACTURER} ${Build.MODEL}")
        add("me: ${myShortId ?: "no identity"}, contacts: $contacts")
        add("bluetooth: " + when {
            bt == null -> "no adapter"
            !bt.isEnabled -> "OFF"
            else -> "on, multi-adv=${runCatching { bt.isMultipleAdvertisementSupported }.getOrDefault(false)}"
        })
        add("location services: ${if (loc?.isLocationEnabled == true) "on" else "off"} (not needed on Android 12+)")
        diagPermissions().forEach { p ->
            add("perm ${p.substringAfterLast('.')}: ${if (ctx.checkSelfPermission(p) == PackageManager.PERMISSION_GRANTED) "granted" else "DENIED"}")
        }
        add("network: ${NewsNet.describe(ctx)}")
    }
}

/** You › About › Connection diagnostics: what the owner screenshots on both phones. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DiagnosticsScreen(myShortId: String?, contacts: Int, onRequestPermissions: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    var tick by remember { mutableStateOf(0) }
    var copied by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { while (true) { kotlinx.coroutines.delay(1_000); tick++ } }
    val report = remember(tick) { Diag.report(diagHeader(ctx, myShortId, contacts)) }
    Scaffold(topBar = {
        TopAppBar(title = { Text(stringResource(R.string.diag_title)) }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        })
    }) { pad ->
        Column(Modifier.fillMaxSize().padding(pad).padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.diag_body), style = MaterialTheme.typography.bodySmall)
            Button(onClick = {
                (ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Hopla diagnostics", report))
                copied = true
            }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(if (copied) R.string.live_report_copied_short else R.string.diag_copy)) }
            OutlinedButton(onClick = onRequestPermissions, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.diag_permissions)) }
            SelectionContainer { Text(report, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall) }
        }
    }
}

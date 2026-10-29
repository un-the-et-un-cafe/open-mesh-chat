package chat.omcp.app.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Contactless
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Tag
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.QrImage
import chat.omcp.app.ui.components.SafetyNumberBlock
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.ContactAddViewModel
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions

/** Short PeerID form `omcp1:xxxx…xxxx` (hex in the beta). */
fun shortPeerId(hex: String) = if (hex.length > 8) "omcp1:${hex.take(4)}…${hex.takeLast(4)}" else "omcp1:$hex"

/** Contact-add chooser + scan + preview sheet (DESIGN_SPEC §5.19–§5.22). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ContactAddScreen(
    vm: ContactAddViewModel,
    onBack: () -> Unit,
    onMyQr: () -> Unit,
    onOpenChat: (String) -> Unit,
) {
    val s by vm.state.collectAsState()
    var pasteOpen by remember { mutableStateOf(false) }
    var pasted by remember { mutableStateOf("") }
    val scan = androidx.activity.compose.rememberLauncherForActivityResult(ScanContract()) { r ->
        r.contents?.let(vm::onScanned)
    }
    val launchScan = {
        scan.launch(
            ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Point at their Hopla QR code. It scans automatically.")
                .setBeepEnabled(false)
                .setOrientationLocked(false),
        )
    }
    val nfc = LocalContext.current.packageManager.hasSystemFeature("android.hardware.nfc")

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.contact_add_title)) },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Text(
                stringResource(R.string.contact_add_subtitle),
                style = MaterialTheme.typography.bodyMedium,
                color = LocalOmcpColors.current.ink2,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            )
            if (s.notOmcp) {
                Callout(
                    stringResource(R.string.qr_not_omcp_title) + "\n" + stringResource(R.string.qr_not_omcp_body),
                    CalloutKind.Danger,
                    Modifier.padding(horizontal = 16.dp),
                )
            }
            if (s.qrStale) {
                Callout(stringResource(R.string.qr_stale), CalloutKind.Warn, Modifier.padding(horizontal = 16.dp))
            }
            HorizontalDivider()
            Row(R.string.contact_add_scan_qr, R.string.contact_add_scan_qr_desc, Icons.Default.QrCodeScanner) { launchScan() }
            Row(R.string.contact_add_my_qr, R.string.contact_add_my_qr_desc, Icons.Default.QrCode, onMyQr)
            if (nfc) {
                // TODO(M5.1): NFC HCE exchange of the same omcp:1: payload.
                Row(R.string.contact_add_nfc, R.string.contact_add_nfc_desc, Icons.Default.Contactless) {}
            }
            Text(
                stringResource(R.string.advanced),
                style = MaterialTheme.typography.labelLarge,
                modifier = Modifier.padding(start = 16.dp, top = 12.dp),
            )
            Row(R.string.contact_add_peer_id, R.string.contact_add_peer_id_desc, Icons.Default.Tag) { pasteOpen = !pasteOpen }
            if (pasteOpen) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(pasted, { pasted = it }, label = { Text("omcp:1:…") }, modifier = Modifier.fillMaxWidth())
                    Button(onClick = { vm.onScanned(pasted) }) { Text(stringResource(R.string.cta_continue)) }
                }
            }
        }
    }

    s.preview?.let { p ->
        val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
        ModalBottomSheet(onDismissRequest = vm::cancel, sheetState = sheet) {
            Column(
                Modifier.padding(20.dp).verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(p.contact.displayName, style = MaterialTheme.typography.titleLarge)
                Text(shortPeerId(p.contact.id), fontFamily = FontFamily.Monospace, color = LocalOmcpColors.current.ink2)
                val old = p.mismatchWith
                if (old != null) {
                    Text(stringResource(R.string.safety_mismatch_title), color = LocalOmcpColors.current.danger)
                    Callout(stringResource(R.string.safety_mismatch_body, old.displayName), CalloutKind.Danger)
                    val differs = p.contact.safetyNumber.indices.filter { p.contact.safetyNumber[it] != old.safetyNumber.getOrNull(it) }.toSet()
                    SafetyNumberBlock(p.contact.safetyNumber, differs)
                    Button(onClick = { vm.cancel(); launchScan() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.scan_again)) }
                    OutlinedButton(onClick = vm::cancel, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.dont_trust)) }
                } else {
                    Text(
                        if (p.contact.verified) stringResource(R.string.verified_contact) else stringResource(R.string.unverified),
                        color = if (p.contact.verified) LocalOmcpColors.current.ok else LocalOmcpColors.current.warn,
                    )
                    Text(stringResource(R.string.safety_number), style = MaterialTheme.typography.labelLarge)
                    SafetyNumberBlock(p.contact.safetyNumber)
                    BodyText(stringResource(R.string.safety_compare, p.contact.displayName))
                    // The BLE responder only accepts contacts: both phones must add each other.
                    Callout(stringResource(R.string.add_mutual_hint, p.contact.displayName), CalloutKind.Warn)
                    Button(onClick = { vm.verifyAndChat()?.let(onOpenChat) }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(R.string.verify_and_chat))
                    }
                    OutlinedButton(onClick = { vm.chatWithoutVerifying()?.let(onOpenChat) }, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(R.string.chat_without_verifying))
                    }
                }
                TextButton(onClick = vm::cancel, modifier = Modifier.align(Alignment.CenterHorizontally)) { Text(stringResource(R.string.cancel)) }
            }
        }
    }
}

@Composable
private fun Row(title: Int, desc: Int, icon: androidx.compose.ui.graphics.vector.ImageVector, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(stringResource(title)) },
        supportingContent = { Text(stringResource(desc)) },
        leadingContent = { Icon(icon, contentDescription = null) },
        trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
    )
}

/** Inline QR block reused by onboarding Done and My QR. */
@Composable
fun MyQrBlock(payload: String, name: String, peerIdHex: String) {
    val ctx = LocalContext.current
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
        QrImage(payload)
        Text(name, style = MaterialTheme.typography.titleMedium)
        Text(shortPeerId(peerIdHex), fontFamily = FontFamily.Monospace, color = LocalOmcpColors.current.ink2)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = {
                ctx.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, payload), null))
            }) { Text(stringResource(R.string.share)) }
            OutlinedButton(onClick = {
                ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("PeerID", payload))
            }) { Text(stringResource(R.string.copy_peer_id)) }
        }
    }
}

@Composable
private fun Row(horizontalArrangement: Arrangement.Horizontal, content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit) =
    androidx.compose.foundation.layout.Row(horizontalArrangement = horizontalArrangement, content = content)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MyQrScreen(vm: ContactAddViewModel, onBack: () -> Unit, onScanTheirs: () -> Unit) {
    val activity = LocalContext.current as? ComponentActivity
    DisposableEffect(Unit) { // raise brightness while shown, restore on leave
        val w = activity?.window
        val old = w?.attributes?.screenBrightness
        w?.attributes = w?.attributes?.apply { screenBrightness = 1f }
        onDispose { w?.attributes = w?.attributes?.apply { screenBrightness = old ?: WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE } }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.contact_add_my_qr)) },
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) } },
            )
        },
    ) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).padding(24.dp).verticalScroll(rememberScrollState()),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            val id = vm.identity
            if (id != null) MyQrBlock(vm.qrPayload, id.displayName, id.peerIdHex)
            Callout(stringResource(R.string.my_qr_hint))
            Callout(stringResource(R.string.my_qr_mutual), CalloutKind.Warn)
            BodyText(stringResource(R.string.my_qr_privacy))
            TextButton(onClick = onScanTheirs) { Text(stringResource(R.string.scan_theirs)) }
        }
    }
}

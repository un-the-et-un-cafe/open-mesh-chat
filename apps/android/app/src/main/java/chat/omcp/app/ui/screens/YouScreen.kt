package chat.omcp.app.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.Identity
import chat.omcp.app.ui.theme.LocalOmcpColors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun YouScreen(
    identity: Identity?,
    suiteLabel: String,
    privateNotifications: Boolean,
    onSetPrivateNotifications: (Boolean) -> Unit,
    hideOnLockScreen: Boolean,
    onSetHideOnLockScreen: (Boolean) -> Unit,
    onGateway: () -> Unit,
    onSecurity: () -> Unit,
    onBackup: () -> Unit,
    onStorage: () -> Unit = {},
    onRelay: () -> Unit = {},
    onMyQr: () -> Unit,
) {
    val c = LocalOmcpColors.current
    var priv by remember { mutableStateOf(privateNotifications) }
    var hideLock by remember { mutableStateOf(hideOnLockScreen) }
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_you)) }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Column(Modifier.padding(16.dp)) {
                Text(identity?.displayName ?: "You", style = MaterialTheme.typography.headlineSmall)
                Text(
                    "PeerID · " + shortPeerId(identity?.peerIdHex ?: ""),
                    fontFamily = FontFamily.Monospace,
                    color = c.ink2,
                    style = MaterialTheme.typography.bodySmall,
                    modifier = Modifier.padding(top = 4.dp),
                )
                Text("Suite: $suiteLabel", color = c.ink3, style = MaterialTheme.typography.labelSmall)
            }
            HorizontalDivider()
            Item(stringResource(R.string.contact_add_my_qr), stringResource(R.string.contact_add_my_qr_desc), onMyQr)
            Item(stringResource(R.string.gateway_title), "Wi‑Fi-only default · 500 MB/day cap", onGateway)
            Item(stringResource(R.string.security_centre), "Safety numbers, session, block list", onSecurity)
            Item(stringResource(R.string.backup_settings_title), stringResource(R.string.backup_settings_desc), onBackup)
            Item(stringResource(R.string.storage_title), stringResource(R.string.storage_keep_title), onStorage)
            Item(stringResource(R.string.relay_title), stringResource(R.string.relay_toggle_desc), onRelay)
            ListItem(
                headlineContent = { Text(stringResource(R.string.notif_private)) },
                supportingContent = { Text(stringResource(R.string.notif_private_desc)) },
                trailingContent = { Switch(checked = priv, onCheckedChange = { priv = it; onSetPrivateNotifications(it) }) },
            )
            ListItem(
                headlineContent = { Text(stringResource(R.string.notif_hide_lock)) },
                supportingContent = { Text(stringResource(R.string.notif_hide_lock_desc)) },
                trailingContent = { Switch(checked = hideLock, onCheckedChange = { hideLock = it; onSetHideOnLockScreen(it) }) },
            )
            ListItem(
                headlineContent = { Text("About") },
                supportingContent = { Text("Hopla · built on the open OMCP protocol · MIT · beta (experimental)") },
            )
        }
    }
}

@Composable
private fun Item(title: String, desc: String, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = { Text(desc) },
        trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, null) },
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
    )
}

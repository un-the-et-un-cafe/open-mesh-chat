package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.MeshRelayViewModel
import chat.omcp.app.ui.vm.StorageViewModel

/** Settings > Mesh relay. No mockup exists yet (design gap): plain Material layout. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun MeshRelaySettingsScreen(vm: MeshRelayViewModel, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    val s by vm.state.collectAsState()
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(stringResource(R.string.relay_title)) },
            navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null) } },
        )
    }) { padding ->
        Column(
            Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            ListItem(
                headlineContent = { Text(stringResource(R.string.relay_toggle)) },
                supportingContent = { Text(stringResource(R.string.relay_toggle_desc)) },
                trailingContent = { Switch(checked = s.prefs.enabled, onCheckedChange = vm::setEnabled) },
            )
            Text(stringResource(R.string.relay_cap_title), style = MaterialTheme.typography.labelLarge)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                MeshRelayViewModel.CAP_OPTIONS.forEach { b ->
                    FilterChip(
                        selected = s.prefs.capBytes == b,
                        enabled = s.prefs.enabled,
                        onClick = { vm.setCap(b) },
                        label = { Text(StorageViewModel.formatBytes(b)) },
                    )
                }
            }
            Text(stringResource(R.string.relay_privacy), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            Text(
                stringResource(R.string.relay_usage, s.stats.carried, StorageViewModel.formatBytes(s.stats.carriedBytes), s.stats.neighbours),
                color = c.ink3, style = MaterialTheme.typography.labelSmall,
            )
        }
    }
}

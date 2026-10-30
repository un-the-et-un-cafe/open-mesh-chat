package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import chat.omcp.app.R
import chat.omcp.app.ui.vm.MeshRelayViewModel

/**
 * Settings > Mesh relay (REQUIREMENTS RL4): only "Storage used for relaying: X MB", for
 * information. No switch and no cap picker: the relay is always on (RL1) and the cap is
 * automatic (RL2). No mockup exists yet (DESIGN_SPEC Q36 placeholder): plain Material row.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MeshRelaySettingsScreen(vm: MeshRelayViewModel, onBack: () -> Unit) {
    val s by vm.state.collectAsState()
    LaunchedEffect(Unit) { vm.refresh() }
    Scaffold(topBar = {
        androidx.compose.material3.TopAppBar(
            title = { Text(stringResource(R.string.relay_title)) },
            navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null) } },
        )
    }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            ListItem(
                headlineContent = {
                    Text(stringResource(R.string.relay_storage_used, MeshRelayViewModel.formatMb(s.storageUsedBytes)))
                },
            )
        }
    }
}

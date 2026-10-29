package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.TeamLogUi
import chat.omcp.app.ui.theme.LocalOmcpColors
import java.text.DateFormat
import java.util.Date

/** Reason code (§23.9(7)) → string resource. */
fun moderationReasonRes(code: Int): Int = when (code) {
    1 -> R.string.mod_reason_1
    2 -> R.string.mod_reason_2
    3 -> R.string.mod_reason_3
    4 -> R.string.mod_reason_4
    5 -> R.string.mod_reason_5
    else -> R.string.mod_reason_9
}

/** TM2 / §23.9(5): the public "Removed communities" transparency log. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RemovedCommunitiesScreen(core: OmcpCore, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    var log by remember { mutableStateOf(core.moderationLog()) }
    var input by remember { mutableStateOf("") }
    var result by remember { mutableStateOf<String?>(null) }
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(stringResource(R.string.mod_log_title)) },
            navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null) } },
        )
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding)) {
            item {
                Text(stringResource(R.string.mod_log_desc), color = c.ink2,
                    style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp))
            }
            if (log.isEmpty()) {
                item {
                    Text(stringResource(R.string.mod_log_empty), color = c.ink3, modifier = Modifier.padding(16.dp))
                }
            }
            items(log) { e -> LogRow(e) ; HorizontalDivider() }
            if (core.moderationHasTeam()) {
                // Anyone may hand over a team-signed action (it is verified against the shipped
                // whitelist before it is applied or gossiped); only the key holder can make one.
                item {
                    Column(Modifier.padding(16.dp)) {
                        OutlinedTextField(input, { input = it.trim() }, Modifier.fillMaxWidth(),
                            label = { Text(stringResource(R.string.mod_publish_label)) }, singleLine = true)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Button(onClick = {
                                result = core.moderationPublish(input)
                                log = core.moderationLog()
                            }, enabled = input.isNotEmpty(), modifier = Modifier.padding(top = 8.dp)) {
                                Text(stringResource(R.string.mod_publish))
                            }
                            result?.let { Text(it, color = c.ink2, modifier = Modifier.padding(start = 12.dp)) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LogRow(e: TeamLogUi) {
    val c = LocalOmcpColors.current
    val date = DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(e.appliedMs))
    val head = if (e.kind == "restore") stringResource(R.string.mod_restored)
    else stringResource(moderationReasonRes(e.reasonCode))
    ListItem(
        headlineContent = { Text(head) },
        supportingContent = {
            Column {
                Text("Community " + e.communityIdHex.take(12) + "… · " + date, color = c.ink2)
                if (e.note.isNotBlank()) Text(e.note, color = c.ink2)
                Text(stringResource(R.string.mod_signed_by) + " " + e.signedBy.joinToString(), color = c.ink3,
                    style = MaterialTheme.typography.labelSmall)
            }
        },
    )
}

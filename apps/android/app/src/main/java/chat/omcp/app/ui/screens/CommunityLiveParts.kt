package chat.omcp.app.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.runtime.CompositionLocalProvider
import chat.omcp.app.R
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind

/** Communities are capped at 32 members (OpenMLS small group, docs/LIVE.md). */
const val COMMUNITY_MAX = 32

/** §5.126: stream id (32 hex) in groups of 4 for reading out and comparing. */
fun groupStreamId(hex: String): String = hex.lowercase().chunked(4).joinToString(" ")

val LIVE_REPORT_REASONS = listOf(
    R.string.live_report_r1, R.string.live_report_r2, R.string.live_report_r3, R.string.live_report_r4, R.string.live_report_r5,
)

/** Plain explanatory sheet (Appeal placeholder, How moderation works). */
@Composable
fun InfoDialog(title: String, body: String, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { Text(body) },
        confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.ok)) } },
    )
}

/** §5.122 confirm sheet: danger + Cancel. */
@Composable
fun RemoveFromLiveDialog(name: String, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.live_rm_t, name)) },
        text = { Text(stringResource(R.string.live_rm_d)) },
        confirmButton = {
            Button(onClick = onConfirm, colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error)) {
                Text(stringResource(R.string.live_remove_viewer))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** §5.123: host sees reason + Appeal (placeholder, §23.5 not built); viewer sees live_taken_down only. */
@Composable
fun LiveTakedownNotice(mine: Boolean, reasonCode: Int?, onOk: () -> Unit, modifier: Modifier = Modifier) {
    var appeal by remember { mutableStateOf(false) }
    var learn by remember { mutableStateOf(false) }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (mine) {
            Callout(stringResource(R.string.live_td_t) + "\n" + stringResource(R.string.live_td_host), CalloutKind.Danger)
            reasonCode?.let { Text(stringResource(R.string.live_td_reason, stringResource(moderationReasonRes(it)))) }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { appeal = true }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.live_appeal)) }
                TextButton(onClick = { learn = true }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.mod_learn)) }
                Button(onClick = onOk, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.ok)) }
            }
        } else {
            Callout(stringResource(R.string.live_taken_down), CalloutKind.Warn)
            Button(onClick = onOk, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.ok)) }
        }
    }
    if (appeal) InfoDialog(stringResource(R.string.live_appeal_t), stringResource(R.string.live_appeal_d)) { appeal = false }
    if (learn) InfoDialog(stringResource(R.string.mod_learn), stringResource(R.string.mod_learn_d)) { learn = false }
}

/** Plain text a report carries (the user picks where to send it; no anonymity claim). */
fun liveReportText(streamIdHex: String, reason: String): String =
    "Hopla live report\nStream ID: ${streamIdHex.lowercase()}\nReason: $reason"

/**
 * §5.126 Report this live: stream id (LTR, monospace, selectable), Copy, reason,
 * Send report. There is no report inbox in the protocol yet, so Send opens the
 * system share sheet with the id + reason.
 */
@Composable
fun ReportLiveDialog(streamIdHex: String, onDismiss: () -> Unit) {
    val ctx = LocalContext.current
    var reason by remember { mutableIntStateOf(0) }
    var copied by remember { mutableStateOf(false) }
    val reasonText = stringResource(LIVE_REPORT_REASONS[reason])
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.live_report)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.live_report_d))
                Text(stringResource(R.string.live_report_sid), style = MaterialTheme.typography.labelMedium)
                CompositionLocalProvider(LocalLayoutDirection provides LayoutDirection.Ltr) {
                    SelectionContainer { Text(groupStreamId(streamIdHex), fontFamily = FontFamily.Monospace) }
                }
                OutlinedButton(onClick = {
                    (ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager)
                        .setPrimaryClip(ClipData.newPlainText("stream id", streamIdHex.lowercase()))
                    copied = true
                }, Modifier.heightIn(min = 48.dp)) {
                    Text(stringResource(if (copied) R.string.live_report_copied else R.string.live_report_copy))
                }
                Text(stringResource(R.string.live_report_why), style = MaterialTheme.typography.titleSmall)
                Column(Modifier.selectableGroup()) {
                    LIVE_REPORT_REASONS.forEachIndexed { i, r ->
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 48.dp)
                                .selectable(selected = reason == i, onClick = { reason = i }, role = Role.RadioButton),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(selected = reason == i, onClick = null)
                            Text(stringResource(r), Modifier.padding(start = 8.dp))
                        }
                    }
                }
                Text(stringResource(R.string.live_report_note), style = MaterialTheme.typography.bodySmall)
            }
        },
        confirmButton = {
            Button(onClick = {
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, liveReportText(streamIdHex, reasonText))
                ctx.startActivity(Intent.createChooser(send, null))
                onDismiss()
            }) { Text(stringResource(R.string.live_report_send)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

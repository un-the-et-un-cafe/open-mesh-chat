package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.InviteCandidateUi
import chat.omcp.app.core.InviteRequestUi
import chat.omcp.app.core.InviteSentUi
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.OpenCommunityUi
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.theme.LocalOmcpColors

/*
 * §20.7 private invites (TM13–TM17). Designy's pack has `group-invite` (pick
 * from community members); the per-community setting row and the incoming
 * request card below are minimal screens consistent with §5U (no mockup yet).
 */

@Composable
fun inviteStatusLabel(status: String): String = stringResource(
    when (status) {
        "Accepted" -> R.string.inv_status_accepted
        "Declined" -> R.string.inv_status_declined
        "Expired" -> R.string.inv_status_expired
        else -> R.string.inv_status_pending
    }
)

/** TM15: "Invites from people who aren't contacts", per community, off by default. */
@Composable
fun InviteSettingRow(core: OmcpCore, communityId: String) {
    val c = LocalOmcpColors.current
    var on by remember(communityId) { mutableStateOf(core.inviteSettingEnabled(communityId)) }
    val label = stringResource(R.string.inv_setting_t)
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(label, style = MaterialTheme.typography.titleSmall)
            Text(stringResource(R.string.inv_setting_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
        }
        Switch(checked = on, onCheckedChange = { want -> if (core.setInviteSetting(communityId, want)) on = want })
    }
}

/** TM14: requests, "<name> from <community> invites you to a private group"; nothing happens until Accept. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun InviteRequestsSection(core: OmcpCore) {
    val c = LocalOmcpColors.current
    val ctx = LocalContext.current
    var list by remember { mutableStateOf(listOf<InviteRequestUi>()) }
    LaunchedEffect(Unit) { while (true) { list = core.inviteRequests(); kotlinx.coroutines.delay(1_500) } }
    if (list.isEmpty()) return
    Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.inv_req_h), style = MaterialTheme.typography.titleSmall)
        list.forEach { r ->
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    val who = r.inviterName ?: pseudonymLabel(r.inviterKey)
                    Text(stringResource(R.string.inv_req_line, who, r.communityName), style = MaterialTheme.typography.titleSmall)
                    if (r.groupName.isNotBlank()) Text(r.groupName, color = c.ink2)
                    if (r.text.isNotBlank()) Text(r.text, color = c.ink2, style = MaterialTheme.typography.bodySmall)
                    if (r.accepted) Text(stringResource(R.string.inv_waiting), color = c.ink3)
                    else FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(onClick = { core.inviteAnswer(r.offerId, true); list = core.inviteRequests() }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.inv_accept)) }
                        OutlinedButton(onClick = { core.inviteAnswer(r.offerId, false); list = core.inviteRequests() }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.inv_decline)) }
                        TextButton(onClick = { core.inviteReport(r.offerId)?.let { shareReport(ctx, it) } }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.inv_report)) }
                        TextButton(onClick = { core.inviteBlock(r.offerId); list = core.inviteRequests() }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.inv_block), color = c.danger) }
                    }
                }
            }
        }
    }
}

/** Short pseudonym for a member key (same 8-hex style as MemberIdentity). */
fun pseudonymLabel(keyHex: String): String = "#" + keyHex.take(8)

/** `group-invite`: founder picks members of their communities (not only contacts), x/32. */
@Composable
fun GroupInviteFromCommunities(core: OmcpCore, groupId: String, full: Boolean) {
    val c = LocalOmcpColors.current
    var comms by remember { mutableStateOf(listOf<OpenCommunityUi>()) }
    var cands by remember { mutableStateOf(mapOf<String, List<InviteCandidateUi>>()) }
    var sent by remember { mutableStateOf(listOf<InviteSentUi>()) }
    var notice by remember { mutableStateOf(false) }
    LaunchedEffect(groupId) {
        while (true) {
            comms = core.openCommunities()
            cands = comms.associate { it.id to core.inviteCandidates(it.id) }
            sent = core.invitesSent().filter { it.groupId == groupId }
            kotlinx.coroutines.delay(2_000)
        }
    }
    Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        if (notice) Callout(stringResource(R.string.inv_mesh_only), CalloutKind.Warn)
        sent.forEach { s -> Text(inviteStatusLabel(s.status), color = c.ink3, style = MaterialTheme.typography.bodySmall) }
        comms.forEach { k ->
            Text(stringResource(R.string.inv_members_h, k.name), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
            val list = cands[k.id].orEmpty()
            if (list.isEmpty()) Text(stringResource(R.string.inv_none), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            list.forEach { m ->
                var done by remember(k.id, m.key) { mutableStateOf(false) }
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(m.contactName ?: pseudonymLabel(m.key), Modifier.weight(1f))
                    TextButton(enabled = !done, modifier = Modifier.heightIn(min = 48.dp).semantics { if (full) disabled() },
                        onClick = {
                            if (!full) core.inviteSend(groupId, k.id, m.key, "")?.let { done = true; notice = true; sent = core.invitesSent().filter { it.groupId == groupId } }
                        }) { Text(stringResource(if (done) R.string.community_invited else R.string.community_invite)) }
                }
            }
        }
    }
}

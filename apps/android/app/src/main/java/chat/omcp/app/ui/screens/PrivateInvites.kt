package chat.omcp.app.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Badge
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.CommunityUi
import chat.omcp.app.core.InviteCandidateUi
import chat.omcp.app.core.InviteRequestUi
import chat.omcp.app.core.InviteSentUi
import chat.omcp.app.core.LiveStreamUi
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.OpenCommunityUi
import chat.omcp.app.core.OpenMemberUi
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.Pseudonym
import chat.omcp.app.ui.theme.LocalOmcpColors

/* §20.7 private invites (TM13–TM17), Designy §5.150–5.155. */

/** Status code → label + icon, exactly as built (no "Delivered": there is no receipt). */
data class InviteStatusView(val label: Int, val icon: ImageVector)

fun inviteStatusView(status: String): InviteStatusView = when (status) {
    "Accepted" -> InviteStatusView(R.string.inv_status_accepted, Icons.Filled.Check)
    "Declined" -> InviteStatusView(R.string.inv_status_declined, Icons.Filled.Close)
    "Expired" -> InviteStatusView(R.string.inv_status_expired, Icons.Filled.Info)
    // A block or a limit drop looks the same as slow delivery (§5.152).
    else -> InviteStatusView(R.string.inv_status_pending, Icons.Filled.Sync)
}

@Composable
fun inviteStatusLabel(status: String): String = stringResource(inviteStatusView(status).label)

/** Days left until [expiresMs], at least 1 while not expired. */
fun daysLeft(expiresMs: Long, nowMs: Long): Int = (((expiresMs - nowMs).coerceAtLeast(0) + 86_399_999) / 86_400_000).toInt()

/** `comm-settings-invites` (§5.151): "Invites from people who aren't contacts", per community, off by default. */
@Composable
fun InviteSettingRow(core: OmcpCore, communityId: String, inCommunity: Boolean = false, label: String? = null) {
    val c = LocalOmcpColors.current
    var on by remember(communityId) { mutableStateOf(core.inviteSettingEnabled(communityId)) }
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            if (inCommunity) Text(stringResource(R.string.u_privacy), color = c.ink2, style = MaterialTheme.typography.labelMedium)
            Text(label ?: stringResource(R.string.inv_setting_t), style = MaterialTheme.typography.titleSmall)
            Text(stringResource(R.string.inv_setting_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            if (inCommunity) Text(stringResource(R.string.u_set_also), color = c.ink3, style = MaterialTheme.typography.bodySmall)
        }
        Switch(checked = on, onCheckedChange = { want -> if (core.setInviteSetting(communityId, want)) on = want })
    }
}

/** Settings › Privacy (TM15): the same per-community switch, for every joined community. */
@Composable
fun PrivacyInvitesSection(core: OmcpCore) {
    val list = remember { core.openCommunities() }
    if (list.isEmpty()) return
    Column {
        Text(stringResource(R.string.u_privacy), style = MaterialTheme.typography.titleSmall,
            modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp))
        Text(stringResource(R.string.inv_setting_t), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(horizontal = 16.dp))
        list.forEach { k -> InviteSettingRow(core, k.id, label = k.name) }
    }
}

/** `live-entry-chats`: the Requests row at the top of Chats with a count badge. */
@Composable
fun InviteRequestsRow(core: OmcpCore, onOpen: () -> Unit) {
    var n by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) { while (true) { n = core.inviteRequests().count { !it.accepted }; kotlinx.coroutines.delay(1_500) } }
    if (n == 0) return
    Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).clickable(onClick = onOpen).padding(horizontal = 16.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(Icons.Filled.Lock, null)
        Column(Modifier.weight(1f)) {
            Text(stringResource(R.string.u_inv_requests), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(R.string.u_inv_req_n, n), style = MaterialTheme.typography.bodySmall)
        }
        Badge { Text("$n") }
    }
}

/** Chats list: "{host} is live in {group}" + Join (role=status; icon + text). */
@Composable
fun LiveBannerChats(core: OmcpCore, groups: List<CommunityUi>, names: Map<String, String>, onOpenGroup: (String) -> Unit, onOpenLive: () -> Unit) {
    val c = LocalOmcpColors.current
    var lives by remember { mutableStateOf(listOf<LiveStreamUi>()) }
    LaunchedEffect(Unit) { while (true) { lives = core.liveStreams().filter { !it.mine && !it.listening }; kotlinx.coroutines.delay(1_500) } }
    if (lives.isEmpty()) return
    Column(Modifier.padding(horizontal = 16.dp, vertical = 4.dp).semantics { liveRegion = LiveRegionMode.Polite }) {
        lives.forEach { l ->
            val host = names[l.hostContactId] ?: Pseudonym.of(l.hostContactId).name
            val group = groups.firstOrNull { it.id == l.communityId }?.name
            Card(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
                Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.live_badge), color = c.danger, style = MaterialTheme.typography.labelMedium)
                    Text(if (group != null) stringResource(R.string.u_live_banner, host, group) else stringResource(R.string.u_live_1to1, host),
                        Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
                    Button(onClick = { if (l.communityId.isNotEmpty()) onOpenGroup(l.communityId) else onOpenLive() },
                        Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_live_join)) }
                }
            }
        }
        Text(stringResource(R.string.u_live_where), color = c.ink3, style = MaterialTheme.typography.bodySmall)
    }
}

/** `invite-request` / `invite-accepted` / declined / blocked (§5.150). */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun InviteRequestsScreen(core: OmcpCore, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    val ctx = LocalContext.current
    var list by remember { mutableStateOf(listOf<InviteRequestUi>()) }
    // Undo toast: the action runs only after the toast times out (≥ 5 s).
    var pending by remember { mutableStateOf<Triple<InviteRequestUi, Boolean, Long>?>(null) }
    LaunchedEffect(Unit) { while (true) { list = core.inviteRequests(); kotlinx.coroutines.delay(1_500) } }
    LaunchedEffect(pending) {
        val p = pending ?: return@LaunchedEffect
        kotlinx.coroutines.delay(6_000)
        if (p.second) core.inviteBlock(p.first.offerId) else core.inviteAnswer(p.first.offerId, false)
        pending = null
        list = core.inviteRequests()
    }
    Scaffold(topBar = {
        TopAppBar(title = { Text(stringResource(R.string.inv_req_h)) }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        })
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            pending?.let { p ->
                item {
                    val who = p.first.inviterName ?: Pseudonym.of(p.first.inviterKey).name
                    // role=status, focus not moved.
                    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp).semantics { liveRegion = LiveRegionMode.Polite },
                        verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(if (p.second) stringResource(R.string.u_inv_blocked_t, who) else stringResource(R.string.u_inv_declined_t),
                                style = MaterialTheme.typography.titleSmall)
                            Text(if (p.second) stringResource(R.string.u_inv_blocked_d) else stringResource(R.string.u_inv_declined_d, who),
                                color = c.ink2, style = MaterialTheme.typography.bodySmall)
                        }
                        TextButton(onClick = { pending = null }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_undo)) }
                    }
                }
            }
            items(list.filter { it.offerId != pending?.first?.offerId }, key = { it.offerId }) { r ->
                val id = remember(r.inviterKey) { Pseudonym.of(r.inviterKey) }
                val who = r.inviterName ?: id.name
                Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                    Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            chat.omcp.app.ui.components.Identicon(id)
                            Column(Modifier.weight(1f)) {
                                Text(stringResource(R.string.inv_req_line, who, r.communityName), style = MaterialTheme.typography.titleSmall)
                                Text(id.shortKey, color = c.ink3, style = MaterialTheme.typography.labelSmall)
                            }
                        }
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Filled.Lock, null, Modifier.size(16.dp))
                            Text(stringResource(R.string.u_chip_groups), style = MaterialTheme.typography.labelMedium)
                            Text("# " + r.communityName, style = MaterialTheme.typography.labelMedium)
                        }
                        if (r.groupName.isNotBlank()) Text(stringResource(R.string.u_inv_group, r.groupName), color = c.ink2)
                        Text(stringResource(R.string.u_inv_count), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                        if (r.text.isNotBlank()) Text(r.text, color = c.ink2, style = MaterialTheme.typography.bodySmall)
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Filled.Lock, null, Modifier.size(16.dp))
                            Text(stringResource(R.string.u_inv_e2e), color = c.ink3, style = MaterialTheme.typography.bodySmall)
                        }
                        if (r.accepted) {
                            Column(Modifier.semantics { liveRegion = LiveRegionMode.Polite }) {
                                Text(stringResource(R.string.inv_waiting), color = c.ink2, style = MaterialTheme.typography.titleSmall)
                                Text(stringResource(R.string.u_inv_accepted_d, r.groupName.ifBlank { r.communityName }), color = c.ink3,
                                    style = MaterialTheme.typography.bodySmall)
                            }
                        } else {
                            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                Button(onClick = { core.inviteAnswer(r.offerId, true); list = core.inviteRequests() }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.inv_accept)) }
                                OutlinedButton(onClick = { pending = Triple(r, false, System.currentTimeMillis()) }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.inv_decline)) }
                                var more by remember(r.offerId) { mutableStateOf(false) }
                                IconButton(onClick = { more = true }) {
                                    Icon(Icons.Filled.MoreVert, stringResource(R.string.u_inv_more))
                                }
                                DropdownMenu(more, { more = false }) {
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.inv_block), color = c.danger) },
                                        onClick = { more = false; pending = Triple(r, true, System.currentTimeMillis()) },
                                    )
                                    DropdownMenuItem(
                                        text = { Text(stringResource(R.string.inv_report)) },
                                        onClick = { more = false; core.inviteReport(r.offerId)?.let { shareReport(ctx, it) } },
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** Short pseudonym for a member key (same 8-hex style as MemberIdentity). */
fun pseudonymLabel(keyHex: String): String = "#" + keyHex.take(8)

/** Members of a community who can't be invited (switch off): no `invite_pk` published. */
fun notAccepting(members: List<OpenMemberUi>, cands: List<InviteCandidateUi>): List<OpenMemberUi> {
    val ok = cands.map { it.key }.toSet()
    return members.filter { !it.isMe && it.key !in ok }
}

/** `group-invite` + `invite-sent-status` (§5.152): founder picks members of their communities. */
@Composable
fun GroupInviteFromCommunities(core: OmcpCore, groupId: String, full: Boolean) {
    val c = LocalOmcpColors.current
    var comms by remember { mutableStateOf(listOf<OpenCommunityUi>()) }
    var cands by remember { mutableStateOf(mapOf<String, List<InviteCandidateUi>>()) }
    var members by remember { mutableStateOf(mapOf<String, List<OpenMemberUi>>()) }
    var sent by remember { mutableStateOf(listOf<InviteSentUi>()) }
    LaunchedEffect(groupId) {
        while (true) {
            comms = core.openCommunities()
            cands = comms.associate { it.id to core.inviteCandidates(it.id) }
            members = comms.associate { it.id to core.openMembers(it.id) }
            sent = core.invitesSent().filter { it.groupId == groupId }
            kotlinx.coroutines.delay(2_000)
        }
    }
    Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        // MR39: shown on every send screen.
        Callout(stringResource(R.string.inv_mesh_only), CalloutKind.Info)
        Text(stringResource(R.string.u_inv_limits), color = c.ink3, style = MaterialTheme.typography.bodySmall)
        if (sent.isNotEmpty()) Text(stringResource(R.string.u_inv_sent_h), style = MaterialTheme.typography.titleSmall)
        val now = System.currentTimeMillis()
        sent.forEach { s ->
            val v = inviteStatusView(s.status)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Icon(v.icon, null, Modifier.size(16.dp))
                Text(stringResource(v.label), style = MaterialTheme.typography.labelLarge)
                if (s.status != "Accepted" && s.status != "Declined" && s.status != "Expired" && s.expiresMs > 0)
                    Text(stringResource(R.string.u_inv_exp, daysLeft(s.expiresMs, now)), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            }
        }
        if (sent.any { inviteStatusView(it.status).label == R.string.inv_status_pending })
            Text(stringResource(R.string.u_inv_pending_d), color = c.ink3, style = MaterialTheme.typography.bodySmall)
        comms.forEach { k ->
            Text(stringResource(R.string.inv_members_h, k.name), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
            val list = cands[k.id].orEmpty()
            val off = notAccepting(members[k.id].orEmpty(), list)
            if (list.isEmpty() && off.isEmpty()) Text(stringResource(R.string.inv_none), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            list.forEach { m ->
                var done by remember(k.id, m.key) { mutableStateOf(false) }
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(m.contactName ?: Pseudonym.of(m.key).name, Modifier.weight(1f))
                    TextButton(enabled = !done, modifier = Modifier.heightIn(min = 48.dp).semantics { if (full) disabled() },
                        onClick = {
                            if (!full) core.inviteSend(groupId, k.id, m.key, "")?.let { done = true; sent = core.invitesSent().filter { it.groupId == groupId } }
                        }) { Text(stringResource(if (done) R.string.community_invited else R.string.community_invite)) }
                }
            }
            off.forEach { m ->
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(m.contactName ?: Pseudonym.of(m.key).name)
                        Text(stringResource(R.string.u_inv_noacc), color = c.ink3, style = MaterialTheme.typography.bodySmall)
                    }
                    TextButton(onClick = {}, modifier = Modifier.heightIn(min = 48.dp).semantics { disabled() }, enabled = false) {
                        Text(stringResource(R.string.community_invite))
                    }
                }
            }
        }
    }
}

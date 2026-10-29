package chat.omcp.app.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.CommunityInviteUi
import chat.omcp.app.core.CommunityMemberUi
import chat.omcp.app.core.CommunityUi
import chat.omcp.app.core.Contact
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.LiveViewModel

/** Communities tab: Live entry on top (§5.107), invites, your communities, create one. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CommunitiesHomeScreen(core: OmcpCore, onOpenLive: () -> Unit, onOpen: (String) -> Unit) {
    val c = LocalOmcpColors.current
    var list by remember { mutableStateOf(listOf<CommunityUi>()) }
    var invites by remember { mutableStateOf(listOf<CommunityInviteUi>()) }
    var name by remember { mutableStateOf("") }
    val contacts = remember { core.contacts() }
    LaunchedEffect(Unit) {
        while (true) {
            list = core.communities(); invites = core.communityInvites()
            kotlinx.coroutines.delay(1_000)
        }
    }
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_communities)) }) }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { TextButton(onClick = onOpenLive, modifier = Modifier.padding(horizontal = 8.dp)) { Text(stringResource(R.string.live_entry)) } }
            items(invites, key = { "i" + it.communityId }) { inv ->
                val from = contacts.firstOrNull { it.id == inv.fromContactId }?.displayName ?: ("omcp1:" + inv.fromContactId.take(4) + "…")
                Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                    Column(Modifier.padding(16.dp)) {
                        Text(stringResource(R.string.community_invite_t, inv.name), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.community_invite_from, from), color = c.ink2)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                            Button(onClick = { core.communityAccept(inv.communityId); invites = core.communityInvites() }) { Text(stringResource(R.string.community_accept)) }
                            OutlinedButton(onClick = { core.communityDecline(inv.communityId); invites = core.communityInvites() }) { Text(stringResource(R.string.community_decline)) }
                        }
                    }
                }
            }
            if (list.isEmpty() && invites.isEmpty()) {
                item { Text(stringResource(R.string.community_none), color = c.ink3, modifier = Modifier.padding(horizontal = 16.dp)) }
            }
            items(list, key = { it.id }) { k ->
                Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp).clickable { onOpen(k.id) }) {
                    Column(Modifier.padding(16.dp)) {
                        Text(k.name, style = MaterialTheme.typography.titleMedium)
                        Text(communitySubtitle(k), color = if (k.removedReason != null) c.danger else c.ink2)
                    }
                }
            }
            item {
                Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                    Column(Modifier.padding(16.dp)) {
                        Text(stringResource(R.string.community_create_t), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.community_create_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                        OutlinedTextField(name, { name = it.take(60) }, Modifier.fillMaxWidth().padding(top = 8.dp),
                            label = { Text(stringResource(R.string.community_name)) }, singleLine = true)
                        Button(onClick = { core.communityCreate(name)?.let { name = ""; list = core.communities(); onOpen(it.id) } },
                            enabled = name.isNotBlank(), modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.community_create)) }
                    }
                }
            }
        }
    }
}

@Composable
private fun communitySubtitle(k: CommunityUi): String = when {
    k.removedReason != null -> stringResource(R.string.community_removed)
    k.liveCount > 0 -> stringResource(R.string.community_sub_live, k.members, k.liveCount)
    else -> stringResource(R.string.community_sub, k.members)
}

/** One community: who's live, go live, members (invite; remove from your live). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CommunityDetailScreen(
    communityId: String,
    core: OmcpCore,
    vm: LiveViewModel,
    contacts: List<Contact>,
    rtc: chat.omcp.app.live.WebRtcPeers?,
    hasInternet: () -> Boolean,
    onBack: () -> Unit,
) {
    val c = LocalOmcpColors.current
    val s by vm.state.collectAsState()
    var info by remember { mutableStateOf<CommunityUi?>(null) }
    var members by remember { mutableStateOf(listOf<CommunityMemberUi>()) }
    var title by remember { mutableStateOf("") }
    LaunchedEffect(communityId) {
        while (true) {
            info = core.communities().firstOrNull { it.id == communityId }
            members = core.communityMembers(communityId)
            vm.refresh()
            kotlinx.coroutines.delay(1_000)
        }
    }
    val online = hasInternet()
    val k = info
    val removed = k?.removedReason != null
    val lives = core.liveStreams().filter { it.communityId == communityId && !it.mine }
    val hostingHere = s.hosting?.communityId == communityId
    Scaffold(topBar = {
        TopAppBar(title = { Text(k?.name ?: "") }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        })
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            if (removed) {
                item { Callout(stringResource(R.string.community_removed_body), CalloutKind.Warn, Modifier.padding(horizontal = 16.dp)) }
            }
            if (s.notice == "taken_down") {
                item {
                    Callout(stringResource(R.string.live_taken_down), CalloutKind.Warn,
                        Modifier.padding(horizontal = 16.dp).semantics { liveRegion = LiveRegionMode.Polite })
                }
            }
            s.hosting?.takeIf { hostingHere }?.let { h ->
                item {
                    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                        Column(Modifier.padding(16.dp)) {
                            Text(stringResource(R.string.live_badge), color = c.danger, style = MaterialTheme.typography.labelMedium)
                            Text(h.title, style = MaterialTheme.typography.titleMedium)
                            Text(stringResource(R.string.community_live_host_note), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                            Button(onClick = vm::stopLive, modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.live_stop)) }
                        }
                    }
                }
            }
            item { Text(stringResource(R.string.live_happening), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(horizontal = 16.dp)) }
            if (lives.isEmpty()) item { Text(stringResource(R.string.community_no_live), color = c.ink3, modifier = Modifier.padding(horizontal = 16.dp)) }
            items(lives, key = { it.streamIdHex }) { st ->
                val host = members.firstOrNull { it.peerId == st.hostContactId }?.name ?: ("omcp1:" + st.hostContactId.take(4) + "…")
                Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                    Column(Modifier.padding(16.dp)) {
                        Text(stringResource(R.string.live_badge), color = c.danger, style = MaterialTheme.typography.labelMedium)
                        Text(st.title, style = MaterialTheme.typography.titleMedium)
                        Text(host, color = c.ink2)
                        if (st.listening) {
                            Text(stringResource(R.string.live_listening, LiveViewModel.lossPercent(s.stats)), color = c.ink2)
                            OutlinedButton(onClick = vm::leave, modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.live_leave)) }
                        } else {
                            Button(onClick = { vm.listen(st.streamIdHex, online) }, enabled = s.hosting == null && !removed,
                                modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.live_join)) }
                        }
                    }
                }
            }
            if (s.hosting == null && !removed) {
                item {
                    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                        Column(Modifier.padding(16.dp)) {
                            Text(stringResource(R.string.live_start_title), style = MaterialTheme.typography.titleMedium)
                            Text(stringResource(R.string.community_live_body), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                            OutlinedTextField(title, { title = it.take(120) }, Modifier.fillMaxWidth().padding(top = 8.dp),
                                label = { Text(stringResource(R.string.live_title_hint)) }, singleLine = true)
                            Button(onClick = { vm.goLiveCommunity(communityId, title, online) }, modifier = Modifier.padding(top = 8.dp)) {
                                Text(stringResource(R.string.live_go))
                            }
                        }
                    }
                }
            }
            item { Text(stringResource(R.string.community_members, members.size), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(horizontal = 16.dp)) }
            items(members, key = { it.peerId }) { m ->
                Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(if (m.isMe) stringResource(R.string.community_you) else m.name, Modifier.weight(1f))
                    if (hostingHere && !m.isMe) {
                        TextButton(onClick = { vm.removeViewer(m.peerId) }) { Text(stringResource(R.string.live_remove_viewer)) }
                    }
                }
            }
            if (k?.founderIsMe == true && !removed) {
                val memberIds = members.map { it.peerId }.toSet()
                val invitable = contacts.filter { it.id !in memberIds }
                if (invitable.isNotEmpty()) {
                    item { Text(stringResource(R.string.community_invite_h), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(horizontal = 16.dp)) }
                    items(invitable, key = { "c" + it.id }) { ct ->
                        var sent by remember { mutableStateOf(false) }
                        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(ct.displayName, Modifier.weight(1f))
                            TextButton(onClick = { sent = core.communityInvite(communityId, ct.id) }, enabled = !sent) {
                                Text(stringResource(if (sent) R.string.community_invited else R.string.community_invite))
                            }
                        }
                    }
                }
            }
        }
    }
}

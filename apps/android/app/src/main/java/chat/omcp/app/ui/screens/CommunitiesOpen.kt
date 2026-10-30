package chat.omcp.app.ui.screens

import android.content.Context
import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.People
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import chat.omcp.app.R
import chat.omcp.app.core.CommunityLimits
import chat.omcp.app.core.GroupMessageUi
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.OpenCommunityUi
import chat.omcp.app.core.OpenMemberUi
import chat.omcp.app.core.OpenPostUi
import chat.omcp.app.core.OpenTopicUi
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.EmptyState
import chat.omcp.app.ui.theme.LocalOmcpColors

/** `#` glyph in a neutral rounded-square tile (§5.128). */
@Composable
fun HashTile(modifier: Modifier = Modifier) {
    val c = LocalOmcpColors.current
    Box(modifier.size(40.dp).background(c.surface2, RoundedCornerShape(10.dp)), contentAlignment = Alignment.Center) {
        Text("#", fontWeight = FontWeight.Bold, color = c.ink2)
    }
}

/** Round avatar with a lock corner badge (private group, §5.128). */
@Composable
fun GroupAvatar(name: String, modifier: Modifier = Modifier) {
    val c = LocalOmcpColors.current
    Box(modifier.size(44.dp)) {
        Box(Modifier.size(40.dp).background(c.brandSoft, RoundedCornerShape(20.dp)), contentAlignment = Alignment.Center) {
            Text(name.take(1).uppercase(), fontWeight = FontWeight.Bold)
        }
        Icon(Icons.Filled.Lock, null, Modifier.size(16.dp).align(Alignment.BottomEnd).background(c.surface2, RoundedCornerShape(8.dp)).padding(2.dp))
    }
}

/** `create-chooser`: one sheet for both tabs. Creates and returns ("group"|"community", id). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CreateChooserSheet(core: OmcpCore, onDismiss: () -> Unit, onCreated: (kind: String, id: String) -> Unit) {
    val c = LocalOmcpColors.current
    var kind by remember { mutableStateOf<String?>(null) }
    var name by remember { mutableStateOf("") }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.u_create_t), style = MaterialTheme.typography.titleLarge)
            if (kind == null) {
                ChooserRow(stringResource(R.string.u_create_grp), stringResource(R.string.u_create_grp_d), { GroupAvatar("") }) { kind = "group" }
                ChooserRow(stringResource(R.string.u_create_comm), stringResource(R.string.u_create_comm_d), { HashTile() }) { kind = "community" }
            } else {
                Text(stringResource(if (kind == "group") R.string.u_create_grp else R.string.u_create_comm), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(if (kind == "group") R.string.u_create_grp_d else R.string.u_create_comm_d), color = c.ink2)
                OutlinedTextField(name, { name = it.take(60) }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.u_create_name)) }, singleLine = true)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { kind = null }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_cancel)) }
                    Button(enabled = name.isNotBlank(), modifier = Modifier.heightIn(min = 48.dp), onClick = {
                        val id = if (kind == "group") core.communityCreate(name.trim())?.id else core.openCommunityCreate(name.trim())?.id
                        if (id != null) onCreated(kind!!, id)
                    }) { Text(stringResource(R.string.u_create_go)) }
                }
            }
            Box(Modifier.heightIn(min = 24.dp))
        }
    }
}

@Composable
private fun ChooserRow(title: String, body: String, glyph: @Composable () -> Unit, onClick: () -> Unit) {
    val c = LocalOmcpColors.current
    Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable(role = Role.Button, onClick = onClick).padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        glyph()
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(body, color = c.ink2, style = MaterialTheme.typography.bodySmall)
        }
    }
}

/** `comm-tab`: open communities only (private groups live in Chats). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OpenCommunitiesTab(core: OmcpCore, onOpen: (String) -> Unit, onOpenGroup: (String) -> Unit, onOpenLive: () -> Unit) {
    val c = LocalOmcpColors.current
    var list by remember { mutableStateOf(listOf<OpenCommunityUi>()) }
    var code by remember { mutableStateOf("") }
    var failed by remember { mutableStateOf(false) }
    var chooser by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) { while (true) { list = core.openCommunities(); kotlinx.coroutines.delay(1_500) } }
    if (chooser) CreateChooserSheet(core, { chooser = false }) { k, id -> chooser = false; if (k == "group") onOpenGroup(id) else onOpen(id) }
    Scaffold(topBar = {
        TopAppBar(title = { Text(stringResource(R.string.tab_communities)) }, actions = {
            IconButton(onClick = { chooser = true }) { Icon(Icons.Filled.Add, stringResource(R.string.u_create_t)) }
        })
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            item { TextButton(onClick = onOpenLive, modifier = Modifier.padding(horizontal = 8.dp).heightIn(min = 48.dp)) { Text(stringResource(R.string.live_entry)) } }
            if (list.isEmpty()) item { EmptyState(stringResource(R.string.u_e_comm), stringResource(R.string.u_e_comm_d)) }
            else item { Text(stringResource(R.string.u_joined), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(horizontal = 16.dp)) }
            items(list, key = { it.id }) { k ->
                Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable { onOpen(k.id) }.padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    HashTile()
                    Column(Modifier.weight(1f)) {
                        Text(k.name, style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.u_comm_lbl, k.members), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
            item {
                Card(Modifier.fillMaxWidth().padding(16.dp)) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(stringResource(R.string.u_join_code), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.u_join_code_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                        OutlinedTextField(code, { code = it.trim(); failed = false }, Modifier.fillMaxWidth(), singleLine = true,
                            label = { Text(stringResource(R.string.u_join_code)) }, isError = failed,
                            supportingText = { if (failed) Text(stringResource(R.string.u_join_fail)) })
                        Button(enabled = code.isNotBlank(), modifier = Modifier.heightIn(min = 48.dp), onClick = {
                            val j = core.openCommunityJoin(code)
                            if (j == null) failed = true else { code = ""; onOpen(j.id) }
                        }) { Text(stringResource(R.string.u_join_btn)) }
                    }
                }
            }
        }
    }
}

/** `comm-detail`: open header with `c_info`, topics, members, share code. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OpenCommunityScreen(core: OmcpCore, id: String, onTopic: (String) -> Unit, onMembers: () -> Unit, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    val ctx = LocalContext.current
    var info by remember { mutableStateOf<OpenCommunityUi?>(null) }
    var topics by remember { mutableStateOf(listOf<OpenTopicUi>()) }
    var newTopic by remember { mutableStateOf("") }
    LaunchedEffect(id) { while (true) { info = core.openCommunities().firstOrNull { it.id == id }; topics = core.openTopics(id); kotlinx.coroutines.delay(1_500) } }
    val k = info
    Scaffold(topBar = {
        TopAppBar(title = { Text(k?.name ?: "") }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        }, actions = {
            IconButton(onClick = onMembers) { Icon(Icons.Filled.People, stringResource(R.string.u_members_t)) }
        })
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            item {
                Column(Modifier.padding(horizontal = 16.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        HashTile()
                        Text(stringResource(R.string.u_comm_lbl, k?.members ?: 0), color = c.ink2)
                    }
                    Text(stringResource(R.string.u_c_info), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 6.dp))
                    if (k != null) TextButton(onClick = { shareText(ctx, k.code) }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_share_code)) }
                    HorizontalDivider()
                    Text(stringResource(R.string.u_tp), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
                }
            }
            if (topics.isEmpty()) item { Text(stringResource(R.string.u_e_topic), color = c.ink3, modifier = Modifier.padding(horizontal = 16.dp)) }
            items(topics, key = { it.id }) { t ->
                Text(t.title, Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable { onTopic(t.id) }.padding(horizontal = 16.dp, vertical = 12.dp),
                    style = MaterialTheme.typography.titleMedium)
            }
            if (k?.isModerator == true) item {
                Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(newTopic, { newTopic = it.take(80) }, Modifier.weight(1f), singleLine = true, label = { Text(stringResource(R.string.u_new_topic)) })
                    TextButton(enabled = newTopic.isNotBlank(), modifier = Modifier.heightIn(min = 48.dp), onClick = {
                        core.openNewTopic(id, newTopic.trim()); newTopic = ""; topics = core.openTopics(id)
                    }) { Text(stringResource(R.string.u_create_go)) }
                }
            }
            item { Text(stringResource(R.string.u_comm_live_members) + " " + stringResource(R.string.u_comm_live_d), color = c.ink3, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp)) }
        }
    }
}

/** Topic: threads (top-level posts with replies), composer, mod tools, report, stubs. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OpenTopicScreen(core: OmcpCore, id: String, topicId: String, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("communities", Context.MODE_PRIVATE) }
    var posts by remember { mutableStateOf(listOf<OpenPostUi>()) }
    var draft by remember { mutableStateOf("") }
    var replyTo by remember { mutableStateOf<String?>(null) }
    var firstNotice by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<String?>(null) }
    var reporting by remember { mutableStateOf<String?>(null) }
    val info = remember(id) { core.openCommunities().firstOrNull { it.id == id } }
    val title = remember(topicId) { core.openTopics(id).firstOrNull { it.id == topicId }?.title.orEmpty() }
    LaunchedEffect(topicId) { while (true) { posts = core.openPosts(id, topicId); kotlinx.coroutines.delay(1_500) } }
    val bytes = draft.toByteArray().size
    fun send() {
        if (!prefs.getBoolean("first_post_seen", false)) { firstNotice = true; return }
        if (core.openPost(id, topicId, replyTo, draft.trim()) != null) { draft = ""; replyTo = null; posts = core.openPosts(id, topicId) }
    }
    if (firstNotice) AlertDialog(onDismissRequest = { firstNotice = false },
        title = { Text(stringResource(R.string.u_first_t)) }, text = { Text(stringResource(R.string.u_first_d)) },
        confirmButton = { TextButton(onClick = { prefs.edit().putBoolean("first_post_seen", true).apply(); firstNotice = false; send() }) { Text(stringResource(R.string.u_got_it)) } })
    removing?.let { pid ->
        RemoveReasonDialog(onDismiss = { removing = null }) { reason ->
            // Remove thread = the opening post and its replies.
            (listOf(pid) + posts.filter { it.parentId == pid && it.removedReason == null }.map { it.id }).forEach { core.openRemovePost(id, it, reason) }
            removing = null; posts = core.openPosts(id, topicId)
        }
    }
    reporting?.let { pid ->
        ReportSheet(stringResource(R.string.u_rep_c), onDismiss = { reporting = null }) { r, note -> core.openReportPost(id, pid, r, note) }
    }
    Scaffold(topBar = {
        TopAppBar(title = { Text(title) }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        })
    }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Text(stringResource(R.string.u_c_info), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp))
            val top = posts.filter { it.parentId == null }
            LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                if (top.isEmpty()) item { EmptyState(stringResource(R.string.u_e_topic), stringResource(R.string.u_e_topic_d)) }
                top.forEach { p ->
                    item(key = p.id) { PostRow(p, 0, info?.isModerator == true, { replyTo = p.id }, { removing = p.id }, { reporting = p.id }) }
                    items(posts.filter { it.parentId == p.id }, key = { it.id }) { r ->
                        PostRow(r, 1, info?.isModerator == true, { replyTo = p.id }, { removing = r.id }, { reporting = r.id })
                    }
                }
            }
            if (replyTo != null) TextButton(onClick = { replyTo = null }, Modifier.padding(horizontal = 8.dp)) { Text(stringResource(R.string.u_reply) + " ✕") }
            Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(draft, { draft = it }, Modifier.weight(1f), label = { Text(stringResource(if (replyTo == null) R.string.u_new_thread else R.string.u_reply)) },
                    isError = bytes > CommunityLimits.POST_BYTES,
                    supportingText = { Text(stringResource(R.string.u_bytes_left, CommunityLimits.POST_BYTES - bytes)) })
                Button(enabled = draft.isNotBlank() && bytes <= CommunityLimits.POST_BYTES, onClick = ::send, modifier = Modifier.padding(start = 8.dp).heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.u_post_btn))
                }
            }
        }
    }
}

@Composable
private fun PostRow(p: OpenPostUi, depth: Int, canModerate: Boolean, onReply: () -> Unit, onRemove: () -> Unit, onReport: () -> Unit) {
    val c = LocalOmcpColors.current
    Column(Modifier.fillMaxWidth().padding(start = (16 + depth * 24).dp, end = 8.dp, top = 6.dp, bottom = 6.dp)) {
        if (p.removedReason != null) {
            // `comm-removed-stub`: same for everyone, no "view original".
            Text(stringResource(R.string.u_stub, p.removedReason), color = c.ink2)
            Text(stringResource(R.string.u_stub_n), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            return@Column
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            MemberIdentity(p.authorKey, p.authorContactName, null, Modifier.weight(1f))
            var sheet by remember { mutableStateOf(false) }
            IconButton(onClick = { sheet = true }) { Icon(Icons.Filled.MoreVert, stringResource(R.string.d_more_p)) }
            if (sheet) ModToolsSheet(canModerate, isReply = depth > 0, onDismiss = { sheet = false },
                onReply = { sheet = false; onReply() }, onRemove = { sheet = false; onRemove() }, onReport = { sheet = false; onReport() })
        }
        Text(p.body)
    }
}

/**
 * `comm-mod-tools` as its own modal bottom sheet (§5.141): 48 dp rows, focus
 * kept in the sheet. Remove member isn't in the protocol yet (not shown).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ModToolsSheet(canModerate: Boolean, isReply: Boolean, onDismiss: () -> Unit, onReply: () -> Unit, onRemove: () -> Unit, onReport: () -> Unit) {
    val c = LocalOmcpColors.current
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(horizontal = 16.dp)) {
            Text(stringResource(if (canModerate) R.string.d_mt_t else R.string.d_more_p), style = MaterialTheme.typography.titleLarge)
            @Composable fun row(text: String, danger: Boolean = false, go: () -> Unit) =
                Text(text, color = if (danger) c.danger else MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Button, onClick = go).padding(vertical = 14.dp))
            row(stringResource(R.string.u_reply), go = onReply)
            if (canModerate) row(stringResource(if (isReply) R.string.d_rm_post else R.string.d_rm_thread), danger = true, go = onRemove)
            row(stringResource(R.string.d_report), go = onReport)
            if (canModerate) Text(stringResource(R.string.d_mt_note), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 8.dp))
            Row(Modifier.padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.Lock, null, Modifier.size(16.dp)); Text(stringResource(R.string.u_anon), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(start = 6.dp))
            }
            Box(Modifier.heightIn(min = 24.dp))
        }
    }
}

/** §5.138: pseudonym or saved contact name, always with the short key. */
@Composable
fun MemberIdentity(keyHex: String, contactName: String?, role: String?, modifier: Modifier = Modifier) {
    val c = LocalOmcpColors.current
    val id = remember(keyHex) { chat.omcp.app.ui.components.Pseudonym.of(keyHex) }
    val name = contactName ?: id.name
    val a11y = listOfNotNull(name, role, "key " + id.shortKey).joinToString(", ")
    Row(modifier.semantics(mergeDescendants = true) { contentDescription = a11y }, verticalAlignment = Alignment.CenterVertically) {
        if (contactName != null) Box(Modifier.size(40.dp).background(c.brandSoft, RoundedCornerShape(20.dp)), contentAlignment = Alignment.Center) {
            Text(contactName.take(1).uppercase(), fontWeight = FontWeight.Bold)
        } else chat.omcp.app.ui.components.Identicon(id)
        Column(Modifier.padding(start = 10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                // Pseudonyms and keys are LTR English names in every locale.
                Text(name, style = MaterialTheme.typography.labelLarge.copy(textDirection = androidx.compose.ui.text.style.TextDirection.Ltr))
                if (contactName != null) Text(stringResource(R.string.d_contact_b), color = c.ink2, style = MaterialTheme.typography.labelSmall,
                    modifier = Modifier.padding(start = 6.dp).background(c.surface2, RoundedCornerShape(6.dp)).padding(horizontal = 6.dp, vertical = 2.dp))
            }
            Text(listOfNotNull(role, id.shortKey).joinToString(" · "), color = c.ink2,
                style = MaterialTheme.typography.bodySmall.copy(textDirection = androidx.compose.ui.text.style.TextDirection.Ltr))
        }
    }
}

/** `comm-remove-reason`: Remove stays disabled (still focusable) until a reason is chosen. */
@Composable
fun RemoveReasonDialog(onDismiss: () -> Unit, onRemove: (String) -> Unit) {
    val reasons = listOf(R.string.u_rr1, R.string.u_rr2, R.string.u_rr3, R.string.u_rr4, R.string.u_rr5).map { stringResource(it) }
    var pick by remember { mutableStateOf(-1) }
    var other by remember { mutableStateOf("") }
    val ok = pick in 0..3 || (pick == 4 && other.isNotBlank())
    val hint = stringResource(R.string.u_choose)
    AlertDialog(onDismissRequest = onDismiss, title = { Text(stringResource(R.string.u_rr_t)) }, text = {
        Column(Modifier.selectableGroup()) {
            Text(stringResource(R.string.u_rr_req), style = MaterialTheme.typography.bodySmall)
            reasons.forEachIndexed { i, r ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(pick == i, role = Role.RadioButton) { pick = i }, verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(pick == i, null); Text(r, Modifier.padding(start = 8.dp))
                }
            }
            if (pick == 4) OutlinedTextField(other, { other = it.take(200) }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.u_other_ph)) })
            if (!ok) Text(hint, style = MaterialTheme.typography.bodySmall)
        }
    }, confirmButton = {
        TextButton(onClick = { if (ok) onRemove(if (pick == 4) other.trim() else reasons[pick]) },
            modifier = Modifier.heightIn(min = 48.dp).semantics { if (!ok) { disabled(); stateDescription = hint } }) { Text(stringResource(R.string.u_remove_post)) }
    }, dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.u_cancel)) } })
}

/** Report reasons shown in the sheet → protocol reason codes (§23.9). */
val REPORT_REASONS = listOf(
    R.string.u_rep_r1 to 6, R.string.u_rep_r2 to 7, R.string.u_rep_r3 to 4, R.string.u_rep_r4 to 10,
    R.string.u_rep_r5 to 2, R.string.u_rep_r6 to 5, R.string.u_rep_r7 to 9,
)

/**
 * `comm-report`: in-app anonymous report. [make] builds the file in Rust
 * (no reporter identity); the user saves or shares it. Intake address is a
 * placeholder until v1.0.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ReportSheet(title: String, onDismiss: () -> Unit, make: (reason: Int, note: String?) -> ByteArray?) {
    val c = LocalOmcpColors.current
    val ctx = LocalContext.current
    var pick by remember { mutableStateOf(-1) }
    var note by remember { mutableStateOf("") }
    var file by remember { mutableStateOf<ByteArray?>(null) }
    var failed by remember { mutableStateOf(false) }
    var saved by remember { mutableStateOf(false) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val b = file
        if (uri != null && b != null) runCatching { ctx.contentResolver.openOutputStream(uri)?.use { it.write(b) } }.onSuccess { saved = true }
    }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(16.dp).selectableGroup(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(title, style = MaterialTheme.typography.titleLarge)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Filled.Lock, null, Modifier.size(16.dp)); Text(stringResource(R.string.u_anon), color = c.ink2, modifier = Modifier.padding(start = 6.dp))
            }
            REPORT_REASONS.forEachIndexed { i, (res, _) ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(pick == i, role = Role.RadioButton) { pick = i; file = null }, verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(pick == i, null); Text(stringResource(res), Modifier.padding(start = 8.dp))
                }
            }
            OutlinedTextField(note, { note = it.take(500); file = null }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.u_note_opt)) })
            if (failed) Callout(stringResource(R.string.u_rep_fail), CalloutKind.Danger)
            if (saved) Callout(stringResource(R.string.u_rep_saved), CalloutKind.Ok)
            Text(stringResource(R.string.u_rep_intake, stringResource(R.string.u_rep_intake_addr)), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            fun build(): ByteArray? = file ?: make(REPORT_REASONS[pick].second, note.trim().ifEmpty { null }).also { file = it; failed = it == null }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(enabled = pick >= 0, modifier = Modifier.heightIn(min = 48.dp), onClick = { if (build() != null) save.launch("report.omcp-evidence") }) { Text(stringResource(R.string.u_rep_save)) }
                OutlinedButton(enabled = pick >= 0, modifier = Modifier.heightIn(min = 48.dp), onClick = { build()?.let { shareReport(ctx, it) } }) { Text(stringResource(R.string.u_rep_share)) }
            }
            Box(Modifier.heightIn(min = 24.dp))
        }
    }
}

private fun shareReport(ctx: Context, bytes: ByteArray) {
    val dir = java.io.File(ctx.cacheDir, "reports").apply { mkdirs() }
    val f = java.io.File(dir, "report.omcp-evidence").apply { writeBytes(bytes) }
    val uri = FileProvider.getUriForFile(ctx, ctx.packageName + ".files", f)
    ctx.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("application/octet-stream")
        .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION), null))
}

private fun shareText(ctx: Context, text: String) {
    ctx.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, text), null))
}

/** `comm-members`: roles; only the founder sees "Make moderator". */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OpenMembersScreen(core: OmcpCore, id: String, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    var members by remember { mutableStateOf(listOf<OpenMemberUi>()) }
    val isFounder = remember(id) { core.openCommunities().firstOrNull { it.id == id }?.isFounder == true }
    LaunchedEffect(id) { while (true) { members = core.openMembers(id); kotlinx.coroutines.delay(2_000) } }
    Scaffold(topBar = {
        TopAppBar(title = { Text(stringResource(R.string.u_members_t)) }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        })
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding)) {
            item { Text(stringResource(R.string.d_id_note), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp)) }
            item { Text(stringResource(R.string.u_mod_rule), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp)) }
            items(members, key = { it.key }) { m ->
                Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    val role = stringResource(when (m.role) { "founder" -> R.string.u_role_f; "moderator" -> R.string.u_role_m; else -> R.string.u_role_n })
                    MemberIdentity(m.key, if (m.isMe) stringResource(R.string.u_member_you) else m.contactName, role, Modifier.weight(1f))
                    if (isFounder && m.role == "member") OutlinedButton(onClick = { core.openMakeModerator(id, m.key); members = core.openMembers(id) },
                        Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_make_mod)) }
                }
            }
        }
    }
}

/** Private-group chat section: E2EE header, "Checking…" hold, messages, report. */
@Composable
fun GroupChatSection(core: OmcpCore, groupId: String, members: Int, checking: Boolean, removed: Boolean, modifier: Modifier = Modifier) {
    val c = LocalOmcpColors.current
    var msgs by remember { mutableStateOf(listOf<GroupMessageUi>()) }
    var draft by remember { mutableStateOf("") }
    var reporting by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(groupId) { while (true) { msgs = core.groupMessages(groupId); kotlinx.coroutines.delay(1_000) } }
    reporting?.let { mid -> ReportSheet(stringResource(R.string.u_rep_m), { reporting = null }) { r, n -> core.groupReport(groupId, listOf(mid), r, n) } }
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(Modifier.fillMaxWidth().background(c.brandSoft).padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.Lock, null, Modifier.size(16.dp))
            Column(Modifier.padding(start = 8.dp)) {
                Text(stringResource(R.string.u_grp_e2ee), style = MaterialTheme.typography.labelLarge)
                Text(stringResource(R.string.u_grp_lbl, members), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            }
        }
        if (checking) { RestoreChecking(core.meshStats().neighbours > 0); return@Column }
        msgs.takeLast(50).forEach { m ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(if (m.mine) stringResource(R.string.u_member_you) else m.senderName, color = c.ink2, style = MaterialTheme.typography.labelMedium)
                    Text(m.body)
                }
                // No report option for our own messages (and never on 1:1 chats).
                if (m.reportable) {
                    var menu by remember { mutableStateOf(false) }
                    IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, stringResource(R.string.u_rep_m)) }
                    DropdownMenu(menu, { menu = false }) {
                        DropdownMenuItem(text = { Text(stringResource(R.string.u_report_btn)) }, onClick = { menu = false; reporting = m.id })
                    }
                }
            }
        }
        if (!removed) Row(verticalAlignment = Alignment.CenterVertically) {
            val bytes = draft.toByteArray().size
            OutlinedTextField(draft, { draft = it }, Modifier.weight(1f), label = { Text(stringResource(R.string.u_grp_msg_hint)) }, isError = bytes > CommunityLimits.POST_BYTES)
            Button(enabled = draft.isNotBlank() && bytes <= CommunityLimits.POST_BYTES, modifier = Modifier.padding(start = 8.dp).heightIn(min = 48.dp), onClick = {
                if (core.groupSend(groupId, draft.trim()) != null) { draft = ""; msgs = core.groupMessages(groupId) }
            }) { Text(stringResource(R.string.u_send)) }
        }
    }
}


/** `restore-checking` (§5.139): status callout + 3 steps; messages stay hidden. */
@Composable
fun RestoreChecking(foundPhone: Boolean) {
    val c = LocalOmcpColors.current
    val step = if (foundPhone) 2 else 1
    val prog = stringResource(R.string.d_ck_prog, step.toString())
    Column(Modifier.fillMaxWidth().background(c.surface2, RoundedCornerShape(12.dp)).padding(12.dp)
        .semantics { liveRegion = androidx.compose.ui.semantics.LiveRegionMode.Polite }, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(stringResource(R.string.u_checking), style = MaterialTheme.typography.titleMedium)
        Text(stringResource(R.string.u_checking_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
        listOf(R.string.d_ck1, R.string.d_ck2, R.string.d_ck3).forEachIndexed { i, res ->
            val state = when { i + 1 < step -> R.string.d_ck_done; i + 1 == step -> R.string.d_ck_now; else -> R.string.d_ck_wait }
            Row(Modifier.fillMaxWidth().heightIn(min = 32.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(if (i + 1 < step) "✓" else if (i + 1 == step) "•" else "○", Modifier.size(24.dp))
                Text(stringResource(res), Modifier.weight(1f))
                Text(stringResource(state), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            }
        }
        androidx.compose.material3.LinearProgressIndicator(progress = { step / 3f },
            modifier = Modifier.fillMaxWidth().semantics { stateDescription = prog })
        Text(stringResource(R.string.d_ck_hint), color = c.ink3, style = MaterialTheme.typography.bodySmall)
    }
}

/** `live-blocked-24h` (§5.140): countdown updated every minute. */
@Composable
fun LiveBlockedDialog(untilMs: Long, onDismiss: () -> Unit, onLearn: () -> Unit) {
    val c = LocalOmcpColors.current
    var now by remember { mutableStateOf(System.currentTimeMillis()) }
    LaunchedEffect(Unit) { while (true) { kotlinx.coroutines.delay(60_000); now = System.currentTimeMillis() } }
    val left = ((untilMs - now).coerceAtLeast(0) / 60_000)
    AlertDialog(onDismissRequest = onDismiss,
        icon = { Icon(Icons.Filled.Lock, null, tint = c.danger) },
        title = { Text(stringResource(R.string.d_lb_t)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.u_live_blocked))
                Text(stringResource(R.string.d_lb_left, (left / 60).toString(), (left % 60).toString()),
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.fillMaxWidth().background(c.surface2, RoundedCornerShape(8.dp)).padding(12.dp))
                Text(stringResource(R.string.d_no_appeal), color = c.ink2)
                Text(stringResource(R.string.d_lb_other), color = c.ink2)
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("OK") } },
        dismissButton = { TextButton(onClick = onLearn) { Text(stringResource(R.string.mod_learn)) } })
}

/** `group-remove` (§5.141): founder-only confirm sheet. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GroupRemoveSheet(name: String, others: Int, onDismiss: () -> Unit, onRemove: () -> Unit) {
    val c = LocalOmcpColors.current
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.d_grm_t, name), style = MaterialTheme.typography.titleLarge)
            Text(stringResource(R.string.d_grm_d, name, others.toString()), color = c.ink2)
            Text(stringResource(R.string.d_rot_note), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            Button(onClick = onRemove, colors = androidx.compose.material3.ButtonDefaults.buttonColors(containerColor = c.danger),
                modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.d_grm_btn)) }
            OutlinedButton(onClick = onDismiss, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.u_cancel)) }
            Box(Modifier.heightIn(min = 24.dp))
        }
    }
}

/** `group-security` (§5.141): fingerprint 3×5 groups (LTR monospace) + key changes. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GroupSecurityScreen(core: OmcpCore, groupId: String, onBack: () -> Unit) {
    val c = LocalOmcpColors.current
    val sec = remember(groupId) { core.groupSecurity(groupId) }
    Scaffold(topBar = {
        TopAppBar(title = { Text(stringResource(R.string.d_security)) }, navigationIcon = {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        })
    }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            item { Text(stringResource(R.string.d_fp), style = MaterialTheme.typography.titleMedium) }
            item {
                val groups = sec?.fingerprintHex.orEmpty().chunked(4)
                Text(groups.chunked(5).joinToString("\n") { it.joinToString(" ") },
                    fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace,
                    style = MaterialTheme.typography.titleMedium.copy(textDirection = androidx.compose.ui.text.style.TextDirection.Ltr),
                    modifier = Modifier.fillMaxWidth().background(c.surface2, RoundedCornerShape(8.dp)).padding(12.dp))
            }
            item { Text(stringResource(R.string.d_fp_d), color = c.ink2) }
            item { Text(stringResource(R.string.d_rot), style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 8.dp)) }
            items(sec?.events.orEmpty().reversed()) { e ->
                val t = if (e.kind == "left") stringResource(R.string.d_rot1, e.name, e.members.toString())
                    else stringResource(R.string.d_rot2, e.name)
                Column {
                    Text(t)
                    Text(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT).format(java.util.Date(e.atMs)),
                        color = c.ink3, style = MaterialTheme.typography.bodySmall)
                }
            }
            item { Text(stringResource(R.string.d_rot_note), color = c.ink2, style = MaterialTheme.typography.bodySmall) }
        }
    }
}

package chat.omcp.app.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AttachFile
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Send
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.burn.BurnSpec
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.LaunchedEffect
import kotlinx.coroutines.delay
import chat.omcp.app.core.PathKind
import chat.omcp.app.data.BackupGate
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.FileAttachment
import chat.omcp.app.data.FileAttachment.State as S
import chat.omcp.app.ui.vm.ChatThreadViewModel
import chat.omcp.app.ui.vm.ChatsViewModel
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import androidx.compose.runtime.collectAsState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.OutlinedButton
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.PersonAdd
import androidx.compose.material3.TextButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.foundation.clickable
import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.ui.platform.LocalContext
import java.text.DateFormat
import java.util.Date
import chat.omcp.app.ui.components.ChatRow
import chat.omcp.app.ui.components.ChatRowModel
import chat.omcp.app.ui.components.EmptyState
import chat.omcp.app.ui.components.PathBanner
import chat.omcp.app.ui.components.PathChip
import chat.omcp.app.ui.debug.DesignReviewFlags
import chat.omcp.app.ui.theme.LocalOmcpColors

/**
 * Design lock-in: sample rows only when [DesignReviewFlags.showSampleChatsForDesignReview]
 * is on (off by default). Default UI shows real contacts or the empty state.
 */
private val DesignReviewSampleChats = listOf(
    ChatRowModel("1", "Alex Rivera", "See you at the park entrance", "9:38", "AL", true, path = PathKind.Mesh),
    ChatRowModel("2", "Neighbourhood watch", "Maya: Power’s back on our street", "9:12", "N", false, unread = 3, isGroup = true),
)

private fun time(ms: Long) = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(ms))

private fun mb(bytes: Long) = "%.1f".format(bytes / 1_000_000.0)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatsListScreen(
    vm: ChatsViewModel,
    backupGate: BackupGate,
    onOpenChat: (String) -> Unit,
    onAddContact: () -> Unit,
    onBackup: () -> Unit,
    core: chat.omcp.app.core.OmcpCore? = null,
    onOpenGroup: (String) -> Unit = {},
    onOpenCommunity: (String) -> Unit = {},
) {
    var groupsOnly by remember { mutableStateOf(false) }
    var chooser by remember { mutableStateOf(false) }
    var groups by remember { mutableStateOf(listOf<chat.omcp.app.core.CommunityUi>()) }
    var invites by remember { mutableStateOf(listOf<chat.omcp.app.core.CommunityInviteUi>()) }
    if (core != null) LaunchedEffect(Unit) {
        while (true) { groups = core.communities(); invites = core.communityInvites(); kotlinx.coroutines.delay(1_500) }
    }
    if (chooser && core != null) CreateChooserSheet(core, { chooser = false }) { k, id ->
        chooser = false; if (k == "group") onOpenGroup(id) else onOpenCommunity(id)
    }
    val status by vm.status.collectAsState()
    val contacts by vm.contacts.collectAsState()
    val messages by vm.messages.collectAsState()
    val chats = if (DesignReviewFlags.showSampleChatsForDesignReview) DesignReviewSampleChats else contacts.map { c ->
        val last = messages[c.id]?.lastOrNull()
        ChatRowModel(
            id = c.id,
            title = c.displayName,
            preview = last?.let {
                when {
                    it.status == ChatMessage.Status.System -> if (c.restoredUnverified) stringResource(R.string.restored_preview, c.displayName) else ""
                    it.burn != null -> "🔥"
                    it.file?.isPlaceholder == true -> "📎 " + it.file.name + " · " + stringResource(R.string.media_requested_toast).lowercase()
                    else -> it.file?.name ?: it.text
                }
            } ?: "",
            time = last?.let { time(it.timeMs) } ?: "",
            initials = c.displayName.split(" ").mapNotNull { it.firstOrNull()?.uppercase() }.take(2).joinToString(""),
            verified = c.verified,
            restoredUnverified = c.restoredUnverified,
            waitingMedia = messages[c.id]?.count { it.file?.isPlaceholder == true } ?: 0,
            path = if (messages[c.id]?.any { it.mine && it.status == ChatMessage.Status.Queued } == true) PathKind.Queued else null,
        )
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tab_chats)) },
                actions = {
                    IconButton(onClick = onAddContact) {
                        Icon(Icons.Default.PersonAdd, contentDescription = stringResource(R.string.contact_add_title))
                    }
                    if (core != null) IconButton(onClick = { chooser = true }) {
                        Icon(Icons.Default.Add, contentDescription = stringResource(R.string.u_create_t))
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .background(MaterialTheme.colorScheme.background),
        ) {
            if (backupGate == BackupGate.Banner) {
                Callout(
                    stringResource(R.string.backup_banner),
                    CalloutKind.Warn,
                    Modifier.padding(horizontal = 16.dp, vertical = 8.dp).heightIn(min = 48.dp).clickable(onClick = onBackup),
                )
            }
            Row(
                Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (status.nearbyPeers > 0) PathChip(PathKind.Mesh, "Mesh · ${status.nearbyPeers} nearby")
                if (status.queuedCount > 0 || status.nearbyPeers == 0) {
                    PathChip(PathKind.Queued, if (status.queuedCount > 0) "${status.queuedCount} queued" else stringResource(R.string.path_queued))
                }
            }
            if (core != null) Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(!groupsOnly, { groupsOnly = false }, { Text(stringResource(R.string.u_chip_all)) })
                FilterChip(groupsOnly, { groupsOnly = true }, { Text(stringResource(R.string.u_chip_groups)) },
                    leadingIcon = { Icon(Icons.Default.Lock, null, Modifier.size(16.dp)) })
            }
            invites.forEach { inv ->
                Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
                    Column(Modifier.padding(12.dp)) {
                        Text(stringResource(R.string.community_invite_t, inv.name), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.u_grp_e2ee), style = MaterialTheme.typography.bodySmall)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(onClick = { core?.communityAccept(inv.communityId); invites = core?.communityInvites().orEmpty() }) { Text(stringResource(R.string.community_accept)) }
                            OutlinedButton(onClick = { core?.communityDecline(inv.communityId); invites = core?.communityInvites().orEmpty() }) { Text(stringResource(R.string.community_decline)) }
                        }
                    }
                }
            }
            if (groupsOnly && groups.isEmpty()) {
                EmptyState(title = stringResource(R.string.u_e_groups), body = stringResource(R.string.u_e_groups_d))
            } else if (chats.isEmpty() && groups.isEmpty()) {
                EmptyState(
                    title = stringResource(R.string.chats_empty_title),
                    body = stringResource(R.string.chats_empty_body),
                )
            } else {
                LazyColumn {
                    items(groups, key = { "g" + it.id }) { g ->
                        Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).clickable { onOpenGroup(g.id) }.padding(horizontal = 16.dp, vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            GroupAvatar(g.name)
                            Column(Modifier.weight(1f)) {
                                Text(g.name, style = MaterialTheme.typography.titleMedium)
                                Text(if (g.checking) stringResource(R.string.u_checking) else stringResource(R.string.u_grp_lbl, g.members),
                                    style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                    if (!groupsOnly) items(chats, key = { it.id }) { row ->
                        ChatRow(row, onClick = { onOpenChat(row.id) })
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun ChatThreadScreen(
    vm: ChatThreadViewModel,
    onBack: () -> Unit,
    onSecurity: () -> Unit,
) {
    val all by vm.messages.collectAsState()
    val status by vm.status.collectAsState()
    val msgs = all[vm.contactId].orEmpty()
    val contact = vm.contact()
    val c = LocalOmcpColors.current
    var draft by remember { mutableStateOf("") }
    // Per-message 🔥 (long-press Send). Falls back to the chat's Burn-after-read default.
    var burnNext by remember { mutableStateOf(false) }
    var opened by remember { mutableStateOf<OpenedBurn?>(null) }
    LaunchedEffect(vm.contactId) {
        while (true) { vm.tickBurn(); delay(250) }
    }
    opened?.let { o ->
        val live = msgs.firstOrNull { it.id == o.msg.id } ?: o.msg
        BurnViewer(OpenedBurn(live, o.body), now = System::currentTimeMillis) {
            vm.closeBurn(o.msg.id); o.body.fill(0); opened = null
        }
    }
    val path = remember(status, msgs) { vm.path() }
    val ctx = LocalContext.current
    val resendAsks by vm.resendAsks.collectAsState()
    val selfRestored by vm.restoredAtMs.collectAsState()
    var sheetAt by remember { mutableStateOf<Long?>(null) }
    var resendFor by remember { mutableStateOf<ChatMessage?>(null) }
    var resendSent by remember { mutableStateOf(setOf<String>()) }
    var selfBannerOff by remember(vm.contactId) { mutableStateOf(false) }
    var mediaBannerOff by remember(vm.contactId) { mutableStateOf(false) }
    val peer = contact?.displayName ?: ""
    sheetAt?.let { at ->
        chat.omcp.app.ui.components.RestoreSheet(peer, at, onVerifyQr = { sheetAt = null; onSecurity() }, onBlock = null) { sheetAt = null }
    }
    resendFor?.let { m ->
        chat.omcp.app.ui.components.ResendDialog(peer, m.file?.name ?: "", onSend = { t ->
            vm.askToResend(m.id, t); resendSent = resendSent + m.id; resendFor = null
        }) { resendFor = null }
    }
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        var name = "file"
        var size = 0L
        ctx.contentResolver.query(uri, null, null, null, null)?.use { cur ->
            if (cur.moveToFirst()) {
                cur.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 }?.let { name = cur.getString(it) }
                cur.getColumnIndex(OpenableColumns.SIZE).takeIf { it >= 0 }?.let { size = cur.getLong(it) }
            }
        }
        val resolver = ctx.contentResolver
        vm.onFilePicked(name, size, resolver.getType(uri) ?: "application/octet-stream") {
            resolver.openInputStream(uri)?.use { it.readBytes() } ?: error("unreadable")
        }
    }
    val fileError by vm.fileError.collectAsState()
    if (fileError) {
        AlertDialog(
            onDismissRequest = vm::dismissFileError,
            text = { Text(stringResource(R.string.file_read_error)) },
            confirmButton = { TextButton(onClick = vm::dismissFileError) { Text(stringResource(R.string.cancel)) } },
        )
    }
    val confirm by vm.fileConfirm.collectAsState()
    confirm?.let { f ->
        val who = contact?.displayName ?: ""
        AlertDialog(
            onDismissRequest = vm::cancelFile,
            title = { Text(if (f.tooLarge) stringResource(R.string.file_too_large_title) else stringResource(R.string.file_send_title, who)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(f.name, style = MaterialTheme.typography.titleSmall)
                    if (f.tooLarge) {
                        Callout(stringResource(R.string.file_too_large_body, mb(f.sizeBytes), mb(f.maxBytes)), CalloutKind.Danger)
                    } else {
                        Text(stringResource(R.string.file_size_row, mb(f.sizeBytes), mb(f.maxBytes)))
                        Text(f.mime, color = c.ink2)
                        PathChip(f.route, if (f.route == PathKind.Queued) stringResource(R.string.path_queued) else stringResource(R.string.path_mesh))
                        Callout(
                            if (f.route == PathKind.Queued) stringResource(R.string.file_queue_hint, who)
                            else stringResource(R.string.file_mesh_hint, who),
                        )
                    }
                }
            },
            confirmButton = {
                if (f.tooLarge) {
                    TextButton(onClick = { vm.cancelFile(); picker.launch(arrayOf("*/*")) }) { Text(stringResource(R.string.file_choose_another)) }
                } else {
                    TextButton(onClick = vm::confirmFile) {
                        Text(if (f.route == PathKind.Queued) stringResource(R.string.file_queue) else stringResource(R.string.file_send))
                    }
                }
            },
            dismissButton = { TextButton(onClick = vm::cancelFile) { Text(stringResource(R.string.cancel)) } },
        )
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(contact?.displayName ?: "Chat")
                            if (contact?.verified == true) chat.omcp.app.ui.components.VerifiedBadge()
                            else if (contact?.restoredUnverified == true) chat.omcp.app.ui.components.RestoredPill(onClick = { sheetAt = contact.restoredAtMs ?: 0L })
                        }
                        // Product decision: unverified contacts show no badge or label.
                        if (contact?.verified == true) {
                            Text(stringResource(R.string.verified_contact), style = MaterialTheme.typography.labelSmall, color = c.ok)
                        }
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) }
                },
                actions = {
                    IconButton(onClick = onSecurity) {
                        Icon(Icons.Default.Security, contentDescription = stringResource(R.string.security_centre))
                    }
                },
            )
        },
        bottomBar = {
            Row(
                Modifier
                    .fillMaxWidth()
                    .background(MaterialTheme.colorScheme.surface)
                    .padding(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = { picker.launch(arrayOf("*/*")) }) { Icon(Icons.Default.AttachFile, contentDescription = stringResource(R.string.file_attach)) }
                TextField(
                    value = draft,
                    onValueChange = { draft = it },
                    modifier = Modifier.weight(1f),
                    placeholder = { Text("Message") },
                    colors = TextFieldDefaults.colors(
                        focusedContainerColor = c.surface2,
                        unfocusedContainerColor = c.surface2,
                        disabledContainerColor = c.surface2,
                        focusedIndicatorColor = Color.Transparent,
                        unfocusedIndicatorColor = Color.Transparent,
                    ),
                    shape = RoundedCornerShape(10.dp),
                )
                val burnOn = burnNext || vm.chatBurn() != null
                Box(
                    Modifier
                        .size(48.dp)
                        .combinedClickable(
                                                        onClick = {
                                if (draft.isNotBlank()) {
                                    vm.send(draft, if (burnNext) vm.chatBurn() ?: BurnSpec.onOpen() else null)
                                    draft = ""; burnNext = false
                                }
                            },
                            onLongClick = { burnNext = !burnNext },
                        ),
                    contentAlignment = Alignment.Center,
                ) {
                    if (burnOn) Text("🔥") else Icon(Icons.Default.Send, contentDescription = "Send")
                }
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            // Path banner: mesh / gateway / mixed / queued only.
            PathBanner(
                kind = path,
                text = when (path) {
                    PathKind.Gateway -> "Sent via neighbour’s Internet · encrypted end-to-end"
                    PathKind.Queued -> "Waiting for mesh · will send when a path opens"
                    PathKind.Mixed -> "Mixed paths · encrypted end-to-end"
                    PathKind.Mesh -> "Via mesh · encrypted end-to-end"
                },
                modifier = Modifier.fillMaxWidth(),
            )
            if (selfRestored != null && !selfBannerOff) {
                Callout(stringResource(R.string.restored_self_banner), CalloutKind.Info,
                    Modifier.padding(horizontal = 16.dp, vertical = 6.dp).heightIn(min = 48.dp).clickable { selfBannerOff = true })
            }
            val waiting = msgs.count { it.file?.isPlaceholder == true }
            if (waiting > 0 && !mediaBannerOff) {
                Callout(stringResource(R.string.media_banner, waiting, peer), CalloutKind.Info,
                    Modifier.padding(horizontal = 16.dp, vertical = 6.dp).heightIn(min = 48.dp).clickable { mediaBannerOff = true })
            }
            resendAsks.filter { it.contactId == vm.contactId }.forEach { a ->
                val askText = a.note?.let { stringResource(R.string.media_resend_asked_note, peer, a.name, it) }
                    ?: stringResource(R.string.media_resend_asked, peer, a.name)
                Callout(askText, CalloutKind.Info,
                    Modifier.padding(horizontal = 16.dp, vertical = 6.dp).heightIn(min = 48.dp).clickable { vm.dismissResendAsk(a.msgId) })
            }
            LazyColumn(
                modifier = Modifier.weight(1f).fillMaxWidth(),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                items(msgs, key = { it.id }) { m ->
                    val f = m.file
                    if (m.status == ChatMessage.Status.System) {
                        val at = m.restoredAtMs
                        if (at != null) chat.omcp.app.ui.components.RestoreNotice(peer, if (at > 0) at else m.timeMs) { sheetAt = if (at > 0) at else m.timeMs }
                    } else if (f != null && f.isPlaceholder) {
                        chat.omcp.app.ui.components.MissingMediaBubble(
                            mine = m.mine, f = f, peerName = peer,
                            onRequest = { vm.requestMedia(m.id) },
                            onAskResend = { resendFor = m },
                            resendSent = m.id in resendSent,
                        )
                    } else if (m.burn != null) {
                        BurnBubble(m) { vm.openBurn(m.id)?.let { body -> opened = OpenedBurn(vm.messages.value[vm.contactId]!!.first { it.id == m.id }, body) } }
                    } else if (f != null) {
                        FileBubble(
                            mine = m.mine,
                            f = f,
                            peerName = contact?.displayName ?: "",
                            onCancel = { vm.cancelTransfer(m.id) },
                            onOpen = {
                                val path = f.localPath ?: return@FileBubble
                                val app = ctx.applicationContext as chat.omcp.app.OmcpApplication
                                runCatching { ctx.startActivity(app.receivedFiles.openIntent(path, f.mime)) }
                            },
                        )
                    } else {
                        val meta = time(m.timeMs) + " · " + when (m.status) {
                            ChatMessage.Status.Queued -> stringResource(R.string.msg_status_queued)
                            ChatMessage.Status.Sent -> stringResource(R.string.msg_status_sent)
                            ChatMessage.Status.Delivered -> stringResource(R.string.msg_status_delivered)
                            ChatMessage.Status.Received -> stringResource(R.string.msg_status_received)
                            ChatMessage.Status.System -> ""
                        }
                        Bubble(mine = m.mine, text = m.text, meta = meta)
                    }
                }
            }
        }
    }
}

/**
 * File bubble (§5.24, Designy v0.1.1 `file-transfer`): queued / sending / receiving / paused /
 * sent / received (integrity checked in Rust) / failed / integrity fail, with Cancel, Open, Delete.
 */
@Composable
private fun FileBubble(
    mine: Boolean,
    f: FileAttachment,
    peerName: String,
    onCancel: () -> Unit,
    onOpen: () -> Unit,
) {
    val c = LocalOmcpColors.current
    val done = mb(f.doneBytes)
    val total = mb(f.sizeBytes)
    val line = when (f.state) {
        S.Queued -> stringResource(R.string.file_state_queued, total)
        S.Sending -> stringResource(R.string.file_state_sending, done, total)
        S.Sent -> stringResource(R.string.file_state_sent, total)
        S.Receiving -> stringResource(R.string.file_state_receiving, done, total)
        S.Paused -> stringResource(R.string.file_state_paused, done, total)
        S.Received -> stringResource(R.string.file_state_received, total)
        S.Failed -> stringResource(R.string.file_state_failed)
        S.IntegrityFailed -> stringResource(R.string.file_state_integrity, peerName)
        S.Missing, S.Requested, S.Unavailable -> stringResource(R.string.media_waiting, peerName)
    }
    val meta = when (f.state) {
        S.Received -> stringResource(R.string.file_integrity_checked)
        S.Paused -> stringResource(R.string.file_paused_meta)
        else -> null
    }
    val barColor = when (f.state) {
        S.Paused, S.Queued -> c.pathQueue
        S.Failed, S.IntegrityFailed -> c.danger
        else -> MaterialTheme.colorScheme.primary
    }
    val progress = when (f.state) {
        S.Queued -> 0f
        S.Received, S.Sent, S.IntegrityFailed -> 1f
        else -> f.fraction.coerceIn(0f, 1f)
    }
    val pct = (progress * 100).toInt()
    val progressLabel = stringResource(R.string.file_progress_a11y, pct)
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (mine) Alignment.End else Alignment.Start) {
        Column(
            Modifier
                .fillMaxWidth(0.8f)
                .clip(RoundedCornerShape(14.dp))
                .border(1.dp, if (f.state == S.IntegrityFailed) c.danger else c.line, RoundedCornerShape(14.dp))
                .background(MaterialTheme.colorScheme.surface)
                .then(if (f.state == S.Received && f.localPath != null) Modifier.clickable(onClick = onOpen) else Modifier)
                .padding(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(f.name, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
                when (f.state) {
                    S.Queued, S.Sending, S.Paused, S.Receiving -> {
                        val a11y = if (mine) stringResource(R.string.file_cancel_a11y, f.name) else stringResource(R.string.file_cancel_receive_a11y, f.name)
                        IconButton(onClick = onCancel) { Icon(Icons.Default.Close, contentDescription = a11y) }
                    }
                    S.IntegrityFailed -> TextButton(onClick = onCancel) { Text(stringResource(R.string.file_delete)) }
                    S.Received -> if (f.localPath != null) {
                        IconButton(onClick = onOpen) {
                            Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = stringResource(R.string.file_open_a11y, f.name))
                        }
                    }
                    else -> Unit
                }
            }
            Text(line, style = MaterialTheme.typography.bodySmall, color = if (f.state == S.IntegrityFailed || f.state == S.Failed) c.danger else c.ink2)
            LinearProgressIndicator(
                progress = { progress },
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 6.dp)
                    .semantics { contentDescription = progressLabel },
                color = barColor,
            )
            meta?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = c.ink2, modifier = Modifier.padding(top = 4.dp)) }
        }
    }
}

@Composable
private fun Bubble(mine: Boolean, text: String, meta: String) {
    val c = LocalOmcpColors.current
    val bg = if (mine) c.brandSoft else MaterialTheme.colorScheme.surface
    Column(
        Modifier.fillMaxWidth(),
        horizontalAlignment = if (mine) Alignment.End else Alignment.Start,
    ) {
        Box(
            Modifier
                .clip(RoundedCornerShape(14.dp))
                .then(
                    if (!mine) Modifier.border(1.dp, c.line, RoundedCornerShape(14.dp))
                    else Modifier,
                )
                .background(bg)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Text(text, style = MaterialTheme.typography.bodyLarge)
        }
        Text(
            meta,
            style = MaterialTheme.typography.labelSmall,
            color = c.ink3,
            modifier = Modifier.padding(top = 4.dp, start = 4.dp, end = 4.dp),
        )
    }
}

@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package chat.omcp.app.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.text.format.DateUtils
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PushPin
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.onLongClick
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.Contact
import chat.omcp.app.core.NewsStoryUi
import chat.omcp.app.ui.components.EmptyState
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.NEWS_TABS
import chat.omcp.app.ui.vm.NewsViewModel

internal fun tabLabel(t: String) = when (t) {
    "top" -> R.string.news_tab_top; "local" -> R.string.news_tab_local; "world" -> R.string.news_tab_world
    "politics" -> R.string.news_tab_politics; "tech" -> R.string.news_tab_tech; "sport" -> R.string.news_tab_sport
    "health" -> R.string.news_tab_health; else -> R.string.news_tab_emergencies
}

private fun ago(ms: Long): String =
    if (ms <= 0) "" else DateUtils.getRelativeTimeSpanString(ms, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()

/** Text-only source pill (§5.109 SourceLabel): `--surface` fill, 1 dp `--ink-2` border, no logo or brand colour. */
@Composable
fun SourceLabel(name: String) {
    val c = LocalOmcpColors.current
    val d = stringResource(R.string.news_source_cd, name)
    Surface(
        shape = RoundedCornerShape(999.dp), color = MaterialTheme.colorScheme.surface,
        border = BorderStroke(1.dp, c.ink2),
        modifier = Modifier.semantics { contentDescription = d },
    ) {
        Text(name, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp))
    }
}

@Composable
private fun viaLine(s: NewsStoryUi): String? = s.via?.let { stringResource(R.string.news_via, it) }

/**
 * Compact topic chip (§5.108 `.nchip`): 32 dp visual, touch area ≥ 48 dp (WCAG 2.5.8 + platform),
 * long-press mutes (shortcut only; News settings has a single-tap "Mute a topic", WCAG 2.5.7).
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TopicChip(label: String, selected: Boolean, onClick: () -> Unit, onLongClick: (() -> Unit)? = null, check: Boolean = false) {
    val c = LocalOmcpColors.current
    val bg = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface
    val fg = if (selected) c.brandInk else c.ink2
    val muteLabel = stringResource(R.string.news_mute_topic)
    Box(
        Modifier.minimumInteractiveComponentSize()
            .semantics {
                if (onLongClick != null) onLongClick(label = muteLabel) { onLongClick(); true }
            }
            .combinedClickable(role = Role.Tab, onClickLabel = label, onClick = onClick, onLongClick = onLongClick),
        contentAlignment = Alignment.Center,
    ) {
        Row(
            Modifier.heightIn(min = 32.dp)
                .background(bg, RoundedCornerShape(999.dp))
                .border(1.dp, if (selected) MaterialTheme.colorScheme.primary else c.line, RoundedCornerShape(999.dp))
                .padding(horizontal = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (check && selected) Icon(Icons.Default.Check, null, tint = fg, modifier = Modifier.size(14.dp).padding(end = 2.dp))
            Text(label, color = fg, style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.SemiBold)
        }
    }
}

@Composable
private fun StoryCard(s: NewsStoryUi, onOpen: () -> Unit) {
    val c = LocalOmcpColors.current
    val topic = stringResource(tabLabel(s.topic.lowercase()))
    val via = viaLine(s)
    val time = ago(s.publishedMs)
    val pinned = stringResource(R.string.news_pinned)
    val emergency = stringResource(R.string.news_emergency)
    // One focus stop: "headline, source, via contact, time" (§5.109).
    val spoken = listOfNotNull(if (s.emergency) "$emergency, $pinned" else null, s.title, stringResource(R.string.news_source_cd, s.sourceName), via, time)
        .joinToString(", ")
    val border = if (s.emergency) BorderStroke(2.dp, c.danger) else BorderStroke(1.dp, c.line)
    Card(
        onClick = onOpen,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = border,
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)
            .clearAndSetSemantics {
                contentDescription = spoken
                onClick { onOpen(); true }
                if (s.emergency) liveRegion = LiveRegionMode.Polite
            },
    ) {
        Column(Modifier.padding(12.dp)) {
            if (s.emergency) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.Warning, null, tint = c.danger, modifier = Modifier.size(16.dp))
                    Text(" " + emergency.uppercase(), color = c.danger, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.weight(1f))
                    Icon(Icons.Default.PushPin, null, tint = c.ink2, modifier = Modifier.size(14.dp))
                    Text(" $pinned", color = c.ink2, style = MaterialTheme.typography.labelMedium)
                }
                Spacer(Modifier.height(4.dp))
            } else {
                Text("$topic · $time", color = c.ink3, style = MaterialTheme.typography.labelMedium)
            }
            Text(s.title, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurface)
            Spacer(Modifier.height(6.dp))
            FlowRow(verticalArrangement = Arrangement.Center, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SourceLabel(s.sourceName)
                if (via != null) Text(via, style = MaterialTheme.typography.bodySmall, color = c.ink2)
            }
            if (s.emergency) Text(stringResource(R.string.news_alert_from), style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 4.dp))
        }
    }
}

/** News tab (§5.108): settings, chips, status strip, pinned alerts, newest first. No compose/publish entry. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewsHomeScreen(vm: NewsViewModel, onOpen: (String) -> Unit, onSettings: () -> Unit = {}) {
    val st by vm.state.collectAsState()
    val c = LocalOmcpColors.current
    LaunchedEffect(Unit) { vm.refresh() }
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(stringResource(R.string.tab_news), modifier = Modifier.semantics { heading() }) },
            actions = {
                IconButton(onClick = onSettings) { Icon(Icons.Default.Tune, contentDescription = stringResource(R.string.news_settings)) }
            },
        )
    }) { padding ->
        Column(Modifier.padding(padding).imePadding()) {
            Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface).horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                NEWS_TABS.forEach { t ->
                    TopicChip(stringResource(tabLabel(t)), st.tab == t, onClick = { vm.select(t) }, onLongClick = if (t == "emergencies") null else ({ vm.mute(t) }))
                }
            }
            HorizontalDivider(color = c.line)
            if (st.meteredWarning) Text(stringResource(R.string.news_metered_warning), color = c.danger, modifier = Modifier.padding(16.dp).semantics { liveRegion = LiveRegionMode.Polite })
            if (st.tab == "world" || st.tab == "top") {
                Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
                    if (st.tab == "world") Text(stringResource(R.string.news_whl), style = MaterialTheme.typography.titleSmall, modifier = Modifier.semantics { heading() })
                    Text(stringResource(R.string.news_whl_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                }
            }
            if (st.tab == "local" && st.city.isNotBlank()) {
                Row(Modifier.padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.news_city_bar, st.city), style = MaterialTheme.typography.titleSmall)
                        Text(stringResource(R.string.news_region_note), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                    }
                    TextButton(onClick = onSettings) { Text(stringResource(R.string.news_le_btn)) }
                }
            }
            when {
                !st.loaded -> Column {
                    Text(stringResource(R.string.news_loading), modifier = Modifier.padding(16.dp), color = c.ink2)
                }
                st.stories.isEmpty() && st.tab == "local" -> EmptyState(
                    title = if (st.city.isBlank()) stringResource(R.string.news_empty_title) else stringResource(R.string.news_le_t, st.city),
                    body = stringResource(R.string.news_le_d),
                    actionLabel = stringResource(R.string.news_le_btn), onAction = onSettings,
                )
                st.stories.isEmpty() -> EmptyState(
                    title = stringResource(R.string.news_empty_title), body = stringResource(R.string.news_empty_body),
                    actionLabel = stringResource(R.string.news_empty_btn), onAction = onSettings,
                )
                else -> LazyColumn(contentPadding = PaddingValues(bottom = 16.dp)) {
                    items(st.stories, key = { it.idHex }) { s -> StoryCard(s) { onOpen(s.idHex) } }
                }
            }
        }
    }
}

/** Reader (§5.110): summary + "Read on Google News"; bottom bar Share (primary) + More. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewsReaderScreen(vm: NewsViewModel, id: String, contacts: List<Contact>, onBack: () -> Unit) {
    val s = vm.story(id) ?: run { LaunchedEffect(id) { onBack() }; return }
    val ctx = LocalContext.current
    val c = LocalOmcpColors.current
    var share by remember { mutableStateOf(false) }
    var more by remember { mutableStateOf(false) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(s.sourceName) },
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) } },
            )
        },
        bottomBar = {
            Surface(color = MaterialTheme.colorScheme.surface, tonalElevation = 0.dp) {
                Row(Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 16.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Button(onClick = { share = true }, modifier = Modifier.weight(1f).heightIn(min = 48.dp)) { Text(stringResource(R.string.news_share)) }
                    Box {
                        IconButton(onClick = { more = true }) { Icon(Icons.Default.MoreVert, contentDescription = stringResource(R.string.news_more)) }
                        DropdownMenu(expanded = more, onDismissRequest = { more = false }) {
                            DropdownMenuItem(text = { Text(stringResource(R.string.news_mute_this_topic)) }, onClick = { vm.mute(s.topic.lowercase()); more = false; onBack() })
                            DropdownMenuItem(text = { Text(stringResource(R.string.news_copy_link)) }, onClick = {
                                (ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("link", s.link)); more = false
                            })
                        }
                    }
                }
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).verticalScroll(rememberScrollState()).padding(16.dp)) {
            Text(stringResource(tabLabel(s.topic.lowercase())) + " · " + ago(s.publishedMs), color = c.ink3, style = MaterialTheme.typography.labelMedium)
            Text(s.title, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.semantics { heading() })
            Spacer(Modifier.height(8.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SourceLabel(s.sourceName)
                viaLine(s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = c.ink2) }
            }
            if (!s.fetchedHere) Text(stringResource(R.string.news_fetched_by), style = MaterialTheme.typography.bodySmall, color = c.ink2, modifier = Modifier.padding(top = 4.dp))
            Spacer(Modifier.height(12.dp))
            if (s.summary.isNotBlank()) {
                Text(stringResource(R.string.news_summary), style = MaterialTheme.typography.titleSmall, modifier = Modifier.semantics { heading() })
                Text(s.summary, style = MaterialTheme.typography.bodyLarge)
            }
            Spacer(Modifier.height(16.dp))
            OutlinedButton(onClick = { runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(s.link))) } },
                border = BorderStroke(1.dp, c.lineStrong), modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.news_read_gn)) }
            Text(stringResource(R.string.news_read_gn_note), style = MaterialTheme.typography.bodySmall, color = c.ink2)
        }
    }
    if (share) ShareSheet(s, contacts, onSend = { id -> vm.shareToChat(id, s); share = false }, onDismiss = { share = false })
}

/** Share sheet (§5.113): preview, Chats / Communities (single select), note, Send. Close is always visible. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ShareSheet(s: NewsStoryUi, contacts: List<Contact>, onSend: (String) -> Unit, onDismiss: () -> Unit) {
    val c = LocalOmcpColors.current
    var pick by remember { mutableStateOf<String?>(null) }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(horizontal = 16.dp).verticalScroll(rememberScrollState())) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.news_sh_title), style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f).semantics { heading() })
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
            }
            Card(colors = CardDefaults.cardColors(containerColor = c.surface2), modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp)) {
                    Text(s.title, style = MaterialTheme.typography.titleSmall)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { SourceLabel(s.sourceName); viaLine(s)?.let { Text(it, color = c.ink2, style = MaterialTheme.typography.bodySmall) } }
                }
            }
            Text(stringResource(R.string.news_chats_h), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 16.dp).semantics { heading() })
            contacts.forEach { ct ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(selected = pick == ct.id, role = Role.RadioButton) { pick = ct.id },
                    verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(selected = pick == ct.id, onClick = null)
                    Text(ct.displayName, modifier = Modifier.padding(start = 12.dp))
                }
            }
            Text(stringResource(R.string.news_communities_h), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 16.dp).semantics { heading() })
            Text(stringResource(R.string.coming_v02_title), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            Text(stringResource(R.string.news_share_note), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 12.dp))
            Button(onClick = { pick?.let(onSend) }, enabled = pick != null, modifier = Modifier.fillMaxWidth().padding(vertical = 16.dp).heightIn(min = 48.dp)) {
                Text(stringResource(R.string.news_send))
            }
        }
    }
}

/** News settings / interests (§5.115) with WCAG 2.5.7 "Mute a topic" button and emergency alert toggle (§5.111). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewsSettingsScreen(vm: NewsViewModel, onBack: () -> Unit) {
    val st by vm.state.collectAsState()
    val c = LocalOmcpColors.current
    var muteMenu by remember { mutableStateOf(false) }
    var city by remember { mutableStateOf(st.city) }
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(stringResource(R.string.news_settings)) },
            navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back)) } },
        )
    }) { padding ->
        Column(Modifier.padding(padding).imePadding().verticalScroll(rememberScrollState()).padding(16.dp)) {
            Text(stringResource(R.string.news_int_d), color = c.ink2, style = MaterialTheme.typography.bodyMedium)
            Section(stringResource(R.string.news_topics))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                NEWS_TABS.filter { it != "top" }.forEach { t -> TopicChip(stringResource(tabLabel(t)), t in st.topics, onClick = { vm.toggleTopic(t) }, check = true) }
            }
            Section(stringResource(R.string.news_langs))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                listOf("en" to "English", "fr" to "Français").forEach { (k, n) -> TopicChip(n, k in st.languages, onClick = { vm.toggleLanguage(k) }, check = true) }
            }
            Section(stringResource(R.string.news_city))
            OutlinedTextField(value = city, onValueChange = { city = it }, singleLine = true,
                label = { Text(stringResource(R.string.news_or_type)) }, modifier = Modifier.fillMaxWidth())
            Text(stringResource(R.string.news_loc_note), color = c.ink2, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(top = 4.dp))
            Button(onClick = { vm.setCity(city.trim()) }, modifier = Modifier.padding(top = 8.dp).heightIn(min = 48.dp)) { Text(stringResource(R.string.news_save)) }

            Section(stringResource(R.string.news_mute_topics))
            if (st.muted.isEmpty()) Text(stringResource(R.string.news_mute_none), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            st.muted.forEach { t ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(tabLabel(t)) + " · " + stringResource(R.string.news_muted_t), modifier = Modifier.weight(1f))
                    TextButton(onClick = { vm.unmute(t) }) { Text(stringResource(R.string.news_unmute)) }
                }
            }
            Box {
                OutlinedButton(onClick = { muteMenu = true }, border = BorderStroke(1.dp, c.lineStrong), modifier = Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.news_mute_topic)) }
                DropdownMenu(expanded = muteMenu, onDismissRequest = { muteMenu = false }) {
                    NEWS_TABS.filter { it != "emergencies" && it !in st.muted }.forEach { t ->
                        DropdownMenuItem(text = { Text(stringResource(tabLabel(t))) }, onClick = { vm.mute(t); muteMenu = false })
                    }
                }
            }
            Section(stringResource(R.string.news_alert_title))
            ListItem(
                headlineContent = { Text(stringResource(R.string.news_alert_notif)) },
                supportingContent = { Text(stringResource(R.string.news_alert_notif_d)) },
                trailingContent = { Switch(checked = st.emergencyAlerts, onCheckedChange = null) },
                modifier = Modifier.selectable(selected = st.emergencyAlerts, role = Role.Switch) { vm.setEmergencyAlerts(!st.emergencyAlerts) },
            )
            Section(stringResource(R.string.news_storage))
            Text(stringResource(R.string.news_storage_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
        }
    }
}

@Composable
private fun Section(t: String) =
    Text(t, style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 20.dp, bottom = 6.dp).semantics { heading() })

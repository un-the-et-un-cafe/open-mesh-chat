package chat.omcp.app.ui.screens

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.Contact
import chat.omcp.app.core.NewsStoryUi
import chat.omcp.app.ui.components.EmptyState
import chat.omcp.app.ui.vm.NEWS_TABS
import chat.omcp.app.ui.vm.NewsViewModel

private fun tabLabel(t: String) = when (t) {
    "top" -> R.string.news_tab_top; "local" -> R.string.news_tab_local; "world" -> R.string.news_tab_world
    "politics" -> R.string.news_tab_politics; "tech" -> R.string.news_tab_tech; "sport" -> R.string.news_tab_sport
    "health" -> R.string.news_tab_health; else -> R.string.news_tab_emergencies
}

/** Text-only source pill (design §5.109 SourceLabel): no logo, no brand colour. */
@Composable
fun SourceLabel(name: String) {
    val d = stringResource(R.string.news_source_cd, name)
    Surface(shape = MaterialTheme.shapes.small, border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
        modifier = Modifier.semantics { contentDescription = d }) {
        Text(name, style = MaterialTheme.typography.labelSmall, modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp))
    }
}

@Composable
private fun StoryCard(s: NewsStoryUi, onOpen: () -> Unit) {
    val border = if (s.emergency) BorderStroke(2.dp, MaterialTheme.colorScheme.error) else null
    Card(modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp).clickable(onClick = onOpen), border = border) {
        Column(Modifier.padding(12.dp)) {
            if (s.emergency) Text(stringResource(R.string.news_emergency_pinned), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium)
            Text(s.title, style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(6.dp))
            Row(verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                SourceLabel(s.sourceName)
                Spacer(Modifier.width(8.dp))
                Text(s.publisher + (s.via?.let { " · " + stringResource(R.string.news_via, it) } ?: ""), style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

/** News tab (design §5.108): chips, pinned alerts, newest first. No compose/publish entry anywhere. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewsHomeScreen(vm: NewsViewModel, onOpen: (String) -> Unit) {
    val st by vm.state.collectAsState()
    LaunchedEffect(Unit) { vm.refresh() }
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_news)) }) }) { padding ->
        Column(Modifier.padding(padding)) {
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp)) {
                NEWS_TABS.forEach { t ->
                    FilterChip(selected = st.tab == t, onClick = { vm.select(t) }, label = { Text(stringResource(tabLabel(t))) }, modifier = Modifier.padding(end = 6.dp))
                }
            }
            if (st.meteredWarning) Text(stringResource(R.string.news_metered_warning), color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(16.dp))
            if (st.stories.isEmpty()) {
                EmptyState(title = stringResource(R.string.news_empty_title), body = stringResource(R.string.news_empty_body))
            } else {
                LazyColumn { items(st.stories, key = { it.idHex }) { s -> StoryCard(s) { onOpen(s.idHex) } } }
            }
        }
    }
}

/** Reader (design §5.110): summary + "Read on Google News"; share to a chat as a news card (§5.113). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewsReaderScreen(vm: NewsViewModel, id: String, contacts: List<Contact>, onBack: () -> Unit) {
    val s = vm.story(id) ?: run { onBack(); return }
    val ctx = LocalContext.current
    var share by remember { mutableStateOf(false) }
    Scaffold(topBar = { TopAppBar(title = { Text(s.sourceName) }, navigationIcon = { TextButton(onClick = onBack) { Text("‹") } }) }) { padding ->
        Column(Modifier.padding(padding).padding(16.dp)) {
            Text(s.title, style = MaterialTheme.typography.headlineSmall)
            Spacer(Modifier.height(8.dp))
            Text(s.publisher + (s.via?.let { " · " + stringResource(R.string.news_via, it) } ?: ""), style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.height(12.dp))
            if (s.summary.isNotBlank()) {
                Text(stringResource(R.string.news_summary), style = MaterialTheme.typography.titleSmall)
                Text(s.summary)
            }
            Spacer(Modifier.height(16.dp))
            OutlinedButton(onClick = { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(s.link))) }) { Text(stringResource(R.string.news_read_gn)) }
            Text(stringResource(R.string.news_read_gn_note), style = MaterialTheme.typography.bodySmall)
            Spacer(Modifier.height(8.dp))
            Button(onClick = { share = true }) { Text(stringResource(R.string.news_share)) }
        }
    }
    if (share) {
        AlertDialog(
            onDismissRequest = { share = false },
            title = { Text(stringResource(R.string.news_share)) },
            text = {
                Column {
                    Text(stringResource(R.string.news_share_note), style = MaterialTheme.typography.bodySmall)
                    contacts.forEach { c ->
                        TextButton(onClick = { vm.shareToChat(c.id, s); share = false }) { Text(c.displayName) }
                    }
                }
            },
            confirmButton = {},
        )
    }
}

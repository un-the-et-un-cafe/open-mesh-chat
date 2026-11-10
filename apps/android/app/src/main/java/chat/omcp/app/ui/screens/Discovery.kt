package chat.omcp.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import chat.omcp.app.R
import chat.omcp.app.core.DiscoveryUi
import chat.omcp.app.core.ListingSettingsUi
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.theme.LocalOmcpColors

/** TM31 categories, in the §5.149 order (1–8; unknown codes show as Other). */
val CATEGORY_LABELS = listOf(
    R.string.u_cat_1, R.string.u_cat_2, R.string.u_cat_3, R.string.u_cat_4,
    R.string.u_cat_5, R.string.u_cat_6, R.string.u_cat_7, R.string.u_cat_8,
)

fun categoryLabel(code: Int): Int = CATEGORY_LABELS[(if (code in 1..8) code else 8) - 1]

/** Owner bucket for a member count (§20.8 item 2); headers show a range, never a count. */
fun sizeRange(members: Int): Pair<Int, Int> = when {
    members <= 10 -> 1 to 10
    members <= 50 -> 11 to 50
    members <= 200 -> 51 to 200
    members <= 1_000 -> 201 to 1_000
    else -> 1_001 to 5_000
}

@Composable
fun rangeText(min: Int, max: Int): String =
    stringResource(R.string.u_range_fmt, "%,d".format(min), "%,d".format(max))

/** `comm-discover` / `comm-search` (§5.144, §5.149): local search over the on-device index only. */
@Composable
fun DiscoverSection(core: OmcpCore, onOpen: (String) -> Unit) {
    val c = LocalOmcpColors.current
    var query by remember { mutableStateOf("") }
    var cat by remember { mutableStateOf<Int?>(null) }
    var lang by remember { mutableStateOf<String?>(null) }
    var langs by remember { mutableStateOf(listOf<String>()) }
    var nearOnly by remember { mutableStateOf(false) }
    var results by remember { mutableStateOf(listOf<DiscoveryUi>()) }
    var tick by remember { mutableStateOf(0) }
    var reporting by remember { mutableStateOf<String?>(null) }
    var waiting by remember { mutableStateOf(setOf<String>()) }
    LaunchedEffect(query, cat, lang, nearOnly, tick) {
        while (true) {
            langs = core.discoveryLanguages()
            results = core.discoverySearch(query.trim(), cat, lang, nearOnly)
            results.firstOrNull { it.ref in waiting && it.joined }?.let { waiting = waiting - it.ref; onOpen(it.id) }
            kotlinx.coroutines.delay(2_000)
        }
    }
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(stringResource(R.string.u_discover), style = MaterialTheme.typography.titleSmall)
        OutlinedTextField(query, { query = it.take(80) }, Modifier.fillMaxWidth(), singleLine = true,
            label = { Text(stringResource(R.string.u_disc_search)) })
        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            item { FilterChip(selected = cat == null, onClick = { cat = null }, label = { Text(stringResource(R.string.u_cat_all)) }) }
            items((1..8).toList()) { k ->
                FilterChip(selected = cat == k, onClick = { cat = if (cat == k) null else k }, label = { Text(stringResource(categoryLabel(k))) })
            }
            item { FilterChip(selected = nearOnly, onClick = { nearOnly = !nearOnly }, label = { Text(stringResource(R.string.u_f_nearby)) }) }
            if (langs.isNotEmpty()) {
                item { FilterChip(selected = lang == null, onClick = { lang = null }, label = { Text(stringResource(R.string.u_lang_all)) }) }
                items(langs) { code ->
                    FilterChip(selected = lang == code, onClick = { lang = if (lang == code) null else code }, label = { Text(code) })
                }
            }
        }
        Text(stringResource(R.string.u_disc_note), color = c.ink2, style = MaterialTheme.typography.bodySmall)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(Icons.Filled.Lock, null)
            Text(stringResource(R.string.u_no_groups_disc), color = c.ink2, style = MaterialTheme.typography.bodySmall)
        }
        if (results.isEmpty()) {
            if (query.isBlank()) {
                Text(stringResource(R.string.u_e_heard), style = MaterialTheme.typography.titleSmall)
                Text(stringResource(R.string.u_loading), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            } else {
                Text(stringResource(R.string.u_e_srch2, query.trim()), style = MaterialTheme.typography.titleSmall)
                Text(stringResource(R.string.u_e_srch2_d), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            }
        } else {
            Text(stringResource(R.string.u_nearby), color = c.ink2, style = MaterialTheme.typography.labelLarge)
        }
    }
    results.forEach { r ->
        DiscoveryCard(r, waiting = r.ref in waiting,
            onJoin = {
                if (r.approvalNeeded) return@DiscoveryCard
                when (core.discoveryJoin(r.ref)) {
                    true -> onOpen(r.id)
                    false -> waiting = waiting + r.ref
                    null -> {}
                }
            },
            onHide = { core.discoveryHide(r.ref); tick++ },
            onReport = { reporting = r.ref })
    }
    reporting?.let { ref ->
        ReportSheet(stringResource(R.string.u_rep_comm), onDismiss = { reporting = null; tick++ }) { reason, _ -> core.discoveryReport(ref, reason) }
    }
}

@Composable
private fun DiscoveryCard(r: DiscoveryUi, waiting: Boolean, onJoin: () -> Unit, onHide: () -> Unit, onReport: () -> Unit) {
    val c = LocalOmcpColors.current
    var menu by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                CommunityIcon(r.name, r.id)
                Column(Modifier.weight(1f)) {
                    Text(r.name, style = MaterialTheme.typography.titleMedium)
                    Text(
                        buildString {
                            append(stringResource(categoryLabel(r.category)))
                            if (r.tags.isNotEmpty()) append(" · ").append(r.tags.joinToString(" "))
                            if (r.langs.isNotEmpty()) append(" · ").append(r.langs.joinToString(" "))
                        },
                        color = c.ink2, style = MaterialTheme.typography.bodySmall)
                    if (!r.joined && r.approvalNeeded) InviteOnlyLine(16)
                }
                IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, stringResource(R.string.u_rep_comm)) }
                DropdownMenu(menu, { menu = false }) {
                    DropdownMenuItem(text = { Text(stringResource(R.string.u_hide)) }, onClick = { menu = false; onHide() })
                    DropdownMenuItem(text = { Text(stringResource(R.string.u_rep_comm)) }, onClick = { menu = false; onReport() })
                }
            }
            if (r.description.isNotBlank()) Text(r.description, style = MaterialTheme.typography.bodyMedium)
            if (r.nearYou) {
                // TM31: no distance, place or map; worked out on this phone only.
                val near = stringResource(R.string.u_near_tag)
                Row(Modifier.semantics { contentDescription = near }, verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    Icon(Icons.Filled.Place, null)
                    Text(near, style = MaterialTheme.typography.labelLarge)
                }
            }
            Text(stringResource(R.string.u_size_after_join), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            Text(stringResource(R.string.u_open_not_e2ee), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                when {
                    r.joined -> Text(stringResource(R.string.u_joined), color = c.ink2)
                    r.approvalNeeded -> { /* invite line is under the meta; Hide / Report stay */ }
                    else -> Button(enabled = !waiting, onClick = onJoin, modifier = Modifier.heightIn(min = 48.dp)) {
                        Text(stringResource(if (waiting) R.string.u_joining else R.string.u_join_btn))
                    }
                }
                TextButton(onClick = onHide, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_hide)) }
            }
        }
    }
}

/** Founder only: Listed switch (off by default) with the TM27 notice first (`comm-listed-notice`). */
@Composable
fun ListedSettingRow(core: OmcpCore, communityId: String, isFounder: Boolean) {
    val c = LocalOmcpColors.current
    if (!isFounder) return
    var s by remember(communityId) { mutableStateOf(core.communityListing(communityId)) }
    var desc by remember(communityId) { mutableStateOf(s.description) }
    var tags by remember(communityId) { mutableStateOf(s.tags.joinToString(" ")) }
    var langs by remember(communityId) { mutableStateOf(s.langs.joinToString(" ")) }
    var cat by remember(communityId) { mutableStateOf(s.category) }
    var notice by remember { mutableStateOf(false) }
    var failed by remember { mutableStateOf(false) }
    fun settings(listed: Boolean) = ListingSettingsUi(listed, desc.trim(), cat,
        tags.split(' ', ',').map { it.trim().lowercase() }.filter { it.isNotEmpty() }.take(5),
        langs.split(' ', ',').map { it.trim() }.filter { it.isNotEmpty() && it.all { ch -> ch.isLetterOrDigit() || ch == '-' } }.take(3))
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(if (s.listed) R.string.u_listed else R.string.u_unlisted), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(if (s.listed) R.string.u_listed_d else R.string.u_unlisted_d), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            }
            Switch(checked = s.listed, onCheckedChange = { on ->
                if (on) notice = true
                else { failed = !core.setListed(communityId, settings(false)); s = core.communityListing(communityId) }
            })
        }
        Text(stringResource(R.string.u_vis_change), color = c.ink3, style = MaterialTheme.typography.bodySmall)
        OutlinedTextField(desc, { desc = it.codePoints().limit(140).toArray().let { a -> String(a, 0, a.size) } }, Modifier.fillMaxWidth(),
            label = { Text(stringResource(R.string.u_desc_l)) },
            supportingText = { Text("${desc.codePointCount(0, desc.length)} / 140") })
        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            items((1..8).toList()) { k -> FilterChip(selected = cat == k, onClick = { cat = k }, label = { Text(stringResource(categoryLabel(k))) }) }
        }
        OutlinedTextField(tags, { tags = it.take(130) }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(R.string.u_tags_l)) })
        OutlinedTextField(langs, { langs = it.take(40) }, Modifier.fillMaxWidth(), singleLine = true, label = { Text(stringResource(R.string.u_langs_l)) })
        Text(stringResource(R.string.u_listing_rules), color = c.ink3, style = MaterialTheme.typography.bodySmall)
        if (s.listed) TextButton(onClick = { failed = !core.setListed(communityId, settings(true)); s = core.communityListing(communityId) },
            Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_listing_update)) }
        if (failed) Callout(stringResource(R.string.u_listing_fail))
    }
    if (notice) {
        val keep = remember { FocusRequester() }
        LaunchedEffect(Unit) { runCatching { keep.requestFocus() } }
        AlertDialog(onDismissRequest = { notice = false },
            title = { Text(stringResource(R.string.u_listed)) },
            text = { Text(stringResource(R.string.u_listed_notice)) },
            // Focus starts on "Keep unlisted", the safe choice (§5.149).
            dismissButton = { TextButton(onClick = { notice = false }, Modifier.focusRequester(keep)) { Text(stringResource(R.string.u_keep_unlisted)) } },
            confirmButton = { TextButton(onClick = {
                notice = false; failed = !core.setListed(communityId, settings(true)); s = core.communityListing(communityId)
            }) { Text(stringResource(R.string.u_turn_on_listed)) } })
    }
}

/** Non-interactive invite-only line (§5.157): ink-2, 13 px, not tappable. */
@Composable
fun InviteOnlyLine(iconDp: Int, modifier: Modifier = Modifier) {
    val c = LocalOmcpColors.current
    val text = stringResource(R.string.u_comm_join_invite)
    Row(
        modifier.semantics(mergeDescendants = true) { },
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            Icons.Outlined.Info,
            contentDescription = null,
            tint = c.ink2,
            modifier = Modifier.size(iconDp.dp).clearAndSetSemantics { },
        )
        Text(text, color = c.ink2, fontSize = 13.sp, fontWeight = FontWeight.Normal, lineHeight = (13 * 1.4).sp)
    }
}

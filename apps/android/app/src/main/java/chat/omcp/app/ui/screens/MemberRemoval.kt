package chat.omcp.app.ui.screens

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.disabled
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.core.OmcpCore
import chat.omcp.app.core.OpenRemovalUi
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.theme.LocalOmcpColors

/** Duration presets (§5.147): seconds → label. No permanent option for community roles. */
val REMOVAL_DURATIONS = listOf(
    3_600 to R.string.u_dur_1h, 86_400 to R.string.u_dur_24h, 604_800 to R.string.u_dur_7d,
    2_592_000 to R.string.u_dur_30d, 31_536_000 to R.string.u_dur_365d,
)

/** Removal reasons (§23.10.4(3)) → protocol codes; 9 needs a note. */
val REMOVAL_REASONS = listOf(
    R.string.u_rmr_spam to 5, R.string.u_rmr_harass to 4, R.string.u_rmr_hate to 1, R.string.u_rmr_threats to 7,
    R.string.u_rmr_illegal to 2, R.string.u_rmr_selfharm to 10, R.string.u_rmr_other to 9,
)

/** Rank used to show "Remove" only on strictly lower ranks (§23.10.11(1)(b)). */
fun roleRank(role: String): Int = when (role) { "founder" -> 3; "admin" -> 2; "moderator" -> 1; else -> 0 }

/** `comm-report-selfharm` (§5.146): shown inside the report sheet; never blocks reporting. */
@Composable
fun SelfHarmNotice(url: String) {
    val ctx = LocalContext.current
    val c = LocalOmcpColors.current
    Column(Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }.padding(vertical = 4.dp)) {
        Text(stringResource(R.string.u_sh_t), style = MaterialTheme.typography.titleSmall)
        Text(stringResource(R.string.u_sh_d), color = c.ink2)
        OutlinedButton(onClick = { runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) } },
            Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_sh_link)) }
        Text(stringResource(R.string.u_sh_ext), color = c.ink3, style = MaterialTheme.typography.bodySmall)
    }
}

/** `comm-remove-member` (§5.147): duration chips, required reason, quota line. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MemberRemoveSheet(core: OmcpCore, communityId: String, memberKey: String, name: String, onDone: () -> Unit) {
    val c = LocalOmcpColors.current
    var dur by remember { mutableStateOf(2) }
    var reason by remember { mutableStateOf(-1) }
    var note by remember { mutableStateOf("") }
    var failed by remember { mutableStateOf(false) }
    val left = remember(communityId) { core.openRemovalsLeftThisHour(communityId) }
    ModalBottomSheet(onDismissRequest = onDone) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(stringResource(R.string.u_rmm_t, name), style = MaterialTheme.typography.titleLarge)
            if (left == 0) {
                Callout(stringResource(R.string.u_mod_limit_t) + "\n" + stringResource(R.string.u_mod_limit_d), CalloutKind.Danger)
            }
            Text(stringResource(R.string.u_dur), style = MaterialTheme.typography.titleSmall)
            Column(Modifier.selectableGroup()) {
                REMOVAL_DURATIONS.forEachIndexed { i, (_, res) ->
                    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).selectable(dur == i, role = Role.RadioButton) { dur = i },
                        verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(dur == i, null); Text(stringResource(res), Modifier.padding(start = 8.dp))
                    }
                }
            }
            Text(stringResource(R.string.u_reason_req), style = MaterialTheme.typography.titleSmall)
            Column(Modifier.selectableGroup()) {
                REMOVAL_REASONS.forEachIndexed { i, (res, _) ->
                    Row(Modifier.fillMaxWidth().heightIn(min = 44.dp).selectable(reason == i, role = Role.RadioButton) { reason = i },
                        verticalAlignment = Alignment.CenterVertically) {
                        RadioButton(reason == i, null); Text(stringResource(res), Modifier.padding(start = 8.dp))
                    }
                }
            }
            val code = REMOVAL_REASONS.getOrNull(reason)?.second
            if (code == 9) OutlinedTextField(note, { note = it.take(500) }, Modifier.fillMaxWidth(), label = { Text(stringResource(R.string.u_reason_req)) })
            Text(stringResource(R.string.u_rmm_what), color = c.ink2, style = MaterialTheme.typography.bodySmall)
            if (left != null) Text(stringResource(R.string.u_left_q, left), color = c.ink3, style = MaterialTheme.typography.bodySmall)
            if (failed) Callout(stringResource(R.string.u_rmm_fail), CalloutKind.Danger)
            val ok = code != null && (code != 9 || note.isNotBlank()) && left != 0
            val durLabel = stringResource(REMOVAL_DURATIONS[dur].second)
            Button(enabled = ok, onClick = {
                if (core.openRemoveMember(communityId, memberKey, code!!, REMOVAL_DURATIONS[dur].first, note.trim())) onDone() else failed = true
            }, colors = ButtonDefaults.buttonColors(containerColor = c.danger), modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
                Text(stringResource(R.string.u_rmm_btn, durLabel))
            }
            OutlinedButton(onClick = onDone, Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.u_cancel)) }
            Box(Modifier.heightIn(min = 24.dp))
        }
    }
}

private fun timeLeft(ms: Long): String {
    val m = (ms / 60_000).coerceAtLeast(1)
    val d = m / 1_440; val h = (m % 1_440) / 60
    return when { d > 0 -> "${d} d ${h} h"; h > 0 -> "${h} h ${m % 60} min"; else -> "${m} min" }
}

/** `comm-removed-member-notice` (§5.147): role, reason, time left; read-only composer. */
@Composable
fun RemovedMemberNotice(r: OpenRemovalUi, communityName: String, nowMs: Long = System.currentTimeMillis()) {
    val c = LocalOmcpColors.current
    val title = stringResource(when (r.byRole) {
        "team" -> R.string.u_rn_team; "owner" -> R.string.u_rn_f; "admin" -> R.string.u_rn_a; else -> R.string.u_rn_t
    }, communityName)
    val reason = REMOVAL_REASONS.firstOrNull { it.second == r.reason }?.first
    val body = r.endsMs?.let { stringResource(R.string.u_rn_d, timeLeft(it - nowMs)) } ?: stringResource(R.string.u_rn_perm)
    Column(Modifier.fillMaxWidth().padding(8.dp).semantics { liveRegion = LiveRegionMode.Polite }) {
        Callout(title + "\n" + body, CalloutKind.Danger)
        if (reason != null) Text(stringResource(R.string.u_rn_reason, stringResource(reason)), color = c.ink2)
        Text(stringResource(R.string.u_rn_appeal), color = c.ink3, style = MaterialTheme.typography.bodySmall)
        OutlinedTextField("", {}, Modifier.fillMaxWidth().semantics { disabled() }, enabled = false,
            label = { Text(stringResource(R.string.u_rn_comp)) })
    }
}

/** `comm-founder-alert` + Removals list (§5.147): owner/admins only. */
@Composable
fun FounderRemovalsSection(core: OmcpCore, communityId: String, isFounder: Boolean, onChanged: () -> Unit) {
    val c = LocalOmcpColors.current
    var show by remember { mutableStateOf(false) }
    val alerts = core.openOwnerAlerts(communityId)
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        alerts.forEach { a ->
            Callout(stringResource(R.string.u_fa_t, chat.omcp.app.ui.components.Pseudonym.of(a.actorKey).name, a.removals24h) + "\n" + stringResource(R.string.u_fa_d), CalloutKind.Danger)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = { show = true }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_fa_view)) }
                if (isFounder) TextButton(onClick = { core.openRevokeRole(communityId, a.actorKey); onChanged() }, Modifier.heightIn(min = 48.dp)) {
                    Text(stringResource(R.string.u_fa_revoke))
                }
            }
        }
        TextButton(onClick = { show = !show }, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.u_removals_t)) }
        if (show) {
            val list = core.openRemovals(communityId)
            if (list.isEmpty()) Text(stringResource(R.string.u_removals_e), color = c.ink3)
            list.forEach { e ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
                    val reason = REMOVAL_REASONS.firstOrNull { it.second == e.reason }?.first
                    Text("${e.actorRole} · ${e.memberId.take(8)} · " + (reason?.let { stringResource(it) } ?: ""), Modifier.weight(1f), color = c.ink2)
                    // MEMBER_RESTORE (0x5C action 2): rank rules enforced in core.
                    if (e.action == 1 && e.memberKey.isNotEmpty()) {
                        TextButton(
                            onClick = { if (core.openRestoreMember(communityId, e.memberKey)) onChanged() },
                            Modifier.heightIn(min = 48.dp),
                        ) { Text(stringResource(R.string.u_restore)) }
                    }
                }
            }
        }
    }
}

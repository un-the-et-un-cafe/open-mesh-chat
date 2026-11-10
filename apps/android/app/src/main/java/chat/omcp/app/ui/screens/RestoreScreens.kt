package chat.omcp.app.ui.screens

import android.content.Intent
import android.net.Uri
import android.text.format.Formatter
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import chat.omcp.app.R
import chat.omcp.app.restore.RestoreErrorKind
import chat.omcp.app.restore.RestoreStage
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.PlainPage
import chat.omcp.app.ui.vm.OnboardingState
import chat.omcp.app.ui.vm.OnboardingViewModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.text.DateFormat
import java.util.Date

/** Restore from backup screens, DESIGN_SPEC §5L (§5.83–5.90). */

private fun CoroutineScope.bg(f: () -> Unit) = launch(Dispatchers.IO) { f() }

@Composable
fun RestoreCheckingScreen() = PlainPage(stringResource(R.string.restore_detect_title)) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
        BodyText(stringResource(R.string.restore_checking))
    }
}

@Composable
private fun Kv(label: String, value: String) = Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
    Text(label, style = MaterialTheme.typography.bodyMedium)
    Text(value, style = MaterialTheme.typography.bodyMedium)
}

@Composable
fun RestoreDetectScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) {
    val ctx = LocalContext.current
    PlainPage(stringResource(R.string.restore_detect_title)) {
        BodyText(stringResource(R.string.restore_detect_body))
        OutlinedCard(Modifier.fillMaxWidth()) {
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(s.serviceLabel, style = MaterialTheme.typography.titleSmall)
                    Text(stringResource(R.string.restore_found), color = MaterialTheme.colorScheme.primary)
                }
                s.foundBackup?.let { m ->
                    Kv(stringResource(R.string.restore_kv_backed_up), DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(m.createdAtMs)))
                    if (m.deviceHint.isNotBlank()) Kv(stringResource(R.string.restore_kv_device), m.deviceHint)
                    Kv(stringResource(R.string.restore_kv_size), Formatter.formatShortFileSize(ctx, s.foundBackupSize.toLong()))
                    Kv(stringResource(R.string.restore_kv_protection), stringResource(R.string.restore_protection_password))
                }
            }
        }
        Button(onClick = { io.bg(vm::restoreTapped) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_cta)) }
        OutlinedButton(onClick = vm::startFresh, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_start_fresh)) }
        Row {
            TextButton(onClick = vm::usePhraseInstead) { Text(stringResource(R.string.restore_use_words)) }
            TextButton(onClick = { io.bg(vm::chooseDrive) }) { Text(stringResource(R.string.restore_other_account)) }
        }
    }
    if (s.freshConfirm) {
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
        AlertDialog(
            onDismissRequest = vm::restoreInstead,
            title = { Text(stringResource(R.string.restore_fresh_title)) },
            text = { Text(stringResource(R.string.restore_fresh_body, s.serviceLabel)) },
            // Default (focused, primary) action is the safe one.
            confirmButton = { Button(onClick = vm::restoreInstead, modifier = Modifier.focusRequester(focus)) { Text(stringResource(R.string.restore_fresh_restore_instead)) } },
            dismissButton = { TextButton(onClick = vm::confirmStartFresh) { Text(stringResource(R.string.restore_fresh_create)) } },
        )
    }
}

@Composable
fun RestoreSourceScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) = PlainPage(stringResource(R.string.restore_source_title)) {
    BodyText(stringResource(R.string.restore_source_body))
    if (s.driveAvailable) {
        OutlinedCard(onClick = { io.bg(vm::chooseDrive) }, enabled = !s.restoreBusy, modifier = Modifier.fillMaxWidth()) {
            Column(Modifier.padding(16.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.restore_source_drive), style = MaterialTheme.typography.titleSmall)
                    if (s.restoreBusy) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                }
                BodyText(stringResource(R.string.restore_source_drive_sub))
            }
        }
    }
    OutlinedCard(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(stringResource(R.string.restore_source_words), style = MaterialTheme.typography.titleSmall)
            BodyText(stringResource(R.string.restore_source_words_sub))
        }
    }
    TextButton(onClick = vm::backToWelcome) { Text(stringResource(R.string.back)) }
}

@Composable
fun RestoreNoBackupScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) = PlainPage(stringResource(R.string.restore_nobackup_title)) {
    BodyText(stringResource(R.string.restore_nobackup_body, s.serviceLabel))
    Button(onClick = { io.bg(vm::chooseDrive) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_try_other_account)) }
    OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
    TextButton(onClick = vm::confirmStartFresh) { Text(stringResource(R.string.restore_start_fresh)) }
}

@Composable
fun RestorePasswordScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) {
    val locked = s.restoreLockMinutes > 0
    LaunchedEffect(locked) {
        while (locked) { delay(5_000); vm.refreshLock() }
    }
    PlainPage(stringResource(R.string.restore_pw_title)) {
        BodyText(stringResource(R.string.restore_pw_body))
        OutlinedTextField(
            s.restorePassword, vm::setRestorePassword,
            label = { Text(stringResource(R.string.restore_pw_label)) },
            enabled = !locked && !s.restoreBusy,
            isError = s.restoreWrong,
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
            modifier = Modifier.fillMaxWidth(),
        )
        if (locked) {
            // Text changes once per minute; the live region announces each change once.
            Column(Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite }) {
                Callout(
                    stringResource(R.string.restore_lockout_title, s.restoreLockMinutes) + "\n" + stringResource(R.string.restore_lockout_body),
                    CalloutKind.Warn,
                )
            }
        } else if (s.restoreWrong) {
            Column(Modifier.semantics { liveRegion = LiveRegionMode.Assertive }) {
                Callout(pluralStringResource(R.plurals.restore_pw_wrong, s.restoreTriesLeft, s.restoreTriesLeft, s.restoreNextLockMinutes), CalloutKind.Danger)
            }
        }
        val checking = stringResource(R.string.restore_pw_checking)
        Button(
            onClick = { io.bg { vm.restoreFromCloud() } },
            enabled = !locked && !s.restoreBusy && s.restorePassword.isNotEmpty(),
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (s.restoreBusy) {
                CircularProgressIndicator(Modifier.size(18.dp).semantics { contentDescription = checking }, strokeWidth = 2.dp)
            } else {
                Text(stringResource(R.string.restore_cta))
            }
        }
        // 24-word path is always enabled, even during a lockout.
        OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
    }
}

@Composable
fun RestoreProgressScreen(s: OnboardingState) {
    val ctx = LocalContext.current
    PlainPage(stringResource(R.string.restore_progress_title)) {
        BodyText(stringResource(R.string.restore_progress_body))
        LinearProgressIndicator(progress = { s.restoreFraction }, modifier = Modifier.fillMaxWidth())
        val stages = listOf(
            RestoreStage.Download to (stringResource(R.string.restore_stage_download) to stringResource(R.string.restore_stage_download_sub, Formatter.formatShortFileSize(ctx, s.foundBackupSize.toLong()), s.serviceLabel)),
            RestoreStage.Decrypt to (stringResource(R.string.restore_stage_decrypt) to stringResource(R.string.restore_stage_decrypt_sub)),
            RestoreStage.Contacts to (stringResource(R.string.restore_stage_contacts) to stringResource(R.string.restore_stage_contacts_sub)),
        )
        stages.forEach { (stage, t) ->
            val done = stage.ordinal < s.restoreStage.ordinal
            val current = stage == s.restoreStage
            val mark = if (done) "✓" else if (current) "•" else "○"
            Column(if (current) Modifier.semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Polite } else Modifier) {
                Text("$mark ${t.first}", style = if (current) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodyMedium)
                BodyText(t.second)
            }
        }
    }
}

@Composable
fun RestoreSuccessScreen(s: OnboardingState, vm: OnboardingViewModel) {
    val title = s.restoredName?.let { stringResource(R.string.restore_success_title_named, it) } ?: stringResource(R.string.restore_success_title)
    PlainPage(title) {
        BodyText(stringResource(R.string.restore_success_body))
        if (s.quotaWarning) Callout(stringResource(R.string.restore_quota_warning, s.serviceLabel), CalloutKind.Warn)
        Text(stringResource(R.string.restore_restored), style = MaterialTheme.typography.titleSmall, modifier = Modifier.semantics { heading() })
        BodyText("✓ " + stringResource(R.string.restore_restored_identity))
        BodyText("✓ " + stringResource(R.string.restore_stage_contacts) + " (${s.restoredContacts})")
        BodyText("✓ " + stringResource(R.string.restore_restored_verified) + " (${s.restoredVerified})")
        BodyText("✓ " + stringResource(R.string.restore_restored_profile))
        Text(stringResource(R.string.restore_not_restored), style = MaterialTheme.typography.titleSmall, modifier = Modifier.semantics { heading() })
        BodyText("– " + stringResource(R.string.restore_not_history))
        BodyText("– " + stringResource(R.string.restore_not_burn))
        BodyText("– " + stringResource(R.string.restore_not_nearby))
        Button(onClick = vm::openChats, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_open_chats)) }
    }
}

@Composable
fun RestoreErrorScreen(s: OnboardingState, vm: OnboardingViewModel, io: CoroutineScope) {
    val ctx = LocalContext.current
    val svc = s.serviceLabel
    val (title, body) = when (s.restoreError) {
        RestoreErrorKind.Offline -> stringResource(R.string.restore_err_offline_title) to stringResource(R.string.restore_err_offline_body, svc)
        RestoreErrorKind.PermissionRevoked -> stringResource(R.string.restore_err_permission_title, svc) to stringResource(R.string.restore_err_permission_body, svc)
        RestoreErrorKind.QuotaFull -> stringResource(R.string.restore_err_quota_title, svc) to stringResource(R.string.restore_err_quota_body)
        RestoreErrorKind.TooNew -> stringResource(R.string.restore_err_toonew_title) to stringResource(R.string.restore_err_toonew_body)
        RestoreErrorKind.Corrupted, null -> stringResource(R.string.restore_err_corrupted_title) to stringResource(R.string.restore_err_corrupted_body)
    }
    PlainPage(title) {
        Column(Modifier.semantics { liveRegion = LiveRegionMode.Assertive }) { Callout(body, CalloutKind.Danger) }
        val primary = when (s.restoreError) {
            RestoreErrorKind.Offline -> R.string.restore_try_again
            RestoreErrorKind.PermissionRevoked -> R.string.restore_allow_access
            RestoreErrorKind.QuotaFull -> R.string.restore_anyway
            RestoreErrorKind.TooNew -> R.string.restore_update
            RestoreErrorKind.Corrupted, null -> R.string.restore_use_words
        }
        Button(
            onClick = {
                if (s.restoreError == RestoreErrorKind.TooNew) {
                    val pkg = ctx.packageName
                    runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$pkg")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
                        .onFailure { runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=$pkg")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
                } else {
                    io.bg(vm::errorPrimary)
                }
            },
            enabled = !s.restoreBusy,
            modifier = Modifier.fillMaxWidth(),
        ) { Text(stringResource(primary)) }
        if (s.restoreError == RestoreErrorKind.Corrupted) {
            OutlinedButton(onClick = { io.bg(vm::chooseDrive) }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_try_other_account)) }
        } else {
            OutlinedButton(onClick = vm::usePhraseInstead, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_use_words)) }
        }
    }
}


package chat.omcp.app.ui.screens

import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import chat.omcp.app.R
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.PlainPage
import chat.omcp.app.ui.vm.BackupSettingsViewModel
import java.text.DateFormat
import java.util.Date

/** Settings › Backup (plain layout; design v0.1.2 pending). */
@Composable
fun BackupSettingsScreen(
    vm: BackupSettingsViewModel,
    requestDeviceAuth: (onSuccess: () -> Unit) -> Unit,
    secureWindow: (Boolean) -> Unit,
    onBack: () -> Unit,
) {
    val s by vm.state.collectAsState()
    // Cloud I/O (Drive REST, consent) never runs on the main thread.
    val io = androidx.compose.runtime.rememberCoroutineScope()
    LaunchedEffect(s.shownPhrase != null) { secureWindow(s.shownPhrase != null) }
    PlainPage(stringResource(R.string.backup_settings_title)) {
        Text(stringResource(R.string.backup_drive), style = MaterialTheme.typography.titleMedium)
        when {
            !s.driveAvailable -> Callout(stringResource(R.string.backup_drive_unavailable), CalloutKind.Warn)
            s.driveOn -> Callout(
                stringResource(R.string.backup_last, DateFormat.getDateTimeInstance().format(Date(s.lastBackupMs))),
                CalloutKind.Ok,
            )
            else -> Callout(stringResource(R.string.backup_drive_off))
        }
        if (s.driveAvailable) {
            BodyText(stringResource(R.string.backup_honest_copy))
            OutlinedTextField(
                s.password, vm::setPassword,
                label = { Text(if (s.driveOn) stringResource(R.string.backup_change_pw) else stringResource(R.string.onb_backup_password)) },
                supportingText = { Text(stringResource(R.string.backup_pw_rule)) },
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
                modifier = Modifier.fillMaxWidth(),
            )
            Button(onClick = { io.launch(kotlinx.coroutines.Dispatchers.IO) { vm.uploadWithPassword() } }, enabled = s.passwordOk, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.backup_drive_cta))
            }
            if (s.driveOn) {
                OutlinedButton(onClick = { io.launch(kotlinx.coroutines.Dispatchers.IO) { vm.deleteCloudBackup() } }, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(R.string.backup_delete))
                }
            }
        }
        Text(stringResource(R.string.backup_written), style = MaterialTheme.typography.titleMedium)
        BodyText(if (s.writtenOn) stringResource(R.string.backup_written_done) else stringResource(R.string.backup_written_off))
        val words = s.shownPhrase
        if (words == null) {
            OutlinedButton(onClick = { requestDeviceAuth { vm.revealPhraseAfterAuth() } }, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.backup_view_phrase))
            }
        } else {
            words.chunked(3).forEachIndexed { row, ws ->
                Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                    ws.forEachIndexed { i, w -> Text("${row * 3 + i + 1}. $w", fontFamily = FontFamily.Monospace) }
                }
            }
            OutlinedButton(onClick = vm::hidePhrase, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.backup_hide_phrase)) }
        }
        MediaBackupSection(s, vm)
        s.message?.let { Callout(it) }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

/** Chat backup + Settings › Backup › Photos, videos and files (§5.96). */
@Composable
private fun MediaBackupSection(s: chat.omcp.app.ui.vm.BackupSettingsState, vm: BackupSettingsViewModel) {
    val m = s.media
    Text(stringResource(R.string.chatbackup_title), style = MaterialTheme.typography.titleMedium)
    BodyText(stringResource(R.string.chatbackup_desc))
    s.chatsBackedUpMs?.let { BodyText(stringResource(R.string.chatbackup_last, DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(it)))) }
    OutlinedButton(onClick = { vm.backUpChatsNow() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.chatbackup_now)) }
    SwitchRow(stringResource(R.string.mediabackup_master), stringResource(R.string.mediabackup_master_sub), m.enabled) {
        vm.setMedia(if (it) m.copy(enabled = true, photos = true, voice = true, files = true, videos = false) else m.copy(enabled = false))
    }
    if (!m.enabled) {
        Callout(stringResource(R.string.mediabackup_off_note))
    } else {
        SwitchRow(stringResource(R.string.mediabackup_photos), null, m.photos) { vm.setMedia(m.copy(photos = it)) }
        SwitchRow(stringResource(R.string.mediabackup_videos), null, m.videos) { vm.setMedia(m.copy(videos = it)) }
        SwitchRow(stringResource(R.string.mediabackup_voice), null, m.voice) { vm.setMedia(m.copy(voice = it)) }
        SwitchRow(stringResource(R.string.mediabackup_files), null, m.files) { vm.setMedia(m.copy(files = it)) }
        SwitchRow(stringResource(R.string.mediabackup_mobile), stringResource(R.string.mediabackup_mobile_sub), m.mobileData) { vm.setMedia(m.copy(mobileData = it)) }
    }
    if (s.confirmMediaOff) {
        androidx.compose.material3.AlertDialog(
            onDismissRequest = vm::keepMedia,
            title = { Text(stringResource(R.string.mediabackup_off_title)) },
            text = { Text(stringResource(R.string.mediabackup_off_body)) },
            confirmButton = { TextButton(onClick = { vm.confirmMediaOff() }) { Text(stringResource(R.string.mediabackup_off_confirm), color = MaterialTheme.colorScheme.error) } },
            dismissButton = { TextButton(onClick = vm::keepMedia) { Text(stringResource(R.string.mediabackup_keep)) } },
        )
    }
}

@Composable
private fun SwitchRow(title: String, sub: String?, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
        androidx.compose.foundation.layout.Column(Modifier.weight(1f)) {
            Text(title)
            sub?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
        }
        androidx.compose.material3.Switch(checked = checked, onCheckedChange = onChange)
    }
}

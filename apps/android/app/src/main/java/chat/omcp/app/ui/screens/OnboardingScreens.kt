package chat.omcp.app.ui.screens

import kotlinx.coroutines.launch
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
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
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import chat.omcp.app.R
import chat.omcp.app.ui.components.BodyText
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.components.PlainPage
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel

/**
 * Onboarding per apps/REQUIREMENTS.md "Onboarding & recovery backup (v0.1.1)".
 * Plain layouts on purpose — design v0.1.2 will restyle these.
 */
@Composable
fun OnboardingFlow(
    vm: OnboardingViewModel,
    myQr: @Composable () -> Unit,
    onFindNearby: () -> Unit,
    secureWindow: (Boolean) -> Unit,
) {
    val s by vm.state.collectAsState()
    // Cloud I/O (Drive REST, consent) never runs on the main thread.
    val io = androidx.compose.runtime.rememberCoroutineScope()
    LaunchedEffect(Unit) { kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) { vm.detectBackup() } }
    // FLAG_SECURE while the phrase is on screen.
    LaunchedEffect(s.step) { secureWindow(s.step == OnboardingStep.WriteDown || s.step == OnboardingStep.Quiz) }

    when (s.step) {
        OnboardingStep.Welcome -> PlainPage(stringResource(R.string.app_name)) {
            BodyText(stringResource(R.string.onb_tagline))
            BodyText(stringResource(R.string.onb_welcome_body))
            Button(onClick = vm::getStarted, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_get_started)) }
            OutlinedButton(onClick = vm::openRestoreSource, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.restore_welcome_cta)) }
            TextButton(onClick = vm::usePhraseInstead) { Text(stringResource(R.string.onb_restore_phrase)) }
        }

        OnboardingStep.RestoreChecking -> RestoreCheckingScreen()
        OnboardingStep.RestoreDetect -> RestoreDetectScreen(s, vm, io)
        OnboardingStep.RestoreSource -> RestoreSourceScreen(s, vm, io)
        OnboardingStep.RestoreNoBackup -> RestoreNoBackupScreen(s, vm, io)
        OnboardingStep.RestorePassword -> RestorePasswordScreen(s, vm, io)
        OnboardingStep.RestoreProgress -> RestoreProgressScreen(s)
        OnboardingStep.RestoreSuccess -> RestoreSuccessScreen(s, vm)
        OnboardingStep.RestoreError -> RestoreErrorScreen(s, vm, io)

        OnboardingStep.RestorePhrase -> PlainPage(stringResource(R.string.onb_restore_title)) {
            BodyText(stringResource(R.string.onb_restore_body))
            OutlinedTextField(
                s.restorePhrase, vm::setRestorePhrase,
                label = { Text(stringResource(R.string.onb_24_words)) },
                keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, capitalization = KeyboardCapitalization.None),
                minLines = 4,
                modifier = Modifier.fillMaxWidth(),
            )
            s.error?.let { Callout(it, CalloutKind.Danger) }
            Button(onClick = { vm.restoreFromPhrase() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_restore)) }
            TextButton(onClick = vm::backToWelcome) { Text(stringResource(R.string.back)) }
        }

        OnboardingStep.Profile -> PlainPage(stringResource(R.string.onb_profile_title)) {
            BodyText(stringResource(R.string.onb_profile_body))
            OutlinedTextField(
                s.displayName, vm::setName,
                label = { Text(stringResource(R.string.onb_display_name)) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            // TODO(v0.1.2 design): optional avatar picker.
            s.error?.let { Callout(it, CalloutKind.Danger) }
            Button(onClick = vm::profileNext, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.cta_continue)) }
        }

        OnboardingStep.Secure -> PlainPage(stringResource(R.string.onb_secure_title)) {
            BodyText(stringResource(R.string.onb_secure_body))
            // Google Drive (Android default)
            Text(stringResource(R.string.backup_drive), style = androidx.compose.material3.MaterialTheme.typography.titleMedium)
            if (s.driveDone) {
                Callout(stringResource(R.string.backup_drive_done), CalloutKind.Ok)
            } else if (!s.driveAvailable) {
                Callout(stringResource(R.string.backup_drive_unavailable), CalloutKind.Warn)
            } else {
                BodyText(stringResource(R.string.backup_honest_copy))
                OutlinedTextField(
                    s.drivePassword, vm::setDrivePassword,
                    label = { Text(stringResource(R.string.onb_backup_password)) },
                    supportingText = {
                        Text(
                            when {
                                s.drivePassword.isEmpty() -> stringResource(R.string.backup_pw_rule)
                                s.drivePasswordTooShort -> stringResource(R.string.backup_pw_short)
                                !s.drivePasswordOk -> stringResource(R.string.backup_pw_weak)
                                else -> stringResource(R.string.backup_pw_ok)
                            },
                        )
                    },
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false),
                    modifier = Modifier.fillMaxWidth(),
                )
                Button(onClick = { io.launch(kotlinx.coroutines.Dispatchers.IO) { vm.backupToDrive() } }, enabled = s.drivePasswordOk, modifier = Modifier.fillMaxWidth()) {
                    Text(stringResource(R.string.backup_drive_cta))
                }
            }
            // Write it down
            Text(stringResource(R.string.backup_written), style = androidx.compose.material3.MaterialTheme.typography.titleMedium)
            if (s.writtenDone) {
                Callout(stringResource(R.string.backup_written_done), CalloutKind.Ok)
            } else {
                OutlinedButton(onClick = vm::writeItDown, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.backup_written_cta)) }
            }
            s.error?.let { Callout(it, CalloutKind.Danger) }
            Button(onClick = { vm.secureNext() }, enabled = s.anyBackupDone, modifier = Modifier.fillMaxWidth()) {
                Text(stringResource(R.string.cta_continue))
            }
            if (!s.anyBackupDone) TextButton(onClick = vm::remindLater) { Text(stringResource(R.string.backup_remind_later)) }
        }

        OnboardingStep.WriteDown -> PlainPage(stringResource(R.string.backup_written)) {
            BodyText(stringResource(R.string.onb_words_body))
            Callout(stringResource(R.string.onb_no_screenshot), CalloutKind.Warn)
            s.words.chunked(3).forEachIndexed { row, ws ->
                Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                    ws.forEachIndexed { i, w -> Text("${row * 3 + i + 1}. $w", fontFamily = FontFamily.Monospace) }
                }
            }
            Button(onClick = vm::startQuiz, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_wrote_it)) }
            TextButton(onClick = vm::backToSecure) { Text(stringResource(R.string.back)) }
        }

        OnboardingStep.Quiz -> PlainPage(stringResource(R.string.onb_quiz_title)) {
            BodyText(stringResource(R.string.onb_quiz_body))
            s.quiz.forEachIndexed { i, f ->
                OutlinedTextField(
                    f.input, { vm.setQuizInput(i, it) },
                    label = { Text(stringResource(R.string.onb_word_n, f.position + 1)) },
                    isError = f.error,
                    supportingText = if (f.error) ({ Text(stringResource(R.string.onb_word_wrong, f.position + 1)) }) else null,
                    trailingIcon = if (f.ok) ({ Text("✓") }) else null,
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(autoCorrectEnabled = false, capitalization = KeyboardCapitalization.None),
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            Button(onClick = { vm.submitQuiz() }, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_confirm)) }
            TextButton(onClick = vm::writeItDown) { Text(stringResource(R.string.onb_show_again)) }
        }

        OnboardingStep.Done -> PlainPage(stringResource(R.string.onb_done_title)) {
            BodyText(stringResource(R.string.onb_done_body))
            myQr()
            OutlinedButton(onClick = onFindNearby, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_find_nearby)) }
            Button(onClick = vm::finish, modifier = Modifier.fillMaxWidth()) { Text(stringResource(R.string.onb_start_chatting)) }
        }
    }
}

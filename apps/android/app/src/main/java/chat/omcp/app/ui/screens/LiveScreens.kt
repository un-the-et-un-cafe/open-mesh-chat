package chat.omcp.app.ui.screens

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import chat.omcp.app.R
import chat.omcp.app.core.Contact
import chat.omcp.app.core.LiveStreamUi
import chat.omcp.app.ui.components.Callout
import chat.omcp.app.ui.components.CalloutKind
import chat.omcp.app.ui.theme.LocalOmcpColors
import chat.omcp.app.ui.vm.LiveViewModel
import chat.omcp.app.live.VideoState
import chat.omcp.app.live.WebRtcPeers
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.viewinterop.AndroidView

/** Live tab: who's live, listen, or go live (audio over the mesh; REQUIREMENTS "Live"). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LiveTabScreen(vm: LiveViewModel, contacts: List<Contact>, rtc: WebRtcPeers?, hasInternet: () -> Boolean) {
    val c = LocalOmcpColors.current
    val ctx = LocalContext.current
    val s by vm.state.collectAsState()
    var title by remember { mutableStateOf("") }
    var picked by remember { mutableStateOf(setOf<String>()) }
    var micDenied by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        while (true) { vm.refresh(); kotlinx.coroutines.delay(1_000) }
    }
    val go = { vm.goLive(title, picked.toList(), hasInternet()) }
    // Microphone: asked the first time the user goes live, never at start-up.
    val mic = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { res ->
        val ok = res[Manifest.permission.RECORD_AUDIO] ?: (ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED)
        micDenied = !ok
        if (ok) go() // without CAMERA the host still goes live, audio only
    }
    val online = hasInternet()
    val wantCamera = online && rtc != null
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.tab_live)) }) }) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item {
                Callout(
                    stringResource(if (online) R.string.live_online_callout else R.string.live_offline_callout),
                    if (online) CalloutKind.Info else CalloutKind.Warn,
                    Modifier.padding(horizontal = 16.dp),
                )
            }
            s.hosting?.let { h ->
                item {
                    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                        Column(Modifier.padding(16.dp)) {
                            LiveBadge()
                            Text(h.title, style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 6.dp))
                            if (rtc != null && s.video == VideoState.Hosting) VideoView(rtc)
                            Text(stringResource(R.string.live_you_are_live, s.stats.listeners), color = c.ink2,
                                modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                            Button(onClick = vm::stopLive, modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.live_stop)) }
                        }
                    }
                }
            }
            item {
                Text(stringResource(R.string.live_happening), style = MaterialTheme.typography.titleSmall,
                    modifier = Modifier.padding(horizontal = 16.dp))
            }
            if (s.streams.isEmpty()) {
                item { Text(stringResource(R.string.live_empty), color = c.ink3, modifier = Modifier.padding(horizontal = 16.dp)) }
            }
            items(s.streams, key = { it.streamIdHex }) { st ->
                StreamCard(st, contacts, s.hosting == null, onListen = { vm.listen(st.streamIdHex, online) }, onLeave = vm::leave,
                    lossPct = if (st.listening) LiveViewModel.lossPercent(s.stats) else null,
                    video = if (st.listening) s.video else VideoState.Off, rtc = rtc, online = online)
            }
            if (s.hosting == null) {
                item {
                    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
                        Column(Modifier.padding(16.dp)) {
                            Text(stringResource(R.string.live_start_title), style = MaterialTheme.typography.titleMedium)
                            Text(stringResource(R.string.live_start_body), color = c.ink2, style = MaterialTheme.typography.bodySmall)
                            OutlinedTextField(title, { title = it.take(120) }, Modifier.fillMaxWidth().padding(top = 8.dp),
                                label = { Text(stringResource(R.string.live_title_hint)) }, singleLine = true)
                            Text(stringResource(R.string.live_pick), style = MaterialTheme.typography.labelLarge, modifier = Modifier.padding(top = 8.dp))
                            contacts.forEach { ct ->
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Checkbox(ct.id in picked, { on -> picked = if (on) picked + ct.id else picked - ct.id })
                                    Text(ct.displayName)
                                }
                            }
                            if (micDenied) Text(stringResource(R.string.live_mic_denied), color = c.danger)
                            Button(
                                onClick = {
                                    val granted = ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                                                    val camOk = !wantCamera || ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
                                    if (granted && camOk) go() else mic.launch(
                                        if (wantCamera) arrayOf(Manifest.permission.RECORD_AUDIO, Manifest.permission.CAMERA) else arrayOf(Manifest.permission.RECORD_AUDIO))
                                },
                                enabled = picked.isNotEmpty(),
                                modifier = Modifier.padding(top = 8.dp),
                            ) { Text(stringResource(R.string.live_go)) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LiveBadge() {
    val c = LocalOmcpColors.current
    Text(stringResource(R.string.live_badge), color = c.danger, style = MaterialTheme.typography.labelMedium)
}

@Composable
private fun StreamCard(
    st: LiveStreamUi,
    contacts: List<Contact>,
    canListen: Boolean,
    onListen: () -> Unit,
    onLeave: () -> Unit,
    lossPct: Int?,
    video: VideoState,
    rtc: WebRtcPeers?,
    online: Boolean,
) {
    val c = LocalOmcpColors.current
    val host = contacts.firstOrNull { it.id == st.hostContactId }?.displayName ?: ("omcp1:" + st.hostContactId.take(4) + "…")
    Card(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                LiveBadge()
                Spacer(Modifier.width(8.dp))
                AssistChip(onClick = {}, label = {
                    Text(stringResource(if (st.video && online && rtc != null) R.string.live_video_badge else R.string.live_audio_badge))
                })
            }
            Text(st.title, style = MaterialTheme.typography.titleMedium)
            Text(host, color = c.ink2)
            if (st.listening) {
                if (rtc != null && video == VideoState.Watching) VideoView(rtc)
                videoMessage(video)?.let { m ->
                    Text(stringResource(m), color = c.ink2, modifier = Modifier.padding(top = 4.dp).semantics { liveRegion = LiveRegionMode.Polite })
                }
                Text(stringResource(R.string.live_listening, lossPct ?: 0), color = c.ink2,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                OutlinedButton(onClick = onLeave, modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.live_leave)) }
            } else {
                Button(onClick = onListen, enabled = canListen, modifier = Modifier.padding(top = 8.dp)) { Text(stringResource(R.string.live_join)) }
            }
        }
    }
}

/** User-facing video status (null = nothing to say). */
fun videoMessage(v: VideoState): Int? = when (v) {
    VideoState.Connecting -> R.string.live_video_connecting
    VideoState.NoDirectPath -> R.string.live_video_no_path
    VideoState.NoSlot -> R.string.live_video_full
    else -> null
}

@Composable
private fun VideoView(rtc: WebRtcPeers) {
    val view = remember { mutableStateOf<org.webrtc.SurfaceViewRenderer?>(null) }
    AndroidView(
        factory = { ctx ->
            org.webrtc.SurfaceViewRenderer(ctx).also {
                it.init(rtc.egl.eglBaseContext, null)
                it.setScalingType(org.webrtc.RendererCommon.ScalingType.SCALE_ASPECT_FIT)
                rtc.setSink(it)
                view.value = it
            }
        },
        modifier = Modifier.fillMaxWidth().aspectRatio(4f / 3f).padding(top = 8.dp),
    )
    DisposableEffect(Unit) {
        onDispose { rtc.setSink(null); view.value?.release() }
    }
}

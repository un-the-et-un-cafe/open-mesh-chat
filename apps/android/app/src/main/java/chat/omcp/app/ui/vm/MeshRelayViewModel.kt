package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.MeshRelayPrefs
import chat.omcp.app.core.MeshRelayStats
import chat.omcp.app.core.OmcpCore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Settings > Mesh relay (PROTOCOL §22.4): carry other people's end-to-end encrypted
 * messages, and how much space that may use. Defaults and limits live in Rust.
 */
class MeshRelayViewModel(private val core: OmcpCore) : ViewModel() {
    data class State(val prefs: MeshRelayPrefs, val stats: MeshRelayStats)

    private val _state = MutableStateFlow(read())
    val state: StateFlow<State> = _state.asStateFlow()

    private fun read() = State(
        runCatching { core.meshSettings() }.getOrDefault(MeshRelayPrefs()),
        runCatching { core.meshStats() }.getOrDefault(MeshRelayStats()),
    )

    fun setEnabled(on: Boolean) = apply(_state.value.prefs.copy(enabled = on))
    fun setCap(bytes: Long) = apply(_state.value.prefs.copy(capBytes = bytes))
    fun refresh() { _state.value = read() }

    private fun apply(p: MeshRelayPrefs) {
        runCatching { core.setMeshSettings(p) }
        _state.value = read()
    }

    companion object {
        /** Placeholder presets (spec gap: no design for this screen yet); 50 MB is the §22.4 default. */
        val CAP_OPTIONS = listOf(10_000_000L, 50_000_000L, 100_000_000L, 250_000_000L, 500_000_000L)
    }
}

package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.MeshRelayStats
import chat.omcp.app.core.OmcpCore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Settings > Mesh relay (REQUIREMENTS "Relay", RL1–RL4). The relay is always on and its
 * 500 MB cap is automatic, so there is nothing to set: the screen shows only
 * "Storage used for relaying: X MB" (RL4). Numbers come from Rust.
 */
class MeshRelayViewModel(private val core: OmcpCore) : ViewModel() {
    private val _state = MutableStateFlow(read())
    val state: StateFlow<MeshRelayStats> = _state.asStateFlow()

    private fun read() = runCatching { core.meshStats() }.getOrDefault(MeshRelayStats())

    fun refresh() { _state.value = read() }

    companion object {
        /** The "X" in "X MB" (decimal MB, one decimal under 10 MB). */
        fun formatMb(bytes: Long): String {
            val mb = bytes.coerceAtLeast(0) / 1e6
            return when {
                bytes <= 0 -> "0"
                mb < 10 -> "%.1f".format(java.util.Locale.ROOT, mb)
                else -> Math.round(mb).toString()
            }
        }
    }
}

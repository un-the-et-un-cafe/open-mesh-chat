package chat.omcp.app.ui.theme

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** Persists the Appearance choice (System default / Light / Dark). */
class ThemePrefs(context: Context) {
    private val prefs = context.getSharedPreferences("omcp-ui", Context.MODE_PRIVATE)
    private val _mode = MutableStateFlow(ThemeMode.parse(prefs.getString(KEY, null)))
    val mode: StateFlow<ThemeMode> = _mode.asStateFlow()

    fun set(m: ThemeMode) {
        prefs.edit().putString(KEY, m.name).apply()
        _mode.value = m
    }

    private companion object { const val KEY = "theme_mode" }
}

package chat.omcp.app.ui.vm

import androidx.lifecycle.ViewModel
import chat.omcp.app.core.NetworkType
import chat.omcp.app.core.NewsStoryUi
import chat.omcp.app.data.MeshRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** News tab chips (design §5.108). */
val NEWS_TABS = listOf("top", "local", "world", "politics", "tech", "sport", "health", "emergencies")

data class NewsUiState(
    val tab: String = "top",
    val stories: List<NewsStoryUi> = emptyList(),
    val meteredWarning: Boolean = false,
    val fetching: Boolean = false,
    /** News settings (§5.115). Personalisation stays on this phone. */
    val topics: Set<String> = emptySet(),
    val languages: Set<String> = setOf("en"),
    val city: String = "",
    val muted: Set<String> = emptySet(),
    /** §5.111: on by default, user can turn it off. */
    val emergencyAlerts: Boolean = true,
    val loaded: Boolean = false,
)

/** §5.108 ordering: pinned emergency alerts first, then newest; muted topics hidden (alerts never). */
fun arrangeNews(stories: List<NewsStoryUi>, muted: Set<String>): List<NewsStoryUi> =
    stories.filter { it.emergency || it.topic.lowercase() !in muted }
        .sortedWith(compareByDescending<NewsStoryUi> { it.emergency }.thenByDescending { it.publishedMs })

/**
 * Hopla News (REQUIREMENTS "Hopla News", v0.4). Read-only: there is no publish API.
 * Fetching happens only on gateway phones (Settings > Gateway on) and only when the
 * gateway rules allow it (Wi-Fi-only by default, metered warns, 500 MB/day cap — decided in Rust).
 * Everyone else receives stories over the mesh.
 */
class NewsViewModel(
    private val repo: MeshRepository,
    private val http: (String) -> String? = ::httpGet,
) : ViewModel() {
    private val io = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _state = MutableStateFlow(NewsUiState())
    val state: StateFlow<NewsUiState> = _state.asStateFlow()

    init { refresh() }

    fun select(tab: String) { _state.value = _state.value.copy(tab = tab); refresh() }

    fun refresh() {
        val st = _state.value
        val raw = runCatching { repo.core.newsFeed(st.tab) }.getOrDefault(emptyList())
        _state.value = st.copy(stories = arrangeNews(raw, st.muted), loaded = true)
    }

    /** Long-press shortcut and the single-tap "Mute a topic" button (WCAG 2.5.7) both land here. */
    fun mute(topic: String) { if (topic != "emergencies") { _state.value = _state.value.copy(muted = _state.value.muted + topic); refresh() } }
    fun unmute(topic: String) { _state.value = _state.value.copy(muted = _state.value.muted - topic); refresh() }
    fun toggleTopic(topic: String) = _state.value.let { _state.value = it.copy(topics = if (topic in it.topics) it.topics - topic else it.topics + topic) }
    fun toggleLanguage(l: String) = _state.value.let { if (l in it.languages && it.languages.size == 1) return@let; _state.value = it.copy(languages = if (l in it.languages) it.languages - l else it.languages + l) }
    fun setCity(c: String) { _state.value = _state.value.copy(city = c.take(60)) }
    fun setEmergencyAlerts(on: Boolean) { _state.value = _state.value.copy(emergencyAlerts = on) }

    fun story(id: String) = _state.value.stories.firstOrNull { it.idHex == id }

    /** Share as a news card to a chat (NW6); replies form its thread. */
    fun shareToChat(contactId: String, story: NewsStoryUi) = repo.send(contactId, repo.core.newsCardText(story))

    /** Fetch due feeds now if this phone is a gateway and the network allows it. Returns stories added. */
    fun fetchNow(network: NetworkType, metered: Boolean, nowMs: Long = System.currentTimeMillis(), sync: Boolean = false): Int {
        if (!repo.appStore.gatewayEnabled) return 0
        val plan = repo.core.newsFetchPlan(!repo.appStore.gatewayAllowMobile, network, metered, nowMs)
        _state.value = _state.value.copy(meteredWarning = plan.meteredWarning)
        if (plan.feeds.isEmpty()) return 0
        var added = 0
        val work = {
            for (url in plan.feeds) {
                val body = http(url) ?: continue
                added += runCatching { repo.core.newsIngestRss(url, body, nowMs) }.getOrDefault(0)
            }
            refresh()
        }
        if (sync) work() else io.launch { work() }
        return added
    }

    override fun onCleared() { io.cancel() }

    companion object {
        /** Plain HTTPS GET of a public Google News RSS feed (no API key), capped at 1 MB. */
        fun httpGet(url: String): String? = runCatching {
            val c = java.net.URL(url).openConnection() as javax.net.ssl.HttpsURLConnection
            c.connectTimeout = 15_000; c.readTimeout = 15_000
            c.inputStream.use { s -> String(s.readNBytes(1_000_000), Charsets.UTF_8) }
        }.getOrNull()
    }
}

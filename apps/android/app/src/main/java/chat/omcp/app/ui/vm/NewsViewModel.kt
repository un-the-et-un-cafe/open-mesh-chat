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

/** Topic codes the Rust core subscribes to (News settings › Topics). */
val NEWS_PREF_TOPICS = listOf("world", "politics", "tech", "sport", "health", "emergencies")

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
    /** Why the list may be empty (shown on the empty News screen). */
    val fetch: NewsFetchState = NewsFetchState.Idle,
    /** Last successful fetch on this phone (0 = never). */
    val lastFetchMs: Long = 0,
)

/** Fetch state for the empty News screen. */
sealed class NewsFetchState {
    data object Idle : NewsFetchState()
    data object Fetching : NewsFetchState()
    data object Ok : NewsFetchState()
    data object WaitingWifi : NewsFetchState()
    data object Offline : NewsFetchState()
    data object DailyCap : NewsFetchState()
    data class Failed(val reason: String) : NewsFetchState()

    companion object {
        fun fromBlocked(b: String?): NewsFetchState? = when (b) {
            null -> null
            "wifi_only" -> WaitingWifi
            "daily_cap" -> DailyCap
            else -> Offline
        }
    }
}

/** §5.108 ordering: pinned emergency alerts first, then newest; muted topics hidden (alerts never). */
fun arrangeNews(stories: List<NewsStoryUi>, muted: Set<String>): List<NewsStoryUi> =
    stories.filter { it.emergency || it.topic.lowercase() !in muted }
        .sortedWith(compareByDescending<NewsStoryUi> { it.emergency }.thenByDescending { it.publishedMs })

/**
 * Hopla News (REQUIREMENTS "Hopla News", v0.4). Read-only: there is no publish API.
 * Any phone with internet fetches (Rust flag `NEWS_FETCH_GATEWAY_ONLY` = false), but only when
 * the network rules allow it (Wi-Fi-only by default, metered warns, 500 MB/day cap — decided in Rust).
 * Everyone else receives stories over the mesh.
 */
class NewsViewModel(
    private val repo: MeshRepository,
    private val http: (String) -> chat.omcp.app.data.FetchResult = chat.omcp.app.data.NewsHttp::get,
    /** Current network (type, metered) for settings-triggered fetches; null = don't fetch. */
    private val network: (() -> Pair<NetworkType, Boolean>)? = null,
    private val clock: () -> Long = System::currentTimeMillis,
    /** Device language, for the first-run feed language. */
    deviceLang: String = java.util.Locale.getDefault().language,
) : ViewModel() {
    private val io = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _state = MutableStateFlow(NewsUiState())
    val state: StateFlow<NewsUiState> = _state.asStateFlow()

    init {
        val p = runCatching { repo.core.newsPrefs() }.getOrDefault(chat.omcp.app.core.NewsPrefsUi())
        _state.value = _state.value.copy(
            topics = p.topics.toSet(), languages = p.langs.toSet().ifEmpty { setOf("en") }, city = p.city.orEmpty(),
            muted = p.muted.toSet(), emergencyAlerts = p.alertsNotify,
            lastFetchMs = runCatching { repo.appStore.newsLastFetchMs }.getOrDefault(0L),
        )
        // First run: follow the phone's language (French phone → hl=fr&gl=FR&ceid=FR:fr feeds).
        if (!p.interestsChosen && deviceLang in setOf("fr", "he") && _state.value.languages == setOf("en")) {
            _state.value = _state.value.copy(languages = setOf(deviceLang)); persist(chosen = false)
        }
        refresh()
    }

    /** Write the settings through to the Rust core (encrypted store), so they survive restarts. */
    private fun persist(chosen: Boolean = true) {
        val st = _state.value
        runCatching {
            repo.core.newsSetPrefs(chat.omcp.app.core.NewsPrefsUi(
                topics = (st.topics.filter { it in NEWS_PREF_TOPICS }.ifEmpty { listOf("world") } + "emergencies").distinct(),
                langs = st.languages.toList(), city = st.city.ifBlank { null }, muted = st.muted.toList(),
                alertsNotify = st.emergencyAlerts, interestsChosen = chosen,
            ))
        }
    }

    fun select(tab: String) { _state.value = _state.value.copy(tab = tab); refresh() }

    fun refresh() {
        val st = _state.value
        val raw = runCatching { repo.core.newsFeed(st.tab) }.getOrDefault(emptyList())
        _state.value = st.copy(stories = arrangeNews(raw, st.muted), loaded = true)
    }

    /** Long-press shortcut and the single-tap "Mute a topic" button (WCAG 2.5.7) both land here. */
    fun mute(topic: String) { if (topic != "emergencies") { _state.value = _state.value.copy(muted = _state.value.muted + topic); persist(); refresh() } }
    fun unmute(topic: String) { _state.value = _state.value.copy(muted = _state.value.muted - topic); persist(); refresh() }
    fun toggleTopic(topic: String) { _state.value.let { _state.value = it.copy(topics = if (topic in it.topics) it.topics - topic else it.topics + topic) }; persist(); fetchForSettings() }
    fun toggleLanguage(l: String) {
        val it = _state.value
        if (l in it.languages && it.languages.size == 1) return
        _state.value = it.copy(languages = if (l in it.languages) it.languages - l else it.languages + l); persist(); fetchForSettings()
    }
    fun setCity(c: String) { _state.value = _state.value.copy(city = c.take(60)); persist(); refresh(); fetchForSettings() }

    /** New feeds are due right away after a settings change (Rust only returns feeds not fetched yet). */
    private fun fetchForSettings() { network?.invoke()?.let { (t, m) -> fetchNow(t, m) } }

    /** Pull-to-refresh / "Fetch now". */
    fun fetchNowFromUi() { network?.invoke()?.let { (t, m) -> fetchNow(t, m) } }

    /** "Allow mobile data" on the waiting-for-Wi-Fi screen (metered networks still warn). */
    fun allowMobileData() { repo.appStore.gatewayAllowMobile = true; fetchNowFromUi() }
    fun setEmergencyAlerts(on: Boolean) { _state.value = _state.value.copy(emergencyAlerts = on); persist() }

    fun story(id: String) = _state.value.stories.firstOrNull { it.idHex == id }

    /** Share as a news card to a chat (NW6); replies form its thread. */
    fun shareToChat(contactId: String, story: NewsStoryUi) = repo.send(contactId, repo.core.newsCardText(story))

    /** Fetch due feeds now if the network rules allow it. Returns stories added (sync mode). */
    fun fetchNow(network: NetworkType, metered: Boolean, nowMs: Long = clock(), sync: Boolean = false): Int {
        val net = network.name.lowercase() + if (metered) " (metered)" else ""
        fun diag(r: String) { chat.omcp.app.diag.Diag.news = chat.omcp.app.diag.Diag.News(nowMs, net, r) }
        if (!repo.core.newsMayFetch(repo.appStore.gatewayEnabled)) { diag("disabled (gateway only)"); return 0 }
        val plan = repo.core.newsFetchPlan(!repo.appStore.gatewayAllowMobile, network, metered, nowMs)
        _state.value = _state.value.copy(meteredWarning = plan.meteredWarning)
        NewsFetchState.fromBlocked(plan.blocked)?.let { diag("blocked: ${plan.blocked}"); _state.value = _state.value.copy(fetch = it); return 0 }
        if (plan.feeds.isEmpty()) {
            diag("nothing due (fetched or tried < 30 min ago)")
            // Nothing due (fetched < 30 min ago): keep the last result, but clear a stale block.
            if (_state.value.fetch !is NewsFetchState.Failed) _state.value = _state.value.copy(fetch = NewsFetchState.Ok)
            return 0
        }
        var added = 0
        val work = {
            _state.value = _state.value.copy(fetching = true, fetch = NewsFetchState.Fetching)
            var ok = 0
            var lastErr: String? = null
            for (url in plan.feeds) {
                when (val r = http(url)) {
                    is chat.omcp.app.data.FetchResult.Err -> lastErr = r.reason
                    is chat.omcp.app.data.FetchResult.Ok -> runCatching { repo.core.newsIngestRss(url, r.body, nowMs) }
                        .onSuccess { added += it; ok++ }.onFailure { lastErr = "not_rss"; chat.omcp.app.diag.Diag.log("news ingest failed: ${it.javaClass.simpleName}", true) }
                }
            }
            if (ok > 0) runCatching { repo.appStore.newsLastFetchMs = nowMs }
            diag(if (ok > 0) "ok: $ok/${plan.feeds.size} feeds, $added new" else "failed: ${lastErr ?: "?"} (${plan.feeds.size} feeds)")
            _state.value = _state.value.copy(
                fetching = false,
                fetch = if (ok > 0 || lastErr == null) NewsFetchState.Ok else NewsFetchState.Failed(lastErr!!),
                lastFetchMs = if (ok > 0) nowMs else _state.value.lastFetchMs,
            )
            refresh()
        }
        if (sync) work() else io.launch { work() }
        return added
    }

    override fun onCleared() { io.cancel() }

}

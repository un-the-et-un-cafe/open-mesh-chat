package chat.omcp.app

import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.core.NetworkType
import chat.omcp.app.core.NewsFetchPlanUi
import chat.omcp.app.core.NewsStoryUi
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.ui.vm.NEWS_TABS
import chat.omcp.app.ui.vm.NewsViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NewsViewModelTest {
    private class NewsCore : chat.omcp.app.core.OmcpCore by FakeOmcpCore() {
        val ingested = mutableListOf<String>()
        var plan = NewsFetchPlanUi(feeds = listOf("https://news.google.com/rss/x"), blocked = null)
        var lastWifiOnly: Boolean? = null
        override fun newsFetchPlan(wifiOnly: Boolean, network: NetworkType, metered: Boolean, nowMs: Long): NewsFetchPlanUi {
            lastWifiOnly = wifiOnly; return plan
        }
        override fun newsIngestRss(feedUrl: String, body: String, nowMs: Long): Int { ingested += feedUrl; return 2 }
        override fun newsFeed(tab: String) = listOf(NewsStoryUi("00", "world", "T", "", "P", "Google News", "https://news.google.com/a", 1, null, true, false))
    }

    @Test fun tabsMatchDesignAndNoPublishApi() {
        assertEquals(listOf("top", "local", "world", "politics", "tech", "sport", "health", "emergencies"), NEWS_TABS)
        assertTrue(NewsViewModel::class.java.methods.none { it.name.contains("publish", true) || it.name.contains("post", true) })
    }

    @Test fun onlyGatewayPhonesFetch() {
        val core = NewsCore(); val store = InMemoryAppStore()
        val vm = NewsViewModel(MeshRepository(core, store) { 0L }) { "<rss/>" }
        assertEquals(0, vm.fetchNow(NetworkType.Wifi, false, sync = true))
        assertTrue(core.ingested.isEmpty())
        store.gatewayEnabled = true
        assertEquals(2, vm.fetchNow(NetworkType.Wifi, false, sync = true))
        assertEquals(true, core.lastWifiOnly) // Wi-Fi-only by default
    }

    @Test fun meteredWarningSurfacesAndBlockedPlanFetchesNothing() {
        val core = NewsCore(); val store = InMemoryAppStore().apply { gatewayEnabled = true; gatewayAllowMobile = true }
        val vm = NewsViewModel(MeshRepository(core, store) { 0L }) { "<rss/>" }
        core.plan = NewsFetchPlanUi(feeds = listOf("https://news.google.com/rss/x"), meteredWarning = true, blocked = null)
        vm.fetchNow(NetworkType.Cellular, true, sync = true)
        assertTrue(vm.state.value.meteredWarning)
        assertEquals(false, core.lastWifiOnly)
        core.plan = NewsFetchPlanUi(blocked = "daily_cap")
        core.ingested.clear()
        assertEquals(0, vm.fetchNow(NetworkType.Wifi, false, sync = true))
        assertTrue(core.ingested.isEmpty())
    }
}

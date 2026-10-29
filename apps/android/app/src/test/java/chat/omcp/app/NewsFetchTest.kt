package chat.omcp.app

import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.core.NetworkType
import chat.omcp.app.core.NewsFetchPlanUi
import chat.omcp.app.core.NewsPrefsUi
import chat.omcp.app.data.FetchResult
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.data.NewsFetchWorker
import chat.omcp.app.data.NewsHttp
import chat.omcp.app.ui.screens.newsEmptyText
import chat.omcp.app.ui.screens.newsReasonText
import chat.omcp.app.ui.vm.NewsFetchState
import chat.omcp.app.ui.vm.NewsViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class NewsFetchTest {
    private val rss = """<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><item><title>x</title></item></channel></rss>"""
    private val consentHtml = """<!DOCTYPE html><html><head><title>Before you continue to Google</title></head><body><form action="https://consent.google.com/save"></form></body></html>"""
    private val sorry = """<html><head><meta http-equiv="content-type" content="text/html; charset=utf-8"/><title>Sorry...</title></head></html>"""

    @Test fun classifyAcceptsRssRejectsHtmlConsentAndErrors() {
        assertEquals(FetchResult.Ok(rss), NewsHttp.classify(200, "application/xml; charset=utf-8", "news.google.com", rss))
        assertEquals(FetchResult.Err("consent"), NewsHttp.classify(200, "text/html", "news.google.com", consentHtml))
        assertEquals(FetchResult.Err("consent"), NewsHttp.classify(200, "text/html", "consent.google.com", "<html/>"))
        assertEquals(FetchResult.Err("http_503"), NewsHttp.classify(503, "text/html", "news.google.com", sorry))
        assertEquals(FetchResult.Err("html"), NewsHttp.classify(200, "text/html; charset=UTF-8", "news.google.com", sorry))
        assertEquals(FetchResult.Err("html"), NewsHttp.classify(200, "application/xml", "news.google.com", "<!doctype html><p>"))
        assertEquals(FetchResult.Err("not_rss"), NewsHttp.classify(200, "application/json", "news.google.com", "{}"))
        assertTrue(NewsHttp.USER_AGENT.startsWith("Mozilla/5.0")) // bare Java/custom agents got 503 "Sorry"
        assertTrue(NewsHttp.ACCEPT.contains("application/rss+xml"))
    }

    private class Core : chat.omcp.app.core.OmcpCore by FakeOmcpCore() {
        var plan = NewsFetchPlanUi(feeds = listOf("https://news.google.com/rss/a", "https://news.google.com/rss/b"), blocked = null)
        var prefs = NewsPrefsUi()
        var plans = 0
        override fun newsFetchPlan(wifiOnly: Boolean, network: NetworkType, metered: Boolean, nowMs: Long): NewsFetchPlanUi { plans++; return plan }
        override fun newsIngestRss(feedUrl: String, body: String, nowMs: Long): Int = 3
        override fun newsPrefs() = prefs
        override fun newsSetPrefs(p: NewsPrefsUi): NewsPrefsUi { prefs = p; return p }
    }

    private fun vm(core: Core, store: InMemoryAppStore = InMemoryAppStore(), lang: String = "en", http: (String) -> FetchResult) =
        NewsViewModel(MeshRepository(core, store) { 0L }, http, network = { NetworkType.Wifi to false }, clock = { 1_000L }, deviceLang = lang)

    @Test fun blockedReasonsBecomeStates() {
        val core = Core()
        val v = vm(core) { FetchResult.Ok(rss) }
        for ((b, s) in listOf("wifi_only" to NewsFetchState.WaitingWifi, "offline" to NewsFetchState.Offline, "daily_cap" to NewsFetchState.DailyCap)) {
            core.plan = NewsFetchPlanUi(blocked = b)
            v.fetchNow(NetworkType.Cellular, true, sync = true)
            assertEquals(s, v.state.value.fetch)
            assertEquals(s.toString(), newsEmptyText(s).first != chat.omcp.app.R.string.news_empty_title, true)
        }
    }

    @Test fun failureKeepsReasonAndSuccessRecordsTime() {
        val core = Core(); val store = InMemoryAppStore()
        val bad = vm(core, store) { FetchResult.Err("consent") }
        assertEquals(0, bad.fetchNow(NetworkType.Wifi, false, sync = true))
        assertEquals(NewsFetchState.Failed("consent"), bad.state.value.fetch)
        assertEquals(chat.omcp.app.R.string.news_reason_consent, newsReasonText("consent").first)
        assertEquals("503", newsReasonText("http_503").second)
        assertEquals(0L, store.newsLastFetchMs)
        val good = vm(core, store) { u -> if (u.endsWith("a")) FetchResult.Err("http_503") else FetchResult.Ok(rss) }
        assertEquals(3, good.fetchNow(NetworkType.Wifi, false, sync = true))
        assertEquals(NewsFetchState.Ok, good.state.value.fetch)
        assertEquals(1_000L, store.newsLastFetchMs)
    }

    @Test fun allowMobileDataSetsTheFlag() {
        val core = Core(); val store = InMemoryAppStore()
        val v = vm(core, store) { FetchResult.Ok(rss) }
        v.allowMobileData()
        assertTrue(store.gatewayAllowMobile)
    }

    @Test fun settingsChangeTriggersFetch() {
        val core = Core()
        val v = vm(core) { FetchResult.Ok(rss) }
        val before = core.plans
        v.toggleTopic("tech"); v.setCity("Lille"); v.toggleLanguage("fr")
        assertEquals(before + 3, core.plans)
    }

    @Test fun frenchPhoneDefaultsToFrenchFeeds() {
        val core = Core()
        vm(core, lang = "fr") { FetchResult.Ok(rss) }
        assertEquals(listOf("fr"), core.prefs.langs)
        assertEquals(false, core.prefs.interestsChosen)
        val core2 = Core().apply { prefs = NewsPrefsUi(langs = listOf("en"), interestsChosen = true) }
        vm(core2, lang = "fr") { FetchResult.Ok(rss) }
        assertEquals(listOf("en"), core2.prefs.langs) // a choice the user made is kept
    }

    @Test fun workerFollowsTheSameRules() {
        val core = Core(); val store = InMemoryAppStore()
        val repo = MeshRepository(core, store) { 0L }
        core.plan = NewsFetchPlanUi(blocked = "wifi_only")
        assertEquals(0, NewsFetchWorker.run(repo, NetworkType.Cellular to true, 5L) { FetchResult.Ok(rss) })
        core.plan = NewsFetchPlanUi(feeds = listOf("u"), blocked = null)
        assertEquals(3, NewsFetchWorker.run(repo, NetworkType.Wifi to false, 5L) { FetchResult.Ok(rss) })
        assertEquals(5L, store.newsLastFetchMs)
    }
}

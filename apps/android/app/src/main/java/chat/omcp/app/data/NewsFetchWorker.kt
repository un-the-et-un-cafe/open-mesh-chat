package chat.omcp.app.data

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import chat.omcp.app.OmcpApplication
import chat.omcp.app.core.NetworkType
import java.util.concurrent.TimeUnit

/**
 * Background News fetch every 30 min (the Rust feed interval). The Rust plan still applies the
 * Wi-Fi-only default, the metered warning and the 500 MB/day cap, so this only provides a schedule.
 */
class NewsFetchWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result {
        val repo = (applicationContext as OmcpApplication).repository
        if (!repo.onboarded.value) return Result.success()
        runCatching { run(repo, NewsNet.current(applicationContext), System.currentTimeMillis(), NewsHttp::get) }
        return Result.success()
    }

    companion object {
        private const val WORK = "omcp-news-fetch"

        /** Same rules as [chat.omcp.app.ui.vm.NewsViewModel.fetchNow]. Returns stories added. */
        fun run(repo: MeshRepository, net: Pair<NetworkType, Boolean>, now: Long, http: (String) -> FetchResult): Int {
            val core = repo.core
            if (!core.newsMayFetch(repo.appStore.gatewayEnabled)) return 0
            val plan = core.newsFetchPlan(!repo.appStore.gatewayAllowMobile, net.first, net.second, now)
            if (plan.blocked != null) return 0
            var added = 0
            var ok = 0
            var err: String? = null
            for (url in plan.feeds) {
                when (val r = http(url)) {
                    is FetchResult.Err -> err = r.reason
                    is FetchResult.Ok -> runCatching { core.newsIngestRss(url, r.body, now) }
                        .onSuccess { added += it; ok++; repo.appStore.newsLastFetchMs = now }.onFailure { err = "not_rss" }
                }
            }
            if (plan.feeds.isNotEmpty()) chat.omcp.app.diag.Diag.news = chat.omcp.app.diag.Diag.News(now,
                "background " + net.first.name.lowercase(), if (ok > 0) "ok: $ok feeds, $added new" else "failed: ${err ?: "?"}")
            return added
        }

        fun schedule(ctx: Context) {
            val req = PeriodicWorkRequestBuilder<NewsFetchWorker>(30, TimeUnit.MINUTES)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(androidx.work.NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, req)
        }
    }
}

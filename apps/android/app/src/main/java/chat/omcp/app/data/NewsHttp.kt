package chat.omcp.app.data

/** Result of one Google News RSS GET. */
sealed class FetchResult {
    data class Ok(val body: String) : FetchResult()
    /** Short, user-facing reason code: http_503, consent, html, not_rss, network, too_big. */
    data class Err(val reason: String) : FetchResult()
}

/**
 * Plain HTTPS GET of public Google News RSS (no API key). Sends a browser-like User-Agent
 * (Google answers bare Java/custom agents with a 503 "Sorry" page) and an RSS Accept header,
 * then rejects anything that is not RSS: HTTP errors, the consent.google.com redirect, HTML.
 */
object NewsHttp {
    const val USER_AGENT = "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Mobile Safari/537.36 Hopla"
    const val ACCEPT = "application/rss+xml, application/xml;q=0.9, text/xml;q=0.8"
    private const val MAX = 1_000_000

    fun get(url: String): FetchResult = try { fetch(url) } catch (_: java.io.IOException) {
        FetchResult.Err("network")
    } catch (_: RuntimeException) {
        FetchResult.Err("network")
    }

    private fun fetch(url: String): FetchResult {
        var u = java.net.URL(url)
        var hops = 0
        while (true) {
            val c = u.openConnection() as javax.net.ssl.HttpsURLConnection
            c.instanceFollowRedirects = false
            c.connectTimeout = 15_000; c.readTimeout = 15_000
            c.setRequestProperty("User-Agent", USER_AGENT)
            c.setRequestProperty("Accept", ACCEPT)
            c.setRequestProperty("Accept-Language", acceptLanguage(url))
            val code = c.responseCode
            if (code in 300..399) {
                val loc = c.getHeaderField("Location") ?: return FetchResult.Err("http_$code")
                val next = java.net.URL(u, loc)
                c.disconnect()
                if (isConsent(next.host)) return FetchResult.Err("consent")
                if (next.protocol != "https" || ++hops > 5) return FetchResult.Err("redirect")
                u = next; continue
            }
            val body = if (code in 200..299) c.inputStream.use { String(it.readNBytes(MAX), Charsets.UTF_8) } else ""
            return classify(code, c.contentType, u.host, body)
        }
    }

    fun isConsent(host: String?) = host != null && (host == "consent.google.com" || host.startsWith("consent."))

    /** Pure check of a response (unit-tested). */
    fun classify(status: Int, contentType: String?, host: String?, body: String): FetchResult {
        if (isConsent(host)) return FetchResult.Err("consent")
        if (status !in 200..299) return FetchResult.Err("http_$status")
        val head = body.trimStart().take(512).lowercase()
        if (head.contains("consent.google") || head.contains("before you continue")) return FetchResult.Err("consent")
        if (head.startsWith("<!doctype html") || head.startsWith("<html")) return FetchResult.Err("html")
        val ct = contentType?.lowercase().orEmpty()
        if (ct.contains("text/html")) return FetchResult.Err("html")
        if (!head.contains("<rss") && !head.startsWith("<?xml")) return FetchResult.Err("not_rss")
        return FetchResult.Ok(body)
    }

    private fun acceptLanguage(url: String): String {
        val hl = Regex("[?&]hl=([A-Za-z-]+)").find(url)?.groupValues?.get(1) ?: "en-US"
        return "$hl,${hl.substringBefore('-')};q=0.9"
    }
}

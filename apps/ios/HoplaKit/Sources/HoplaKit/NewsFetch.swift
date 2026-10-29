import Foundation

/// Outcome of one Google News RSS GET (same rules as Android `NewsHttp.classify`).
public enum NewsHttpResult: Equatable, Sendable {
    case ok(String)
    /// consent, html, not_rss, network, http_<code>
    case err(String)
}

/// Why the News list may be empty.
public enum NewsFetchStatus: Equatable, Sendable {
    case idle, fetching, ok, waitingWifi, offline, dailyCap
    case failed(String)

    public static func from(blocked: String?) -> NewsFetchStatus? {
        switch blocked {
        case nil: return nil
        case "wifi_only": return .waitingWifi
        case "daily_cap": return .dailyCap
        default: return .offline
        }
    }

    /// (title key, body key) for the empty screen.
    public var textKeys: (String, String) {
        switch self {
        case .waitingWifi: return ("news_state_wifi_t", "news_state_wifi_d")
        case .offline: return ("news_state_offline_t", "news_state_offline_d")
        case .dailyCap: return ("news_state_cap_t", "news_state_cap_d")
        case .fetching: return ("news_state_fetching_t", "news_empty_body")
        case .failed: return ("news_state_failed_t", "news_state_failed_d")
        default: return ("news_empty_title", "news_empty_body")
        }
    }

    public static func reasonKey(_ r: String) -> String {
        switch r {
        case "consent": return "news_reason_consent"
        case "html", "not_rss": return "news_reason_not_rss"
        default: return r.hasPrefix("http_") ? "news_reason_http" : "news_reason_network"
        }
    }
}

public enum NewsHttp {
    /// Bare/custom agents got a 503 "Sorry" page from Google News in testing; send a browser-like UA.
    public static let userAgent = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile Hopla"
    public static let accept = "application/rss+xml, application/xml;q=0.9, text/xml;q=0.8"

    public static func isConsent(_ host: String?) -> Bool {
        guard let h = host else { return false }
        return h == "consent.google.com" || h.hasPrefix("consent.")
    }

    public static func classify(status: Int, contentType: String?, host: String?, body: String) -> NewsHttpResult {
        if isConsent(host) { return .err("consent") }
        guard (200...299).contains(status) else { return .err("http_\(status)") }
        let head = String(body.drop(while: { $0.isWhitespace }).prefix(512)).lowercased()
        if head.contains("consent.google") || head.contains("before you continue") { return .err("consent") }
        if head.hasPrefix("<!doctype html") || head.hasPrefix("<html") { return .err("html") }
        if (contentType ?? "").lowercased().contains("text/html") { return .err("html") }
        if !head.contains("<rss") && !head.hasPrefix("<?xml") { return .err("not_rss") }
        return .ok(body)
    }

    public static func request(_ url: URL) -> URLRequest {
        var r = URLRequest(url: url, timeoutInterval: 15)
        r.setValue(userAgent, forHTTPHeaderField: "User-Agent")
        r.setValue(accept, forHTTPHeaderField: "Accept")
        return r
    }
}

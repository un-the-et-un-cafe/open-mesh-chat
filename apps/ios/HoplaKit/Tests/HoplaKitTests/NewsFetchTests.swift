import XCTest
@testable import HoplaKit

final class NewsFetchTests: XCTestCase {
    let rss = "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel></channel></rss>"

    func testClassify() {
        XCTAssertEqual(NewsHttp.classify(status: 200, contentType: "application/xml", host: "news.google.com", body: rss), .ok(rss))
        XCTAssertEqual(NewsHttp.classify(status: 200, contentType: "text/html", host: "consent.google.com", body: "<html>"), .err("consent"))
        XCTAssertEqual(NewsHttp.classify(status: 200, contentType: "text/html", host: "news.google.com",
                                         body: "<!DOCTYPE html><title>Before you continue to Google</title>"), .err("consent"))
        XCTAssertEqual(NewsHttp.classify(status: 503, contentType: "text/html", host: "news.google.com", body: "<html>Sorry"), .err("http_503"))
        XCTAssertEqual(NewsHttp.classify(status: 200, contentType: "text/html", host: "news.google.com", body: "<html><title>Sorry"), .err("html"))
        XCTAssertEqual(NewsHttp.classify(status: 200, contentType: "application/json", host: "news.google.com", body: "{}"), .err("not_rss"))
        XCTAssertTrue(NewsHttp.userAgent.hasPrefix("Mozilla/5.0"))
    }

    func testStates() {
        XCTAssertEqual(NewsFetchStatus.from(blocked: "wifi_only"), .waitingWifi)
        XCTAssertEqual(NewsFetchStatus.from(blocked: "daily_cap"), .dailyCap)
        XCTAssertEqual(NewsFetchStatus.from(blocked: "offline"), .offline)
        XCTAssertNil(NewsFetchStatus.from(blocked: nil))
        XCTAssertEqual(NewsFetchStatus.waitingWifi.textKeys.0, "news_state_wifi_t")
        XCTAssertEqual(NewsFetchStatus.failed("x").textKeys.0, "news_state_failed_t")
        XCTAssertEqual(NewsFetchStatus.reasonKey("http_503"), "news_reason_http")
        XCTAssertEqual(NewsFetchStatus.reasonKey("consent"), "news_reason_consent")
    }
}

//! Minimal RSS 2.0 reader for Google News feeds (REQUIREMENTS § Hopla News).
//!
//! Only what the news ingest needs: `<item>` blocks with `title`, `link`,
//! `pubDate`, `description` and `source`. No DTDs, no external entities, no
//! network: the caller hands in the feed body it fetched. Input is bounded
//! ([`MAX_FEED_BYTES`], [`MAX_ITEMS`]) so a hostile feed can't make it work
//! hard.

use crate::error::{OmcpError, Result};

/// Largest feed body accepted (Google News feeds are ~50–150 KB).
pub const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
/// Items read from one feed at most.
pub const MAX_ITEMS: usize = 100;

/// One `<item>` as found in the feed (text already unescaped).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RssItem {
    pub title: String,
    pub link: String,
    /// `pubDate` in unix ms, if present and parseable.
    pub published_ms: Option<u64>,
    /// `description` with markup removed.
    pub description: String,
    /// `<source>` text (the publisher's name), if any.
    pub source: String,
}

/// Parse an RSS 2.0 document into its items (document order).
pub fn parse_rss(xml: &str) -> Result<Vec<RssItem>> {
    if xml.len() > MAX_FEED_BYTES {
        return Err(OmcpError::Sync("news feed too large"));
    }
    if !xml.contains("<rss") && !xml.contains("<channel") {
        return Err(OmcpError::Sync("not an RSS feed"));
    }
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = find_open(rest, "item") {
        let after = &rest[start..];
        let Some(gt) = after.find('>') else { break };
        let body_start = start + gt + 1;
        let Some(end_rel) = rest[body_start..].find("</item>") else {
            break;
        };
        let body = &rest[body_start..body_start + end_rel];
        out.push(RssItem {
            title: element_text(body, "title")
                .map(clean_text)
                .unwrap_or_default(),
            link: element_text(body, "link")
                .map(|s| unescape(s.trim()))
                .unwrap_or_default(),
            published_ms: element_text(body, "pubDate").and_then(|d| parse_rfc822(&unescape(d))),
            description: element_text(body, "description")
                .map(|d| clean_text(&strip_tags(&unescape(d))))
                .unwrap_or_default(),
            source: element_text(body, "source")
                .map(clean_text)
                .unwrap_or_default(),
        });
        if out.len() >= MAX_ITEMS {
            break;
        }
        rest = &rest[body_start + end_rel + "</item>".len()..];
    }
    Ok(out)
}

/// Offset of `<name>` or `<name …>` (not `<namefoo>`).
fn find_open(s: &str, name: &str) -> Option<usize> {
    let pat = format!("<{name}");
    let mut from = 0;
    while let Some(i) = s[from..].find(&pat) {
        let at = from + i;
        match s.as_bytes().get(at + pat.len()) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') | Some(b'/') => {
                return Some(at)
            }
            _ => from = at + pat.len(),
        }
    }
    None
}

/// Raw inner text of the first `<name>` child (CDATA unwrapped, not unescaped).
fn element_text<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let start = find_open(body, name)?;
    let gt = body[start..].find('>')? + start;
    if body.as_bytes()[gt - 1] == b'/' {
        return Some("");
    }
    let close = format!("</{name}>");
    let end = body[gt + 1..].find(&close)? + gt + 1;
    let inner = body[gt + 1..end].trim();
    Some(
        inner
            .strip_prefix("<![CDATA[")
            .and_then(|s| s.strip_suffix("]]>"))
            .unwrap_or(inner),
    )
}

/// Unescape, drop markup, collapse whitespace.
fn clean_text(s: &str) -> String {
    let u = unescape(s);
    let stripped = if u.contains('<') { strip_tags(&u) } else { u };
    collapse(&unescape(&stripped))
}

fn collapse(s: &str) -> String {
    s.split(|c: char| c.is_whitespace() || c == '\u{a0}')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Remove `<…>` markup (description HTML), keeping the text between tags.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// XML/HTML entity unescape: the five XML entities, `&nbsp;` and numeric
/// references. Unknown entities are kept as written.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail[..tail.len().min(12)].find(';');
        let rep = end.and_then(|e| {
            let ent = &tail[1..e];
            let c = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    u32::from_str_radix(&ent[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, e + 1))
        });
        match rep {
            Some((c, n)) => {
                out.push(c);
                rest = &tail[n..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// RFC 822 / RFC 2822 date (`Sat, 24 Oct 2026 19:30:00 GMT`, `+0200`, …)
/// to unix ms. `None` if it can't be read.
pub fn parse_rfc822(s: &str) -> Option<u64> {
    let s = s.trim();
    let s = s.split_once(',').map(|(_, r)| r.trim()).unwrap_or(s);
    let mut it = s.split_whitespace();
    let day: u32 = it.next()?.parse().ok()?;
    let mon = match &it.next()?.to_ascii_lowercase()[..] {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    };
    let mut year: i64 = it.next()?.parse().ok()?;
    if year < 100 {
        year += 2000;
    }
    let mut hms = it.next()?.split(':');
    let h: i64 = hms.next()?.parse().ok()?;
    let m: i64 = hms.next()?.parse().ok()?;
    let sec: i64 = hms.next().map(|x| x.parse().ok()).unwrap_or(Some(0))?;
    if day == 0 || day > 31 || h > 23 || m > 59 || sec > 60 {
        return None;
    }
    let off_min: i64 = match it.next().unwrap_or("GMT") {
        "GMT" | "UT" | "UTC" | "Z" => 0,
        z if (z.starts_with('+') || z.starts_with('-')) && z.len() == 5 => {
            let v: i64 = z[1..].parse().ok()?;
            let mins = (v / 100) * 60 + v % 100;
            if z.starts_with('-') {
                -mins
            } else {
                mins
            }
        }
        _ => 0,
    };
    let days = days_from_civil(year, mon, day);
    let secs = days * 86_400 + h * 3600 + m * 60 + sec - off_min * 60;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

/// Days since 1970-01-01 (proleptic Gregorian; H. Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn news_rss_dates() {
        assert_eq!(parse_rfc822("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(
            parse_rfc822("Sat, 24 Oct 2026 19:30:00 GMT"),
            Some(1_792_870_200_000)
        );
        assert_eq!(
            parse_rfc822("Sat, 24 Oct 2026 21:30:00 +0200"),
            Some(1_792_870_200_000)
        );
        assert_eq!(parse_rfc822("yesterday"), None);
    }

    #[test]
    fn news_rss_unescape() {
        assert_eq!(unescape("a &amp; b &lt;c&gt; &#233;&#x41;"), "a & b <c> éA");
        assert_eq!(unescape("AT&T"), "AT&T");
    }

    #[test]
    fn news_rss_rejects_non_rss_and_oversize() {
        assert!(parse_rss("<html><body>hi</body></html>").is_err());
        let big = format!("<rss>{}</rss>", " ".repeat(MAX_FEED_BYTES));
        assert!(parse_rss(&big).is_err());
    }
}

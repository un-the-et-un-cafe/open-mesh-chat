//! Hopla News: Google News RSS relayed over the mesh (REQUIREMENTS § Hopla
//! News, v0.4 draft, revised 2026-10-24) — **experimental**.
//!
//! PROTOCOL.md has no News section yet, so the wire choices below are this
//! implementation's (see `docs/NEWS.md`, "Spec gaps").
//!
//! # Ingest path (the only way news is made)
//!
//! A phone with internet fetches the Google News RSS feed of a channel it
//! subscribes to ([`NewsChannel::feed_url`], public URL, no API key) and hands
//! the body to [`items_from_feed`], which only accepts the URL this module
//! built for that channel. Each item becomes one envelope ([`seal_news`]).
//! There is no API to publish anything else: users can't post into News.
//!
//! # Channels and tags
//!
//! A channel is `(language, topic[, city])`, written as the canonical
//! descriptor `"<lang>/<topic>"` or `"<lang>/local/<city>"` (city lowercased,
//! whitespace collapsed). Its mailbox tag (TLV `0x54`) is public and does not
//! rotate: `tag = BLAKE2b-64("OMCP-news-tag-v1" ‖ 0x00 ‖ descriptor)`.
//!
//! # Envelope
//!
//! A normal [`EnvelopeV1`] with `flags = RELAY_OK | STORE_OK`,
//! `envelope_id = BLAKE2b-256("OMCP-news-id-v1" ‖ 0x00 ‖ source ‖ 0x00 ‖ link)[0..16]`
//! (dedup by source + link, NW2), `created_ms` = the story's publication time,
//! and `header_aad` = `0x54` tag ‖ `0x58` outer expiry (48 h after
//! publication; Emergencies 7 d) ‖ `0x8E` descriptor (experimental TLV, so
//! relays can tell news from chats and check the tag).
//! `ciphertext = nonce[24] ‖ XChaCha20-Poly1305(key, nonce, plain, aad)` with
//! `key = HKDF-SHA-256(salt = "OMCP-news-key-v1", ikm = descriptor)` (a public
//! channel: anyone who knows the channel can read it), AEAD AAD =
//! `"OMCP-news-v1" ‖ version ‖ suite ‖ envelope_id ‖ created_ms ‖ flags ‖ header_aad`,
//! and `plain = len u32 BE ‖ CBOR(NewsItem) ‖ zero padding` to a §22.3
//! bucket (256 B / 1 KiB / 4 KiB).
//!
//! # Receive rule (publish ban)
//!
//! [`open_news`] accepts an envelope only if everything above holds *and*
//! the item looks like it came out of [`items_from_feed`]: a registered RSS
//! source, a link on that source's host, the channel's language/topic/city,
//! `envelope_id` = hash of source + link, `created_ms` = publication time,
//! not in the future, and an expiry within the news lifetime. Anything else
//! on a news tag, or carrying the news TLV, is dropped by relays and readers.
//! Without per-story signatures (the requirements rule them out) a modified
//! client can still forge an item that passes these checks; see the gaps.

pub mod gate;
pub mod rss;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::compress::bucket_for;
use crate::sync::relay::{expires_from_aad, put_expires_in_aad};
use crate::sync::tag::{parse_routing_aad, put_tag_in_aad, MailboxTag};

/// `header_aad` TLV carrying the news channel descriptor. Experimental /
/// private-use range (§21.8) until the registry assigns one.
pub const AAD_TLV_NEWS_CHANNEL: u8 = 0x8E;
/// Outer expiry of a story: 48 h after publication.
pub const NEWS_EXPIRY_MS: u64 = 48 * 3_600_000;
/// Outer expiry of an Emergencies story: 7 days.
pub const EMERGENCY_EXPIRY_MS: u64 = 7 * 24 * 3_600_000;
/// News kept per phone (REQUIREMENTS: ≤ 20 MB, oldest dropped first).
pub const NEWS_STORAGE_CAP_BYTES: u64 = 20_000_000;
/// A phone with internet fetches each subscribed feed at most this often
/// (the requirements leave the interval open).
pub const FETCH_INTERVAL_MS: u64 = 30 * 60_000;
/// Newest items taken from one feed fetch.
pub const MAX_ITEMS_PER_FEED: usize = 30;
/// Publication times this far in the future are accepted (clock skew).
pub const MAX_CLOCK_SKEW_MS: u64 = 10 * 60_000;
/// Encoded item limit (REQUIREMENTS: ≤ 2 KB without thumbnail).
pub const MAX_ITEM_BYTES: usize = 2048;
/// Field limits (bytes, UTF-8).
pub const MAX_TITLE: usize = 300;
pub const MAX_SUMMARY: usize = 400;
pub const MAX_LINK: usize = 1024;
pub const MAX_PUBLISHER: usize = 100;
pub const MAX_CITY: usize = 64;
/// `NewsItem::v`.
pub const NEWS_ITEM_VERSION: u8 = 1;

const NONCE_LEN: usize = 24;
const AEAD_PREFIX: &[u8] = b"OMCP-news-v1";
const ENVELOPE_FLAGS: u16 = flags::RELAY_OK | flags::STORE_OK;

/// Topics a channel can carry. `Top` in the apps is a view over the
/// subscribed channels, not a channel of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NewsTopic {
    World,
    Politics,
    Tech,
    Sport,
    Health,
    Emergencies,
    Local,
}

impl NewsTopic {
    pub const ALL: [NewsTopic; 7] = [
        NewsTopic::World,
        NewsTopic::Politics,
        NewsTopic::Tech,
        NewsTopic::Sport,
        NewsTopic::Health,
        NewsTopic::Emergencies,
        NewsTopic::Local,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::World => "world",
            Self::Politics => "politics",
            Self::Tech => "tech",
            Self::Sport => "sport",
            Self::Health => "health",
            Self::Emergencies => "emergencies",
            Self::Local => "local",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.code() == s)
    }

    /// Outer expiry after publication.
    pub fn lifetime_ms(self) -> u64 {
        if self == Self::Emergencies {
            EMERGENCY_EXPIRY_MS
        } else {
            NEWS_EXPIRY_MS
        }
    }
}

/// Feed languages (REQUIREMENTS: EN, FR, HE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NewsLang {
    En,
    Fr,
    He,
}

impl NewsLang {
    pub const ALL: [NewsLang; 3] = [NewsLang::En, NewsLang::Fr, NewsLang::He];

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
            Self::He => "he",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.code() == s)
    }

    /// Google News edition parameters.
    fn edition(self) -> &'static str {
        match self {
            Self::En => "hl=en-US&gl=US&ceid=US:en",
            Self::Fr => "hl=fr&gl=FR&ceid=FR:fr",
            Self::He => "hl=he&gl=IL&ceid=IL:he",
        }
    }

    fn politics_query(self) -> &'static str {
        match self {
            Self::En => "politics",
            Self::Fr => "politique",
            Self::He => "פוליטיקה",
        }
    }

    fn emergencies_query(self) -> &'static str {
        match self {
            Self::En => "\"emergency alert\" OR evacuation",
            Self::Fr => "\"alerte\" OR évacuation",
            Self::He => "התרעה OR פינוי",
        }
    }
}

/// An RSS source. v0.4: Google News only; a second source (the X news RSS
/// feed) is added here later without protocol changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RssSource {
    /// Short id carried in items.
    pub id: &'static str,
    /// Display name for the source label.
    pub name: &'static str,
    /// Item links must start with this.
    pub link_prefix: &'static str,
    /// Feed URLs start with this.
    pub feed_prefix: &'static str,
}

pub const GOOGLE_NEWS: RssSource = RssSource {
    id: "gn",
    name: "Google News",
    link_prefix: "https://news.google.com/",
    feed_prefix: "https://news.google.com/rss",
};

/// Registered sources.
pub const SOURCES: &[RssSource] = &[GOOGLE_NEWS];

pub fn source(id: &str) -> Option<&'static RssSource> {
    SOURCES.iter().find(|s| s.id == id)
}

/// Canonical city name for a channel: trimmed, whitespace collapsed,
/// lowercased; 1–64 bytes, no `/` or control characters.
pub fn normalize_city(s: &str) -> Result<String> {
    let c = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if c.is_empty() || c.len() > MAX_CITY || c.contains('/') || c.chars().any(char::is_control) {
        return Err(OmcpError::Sync("news city name"));
    }
    Ok(c)
}

/// A public news channel.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NewsChannel {
    pub lang: NewsLang,
    pub topic: NewsTopic,
    /// Normalized city, only for [`NewsTopic::Local`].
    pub city: Option<String>,
}

impl NewsChannel {
    pub fn topic(lang: NewsLang, topic: NewsTopic) -> Result<Self> {
        if topic == NewsTopic::Local {
            return Err(OmcpError::Sync("local news needs a city"));
        }
        Ok(Self {
            lang,
            topic,
            city: None,
        })
    }

    pub fn local(lang: NewsLang, city: &str) -> Result<Self> {
        Ok(Self {
            lang,
            topic: NewsTopic::Local,
            city: Some(normalize_city(city)?),
        })
    }

    /// `"<lang>/<topic>"` or `"<lang>/local/<city>"`.
    pub fn descriptor(&self) -> String {
        match &self.city {
            Some(c) => format!("{}/local/{c}", self.lang.code()),
            None => format!("{}/{}", self.lang.code(), self.topic.code()),
        }
    }

    /// Parse a descriptor; it must already be canonical.
    pub fn parse(d: &str) -> Result<Self> {
        let bad = OmcpError::Sync("news channel descriptor");
        let mut it = d.splitn(3, '/');
        let lang = it.next().and_then(NewsLang::from_code).ok_or(bad.clone())?;
        let topic = it
            .next()
            .and_then(NewsTopic::from_code)
            .ok_or(bad.clone())?;
        let ch = match (topic, it.next()) {
            (NewsTopic::Local, Some(c)) => Self::local(lang, c)?,
            (NewsTopic::Local, None) | (_, Some(_)) => return Err(bad),
            (t, None) => Self::topic(lang, t)?,
        };
        if ch.descriptor() != d {
            return Err(bad);
        }
        Ok(ch)
    }

    /// Public mailbox tag of the channel.
    pub fn tag(&self) -> MailboxTag {
        let mut t = [0u8; 8];
        blake2b(b"OMCP-news-tag-v1", &[self.descriptor().as_bytes()], &mut t);
        t
    }

    /// Channel key (public: derived from the descriptor only).
    fn key(&self) -> [u8; 32] {
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(
            Some(b"OMCP-news-key-v1"),
            self.descriptor().as_bytes(),
        );
        let mut k = [0u8; 32];
        hk.expand(b"", &mut k)
            .expect("32 bytes is a valid HKDF-SHA-256 length");
        k
    }

    /// The public Google News RSS feed for this channel (no API key).
    pub fn feed_url(&self) -> String {
        let base = GOOGLE_NEWS.feed_prefix;
        let ed = self.lang.edition();
        let section = |s: &str| format!("{base}/headlines/section/topic/{s}?{ed}");
        let search = |q: &str| format!("{base}/search?q={}&{ed}", percent_encode(q));
        match self.topic {
            NewsTopic::World => section("WORLD"),
            NewsTopic::Tech => section("TECHNOLOGY"),
            NewsTopic::Sport => section("SPORTS"),
            NewsTopic::Health => section("HEALTH"),
            NewsTopic::Politics => search(self.lang.politics_query()),
            NewsTopic::Emergencies => search(self.lang.emergencies_query()),
            NewsTopic::Local => search(self.city.as_deref().unwrap_or_default()),
        }
    }
}

fn percent_encode(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn blake2b(domain: &[u8], parts: &[&[u8]], out: &mut [u8]) {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(out.len()).expect("valid BLAKE2b output size");
    h.update(domain);
    for p in parts {
        h.update(&[0]);
        h.update(p);
    }
    h.finalize_variable(out).expect("output length");
}

/// Story id: `BLAKE2b-256("OMCP-news-id-v1" ‖ 0 ‖ source ‖ 0 ‖ link)`.
pub fn news_id(source: &str, link: &str) -> [u8; 32] {
    let mut o = [0u8; 32];
    blake2b(
        b"OMCP-news-id-v1",
        &[source.as_bytes(), link.as_bytes()],
        &mut o,
    );
    o
}

/// Envelope id of a story (first 16 bytes of [`news_id`]).
pub fn news_envelope_id(source: &str, link: &str) -> [u8; 16] {
    let mut o = [0u8; 16];
    o.copy_from_slice(&news_id(source, link)[..16]);
    o
}

/// One story as relayed (CBOR, short keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewsItem {
    pub v: u8,
    /// [`RssSource::id`].
    pub src: String,
    pub link: String,
    pub title: String,
    pub summary: String,
    /// Publisher named by the feed (e.g. "The Riverside Post").
    #[serde(rename = "pub")]
    pub publisher: String,
    /// Publication time (unix ms), also the envelope's `created_ms`.
    pub ts: u64,
    pub lang: String,
    pub topic: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
}

impl NewsItem {
    pub fn channel(&self) -> Result<NewsChannel> {
        let lang = NewsLang::from_code(&self.lang).ok_or(OmcpError::Sync("news language"))?;
        let topic = NewsTopic::from_code(&self.topic).ok_or(OmcpError::Sync("news topic"))?;
        match (topic, &self.city) {
            (NewsTopic::Local, Some(c)) => {
                let ch = NewsChannel::local(lang, c)?;
                if ch.city.as_deref() != Some(c.as_str()) {
                    return Err(OmcpError::Sync("news city not canonical"));
                }
                Ok(ch)
            }
            (NewsTopic::Local, None) | (_, Some(_)) => Err(OmcpError::Sync("news city")),
            (t, None) => NewsChannel::topic(lang, t),
        }
    }

    pub fn envelope_id(&self) -> [u8; 16] {
        news_envelope_id(&self.src, &self.link)
    }

    pub fn expires_at(&self) -> Result<u64> {
        Ok(self.ts.saturating_add(self.channel()?.topic.lifetime_ms()))
    }

    fn check_fields(&self) -> Result<()> {
        let bad = |m| Err(OmcpError::Sync(m));
        if self.v != NEWS_ITEM_VERSION {
            return bad("news item version");
        }
        let Some(src) = source(&self.src) else {
            return bad("news source not registered");
        };
        if !valid_link(src, &self.link) {
            return bad("news link not from its source");
        }
        if self.title.is_empty() || self.title.len() > MAX_TITLE {
            return bad("news title");
        }
        if self.summary.len() > MAX_SUMMARY || self.publisher.len() > MAX_PUBLISHER {
            return bad("news field length");
        }
        if [&self.title, &self.summary, &self.publisher]
            .iter()
            .any(|s| s.chars().any(char::is_control))
        {
            return bad("news control characters");
        }
        self.channel().map(|_| ())
    }

    fn encode(&self) -> Result<Vec<u8>> {
        let mut v = Vec::new();
        ciborium::into_writer(self, &mut v).map_err(|_| OmcpError::Sync("news item encode"))?;
        Ok(v)
    }
}

fn valid_link(src: &RssSource, link: &str) -> bool {
    link.len() <= MAX_LINK
        && link.starts_with(src.link_prefix)
        && link.len() > src.link_prefix.len()
        && !link.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max.saturating_sub('…'.len_utf8());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", s[..end].trim_end())
}

/// Turn a fetched feed body into news items for `channel`.
///
/// `feed_url` must be exactly `channel.feed_url()` (the ingest path accepts
/// nothing else). Items whose link isn't on the source's host, without a
/// title or date, or already expired, are skipped; the newest
/// [`MAX_ITEMS_PER_FEED`] are kept. Publication times in the future are
/// clamped to `now_ms`.
pub fn items_from_feed(
    channel: &NewsChannel,
    feed_url: &str,
    xml: &str,
    now_ms: u64,
) -> Result<Vec<NewsItem>> {
    if feed_url != channel.feed_url() {
        return Err(OmcpError::Sync("not this channel's RSS feed"));
    }
    let src = &GOOGLE_NEWS;
    let life = channel.topic.lifetime_ms();
    let mut out: Vec<NewsItem> = Vec::new();
    for it in rss::parse_rss(xml)? {
        let Some(ts) = it.published_ms else { continue };
        let ts = ts.min(now_ms);
        if ts.saturating_add(life) <= now_ms || !valid_link(src, &it.link) {
            continue;
        }
        let (title, publisher) = split_publisher(&it.title, &it.source);
        if title.is_empty() {
            continue;
        }
        let item = NewsItem {
            v: NEWS_ITEM_VERSION,
            src: src.id.to_string(),
            link: it.link.clone(),
            title: truncate(&title, MAX_TITLE),
            summary: truncate(
                &summary_from(&it.description, &title, &publisher),
                MAX_SUMMARY,
            ),
            publisher: truncate(&publisher, MAX_PUBLISHER),
            ts,
            lang: channel.lang.code().to_string(),
            topic: channel.topic.code().to_string(),
            city: channel.city.clone(),
        };
        if out.iter().any(|o| o.link == item.link) {
            continue; // same story twice in one feed
        }
        if item.check_fields().is_ok() {
            out.push(item);
        }
    }
    out.sort_by(|a, b| b.ts.cmp(&a.ts).then_with(|| a.link.cmp(&b.link)));
    out.truncate(MAX_ITEMS_PER_FEED);
    Ok(out)
}

/// Google News titles read "Headline - Publisher".
fn split_publisher(title: &str, source: &str) -> (String, String) {
    if !source.is_empty() {
        if let Some(t) = title.strip_suffix(source) {
            if let Some(t) = t.trim_end().strip_suffix('-') {
                return (t.trim().to_string(), source.to_string());
            }
        }
        return (title.trim().to_string(), source.to_string());
    }
    match title.rsplit_once(" - ") {
        Some((t, p)) if !t.trim().is_empty() => (t.trim().to_string(), p.trim().to_string()),
        _ => (title.trim().to_string(), String::new()),
    }
}

/// The feed's description minus the repeated headline and publisher.
/// Google News descriptions are often just those two, giving "".
fn summary_from(desc: &str, title: &str, publisher: &str) -> String {
    let mut s = desc.trim();
    if let Some(r) = s.strip_prefix(title) {
        s = r.trim();
    }
    if !publisher.is_empty() {
        if let Some(r) = s.strip_suffix(publisher) {
            s = r.trim();
        }
    }
    if s.len() < 20 {
        String::new()
    } else {
        s.to_string()
    }
}

fn aead_aad(env: &EnvelopeV1) -> Vec<u8> {
    let mut a = AEAD_PREFIX.to_vec();
    a.extend_from_slice(&env.version.to_be_bytes());
    a.extend_from_slice(&env.suite_id.to_be_bytes());
    a.extend_from_slice(&env.envelope_id);
    a.extend_from_slice(&env.created_ms.to_be_bytes());
    a.extend_from_slice(&env.flags.to_be_bytes());
    a.extend_from_slice(&env.header_aad);
    a
}

/// The channel descriptor TLV (`0x8E`) in `header_aad`, if present.
pub fn channel_in_aad(aad: &[u8]) -> Result<Option<String>> {
    let mut i = 0;
    let mut found = None;
    while i + 2 <= aad.len() {
        let (t, l) = (aad[i], aad[i + 1] as usize);
        let v = aad
            .get(i + 2..i + 2 + l)
            .ok_or(OmcpError::Sync("aad tlv truncated"))?;
        if t == AAD_TLV_NEWS_CHANNEL {
            if found.is_some() {
                return Err(OmcpError::Sync("duplicate news channel tlv"));
            }
            let s = std::str::from_utf8(v).map_err(|_| OmcpError::Sync("news channel utf-8"))?;
            found = Some(s.to_string());
        }
        i += 2 + l;
    }
    Ok(found)
}

/// Whether the envelope claims to be news (carries the news TLV).
pub fn is_news_envelope(env: &EnvelopeV1) -> bool {
    !matches!(channel_in_aad(&env.header_aad), Ok(None))
}

/// Seal one ingested story into a news envelope (`ttl_hops` from the
/// relay's adaptive TTL).
pub fn seal_news(item: &NewsItem, ttl_hops: u8) -> Result<EnvelopeV1> {
    item.check_fields()?;
    let ch = item.channel()?;
    let mut it = item.clone();
    // Fit a 1 KiB bucket when trimming the summary is enough.
    let mut cbor = it.encode()?;
    while cbor.len() + 4 > 1024 && !it.summary.is_empty() {
        let keep = it.summary.len().saturating_sub(64);
        it.summary = if keep < 20 {
            String::new()
        } else {
            truncate(&it.summary, keep)
        };
        cbor = it.encode()?;
    }
    if cbor.len() > MAX_ITEM_BYTES {
        return Err(OmcpError::Sync("news item too large"));
    }
    let desc = ch.descriptor();
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &ch.tag());
    put_expires_in_aad(&mut header_aad, it.expires_at()?);
    header_aad.push(AAD_TLV_NEWS_CHANNEL);
    header_aad.push(u8::try_from(desc.len()).map_err(|_| OmcpError::Sync("news descriptor"))?);
    header_aad.extend_from_slice(desc.as_bytes());

    let mut env = EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: it.envelope_id(),
        flags: ENVELOPE_FLAGS,
        ttl_hops,
        created_ms: it.ts,
        header_aad,
        ciphertext: Vec::new(),
    };
    let padded_len = bucket_for(cbor.len() + 4);
    let mut plain = Vec::with_capacity(padded_len);
    plain.extend_from_slice(&(cbor.len() as u32).to_be_bytes());
    plain.extend_from_slice(&cbor);
    plain.resize(padded_len, 0);
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut nonce).map_err(|_| OmcpError::Sync("rng"))?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&ch.key()).map_err(|_| OmcpError::Sync("news key"))?;
    let aad = aead_aad(&env);
    let ct = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &aad,
            },
        )
        .map_err(|_| OmcpError::Sync("news seal"))?;
    env.ciphertext = nonce.to_vec();
    env.ciphertext.extend_from_slice(&ct);
    Ok(env)
}

/// Open and check a news envelope (the receive rule in the module docs).
/// Any error means: not a valid news item, drop it.
pub fn open_news(env: &EnvelopeV1, now_ms: u64) -> Result<NewsItem> {
    let bad = |m| Err(OmcpError::Sync(m));
    let Some(desc) = channel_in_aad(&env.header_aad)? else {
        return bad("not a news envelope");
    };
    let ch = NewsChannel::parse(&desc)?;
    let routing = parse_routing_aad(&env.header_aad)?;
    if routing.tag != Some(ch.tag()) || !routing.subtags.is_empty() {
        return bad("news tag does not match its channel");
    }
    if env.flags != ENVELOPE_FLAGS
        || env.version != ENVELOPE_VERSION_V1
        || env.suite_id != SUITE_OMCP_2026_A
    {
        return bad("news envelope header");
    }
    if env.ciphertext.len() < NONCE_LEN + 16 {
        return bad("news ciphertext");
    }
    let (nonce, ct) = env.ciphertext.split_at(NONCE_LEN);
    let cipher =
        XChaCha20Poly1305::new_from_slice(&ch.key()).map_err(|_| OmcpError::Sync("news key"))?;
    let plain = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: &aead_aad(env),
            },
        )
        .map_err(|_| OmcpError::Sync("news open"))?;
    if plain.len() < 4 || bucket_for(plain.len()) != plain.len() || plain.len() > 4096 {
        return bad("news padding");
    }
    let n = u32::from_be_bytes(plain[..4].try_into().expect("4")) as usize;
    if n > MAX_ITEM_BYTES || 4 + n > plain.len() || plain[4 + n..].iter().any(|b| *b != 0) {
        return bad("news padding");
    }
    let item: NewsItem =
        ciborium::from_reader(&plain[4..4 + n]).map_err(|_| OmcpError::Sync("news item cbor"))?;
    item.check_fields()?;
    if item.channel()? != ch {
        return bad("news item not on its channel");
    }
    if env.envelope_id != item.envelope_id() {
        return bad("news id is not hash(source, link)");
    }
    if env.created_ms != item.ts || item.ts > now_ms.saturating_add(MAX_CLOCK_SKEW_MS) {
        return bad("news time");
    }
    let Some(exp) = expires_from_aad(&env.header_aad)? else {
        return bad("news without outer expiry");
    };
    if exp > item.expires_at()? || exp <= now_ms {
        return bad("news expiry");
    }
    Ok(item)
}

#[cfg(test)]
mod tests;

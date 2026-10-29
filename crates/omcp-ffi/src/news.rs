//! Hopla News (REQUIREMENTS § Hopla News, v0.4 draft) — FFI glue,
//! **experimental**.
//!
//! The core rules live in [`omcp_core::news`] (channels, tags, envelope,
//! publish ban) and the relay engine (RL2 news class, 20 MB news cap). This
//! module keeps the user's interests, tells the apps which public Google News
//! RSS feeds are due ([`OmcpNode::news_due_feeds`]), turns fetched feed bodies
//! into news envelopes ([`OmcpNode::news_ingest_rss`], the only way news is
//! created; there is no publish API), and serves the reading views from the
//! news envelopes in the carry store.
//!
//! Who fetches: any phone with internet (the apps call `news_due_feeds` when
//! they are online). How often: each subscribed feed at most every 30 min
//! ([`omcp_core::news::FETCH_INTERVAL_MS`]); a failed fetch waits 5 min.

use std::collections::{BTreeSet, HashMap};

use omcp_core::news::{
    self as core_news, NewsChannel, NewsItem, NewsLang, NewsTopic, FETCH_INTERVAL_MS,
};
use omcp_core::sync::budget::RelayClass;
use omcp_core::sync::relay::RelayEvent;
use omcp_core::sync::tag::MailboxTag;
use serde::{Deserialize, Serialize};

use crate::{invalid, now_ms, FfiError, NodeState, OmcpNode};

const NEWS_KEY: &[u8] = b"news-state";
/// Wait after a failed fetch before trying that feed again.
pub const FETCH_RETRY_MS: u64 = 5 * 60_000;
/// Stories returned by one feed view at most.
const MAX_VIEW: usize = 200;

/// The user's news interests (News settings / onboarding, design §5.115).
/// Personalisation runs on this phone only: it decides which channel tags
/// are subscribed; nothing is sent anywhere.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, Serialize, Deserialize)]
pub struct NewsPrefs {
    /// Topic codes (`world`, `politics`, `tech`, `sport`, `health`, `emergencies`).
    pub topics: Vec<String>,
    /// Language codes (`en`, `fr`, `he`).
    pub langs: Vec<String>,
    /// City for Local news (typed, or a city name the app got from the OS).
    pub city: Option<String>,
    /// The city came from the phone's location (label only).
    pub city_from_location: bool,
    /// Muted topic codes: hidden from Top and Local.
    pub muted: Vec<String>,
    /// Notify about Emergencies stories (on by default).
    pub alerts_notify: bool,
    /// The user went through (or skipped) the interests step.
    pub interests_chosen: bool,
}

impl Default for NewsPrefs {
    /// Out of the box: newest world headlines (plus emergency alerts).
    fn default() -> Self {
        Self {
            topics: vec!["world".into(), "emergencies".into()],
            langs: vec!["en".into()],
            city: None,
            city_from_location: false,
            muted: Vec::new(),
            alerts_notify: true,
            interests_chosen: false,
        }
    }
}

/// One story for the News views.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NewsStory {
    pub id_hex: String,
    pub topic: String,
    pub lang: String,
    pub city: Option<String>,
    pub title: String,
    /// May be empty (Google News RSS often has no summary).
    pub summary: String,
    pub publisher: String,
    /// Source label, e.g. "Google News".
    pub source_name: String,
    /// Source id, e.g. "gn" (the reader's link key is per source).
    pub source_id: String,
    pub link: String,
    pub published_ms: u64,
    pub expires_ms: u64,
    /// Relaying contact name or short PeerID; `None` when this phone fetched it
    /// (or the relaying link is unknown).
    pub via: Option<String>,
    /// This phone fetched the story itself.
    pub fetched_here: bool,
    pub emergency: bool,
}

/// News counters for the status strip and News storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct NewsStatus {
    pub stories: u32,
    pub storage_bytes: u64,
    pub storage_cap_bytes: u64,
    /// Last successful fetch by this phone.
    pub last_fetch_ms: Option<u64>,
    pub neighbours: u32,
    /// News envelopes dropped by the publish ban.
    pub rejected: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Via {
    Here,
    Peer([u8; 32]),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct NewsRec {
    prefs: Option<NewsPrefs>,
    last_ok: Vec<(String, u64)>,
    via: Vec<([u8; 16], Via)>,
    /// Gateway fetch byte counter `(day, bytes)`: the 500 MB/day cap holds across restarts.
    #[serde(default)]
    gate: Option<(u64, u64)>,
}

/// News state held next to the relay engine.
#[derive(Debug, Default)]
pub(crate) struct NewsState {
    prefs: NewsPrefs,
    last_ok: HashMap<String, u64>,
    last_try: HashMap<String, u64>,
    via: HashMap<[u8; 16], Via>,
    /// Opened news envelopes in the carry store (or delivered), by id.
    cache: HashMap<[u8; 16], NewsItem>,
    /// Emergency stories not yet handed to the app for notification.
    alerts: Vec<[u8; 16]>,
    /// Daily byte accounting for RSS fetches (gateway rules).
    gate: core_news::gate::FetchGate,
}

/// What the app may fetch now under the gateway rules.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NewsFetchPlan {
    /// Feed URLs to GET now (empty when blocked).
    pub feeds: Vec<String>,
    /// Show the "metered connection" warning.
    pub metered_warning: bool,
    /// `offline`, `wifi_only` or `daily_cap`; `None` when allowed.
    pub blocked: Option<String>,
    pub used_today_bytes: u64,
    pub daily_cap_bytes: u64,
}

fn validate(p: &NewsPrefs) -> Result<NewsPrefs, FfiError> {
    let mut out = p.clone();
    let mut topics = BTreeSet::new();
    for t in &p.topics {
        match NewsTopic::from_code(t) {
            Some(NewsTopic::Local) | None => return Err(invalid("unknown news topic")),
            Some(t) => topics.insert(t),
        };
    }
    let mut langs = BTreeSet::new();
    for l in &p.langs {
        langs.insert(NewsLang::from_code(l).ok_or_else(|| invalid("unknown news language"))?);
    }
    for m in &p.muted {
        NewsTopic::from_code(m).ok_or_else(|| invalid("unknown news topic"))?;
    }
    out.topics = topics.iter().map(|t| t.code().to_string()).collect();
    out.langs = langs.iter().map(|l| l.code().to_string()).collect();
    out.city = match p.city.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(c) => {
            core_news::normalize_city(c).map_err(|_| invalid("news city name"))?;
            Some(c.split_whitespace().collect::<Vec<_>>().join(" "))
        }
    };
    if out.city.is_none() {
        out.city_from_location = false;
    }
    Ok(out)
}

impl NewsState {
    /// Channels the user subscribes to.
    pub(crate) fn channels(&self) -> Vec<NewsChannel> {
        let mut v = BTreeSet::new();
        for l in self
            .prefs
            .langs
            .iter()
            .filter_map(|l| NewsLang::from_code(l))
        {
            for t in self
                .prefs
                .topics
                .iter()
                .filter_map(|t| NewsTopic::from_code(t))
            {
                if let Ok(c) = NewsChannel::topic(l, t) {
                    v.insert(c);
                }
            }
            if let Some(c) = self.prefs.city.as_deref() {
                if let Ok(c) = NewsChannel::local(l, c) {
                    v.insert(c);
                }
            }
        }
        v.into_iter().collect()
    }

    pub(crate) fn tags(&self) -> BTreeSet<MailboxTag> {
        self.channels().iter().map(NewsChannel::tag).collect()
    }

    /// Track news carry-store changes (call with every event batch).
    pub(crate) fn on_events(&mut self, ev: &[RelayEvent], now: u64) {
        for e in ev {
            match e {
                RelayEvent::Stored(it) if it.class == RelayClass::News => {
                    self.note(&it.envelope, now)
                }
                RelayEvent::Removed(id) => {
                    self.cache.remove(id);
                    self.via.remove(id);
                }
                _ => {}
            }
        }
    }

    /// A valid news envelope is held or was delivered.
    pub(crate) fn note(&mut self, env: &omcp_core::EnvelopeV1, now: u64) {
        let id = env.envelope_id;
        if self.cache.contains_key(&id) {
            return;
        }
        let Ok(item) = core_news::open_news(env, now) else {
            return;
        };
        let subscribed = item
            .channel()
            .map(|c| self.channels().contains(&c))
            .unwrap_or(false);
        if item.topic == NewsTopic::Emergencies.code() && subscribed {
            self.alerts.push(id);
        }
        self.cache.insert(id, item);
    }

    /// Remember the neighbour that handed us story `id` ("via ‹contact›").
    /// Returns whether anything changed.
    pub(crate) fn relayed_by(&mut self, id: [u8; 16], peer: [u8; 32]) -> bool {
        if self.via.contains_key(&id) {
            return false;
        }
        self.via.insert(id, Via::Peer(peer));
        true
    }

    fn save_rec(&self) -> NewsRec {
        NewsRec {
            prefs: Some(self.prefs.clone()),
            last_ok: self.last_ok.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            via: self
                .via
                .iter()
                .filter(|(k, _)| self.cache.contains_key(*k))
                .map(|(k, v)| (*k, *v))
                .collect(),
            gate: Some(self.gate.snapshot()),
        }
    }
}

fn short_peer(id: &[u8; 32]) -> String {
    format!("omcp1:{}…", hex::encode(&id[..2]))
}

impl NodeState {
    /// Load news prefs and rebuild the story cache from the carry store.
    pub(crate) fn load_news(&mut self, now: u64) -> Result<(), FfiError> {
        let rec: NewsRec = self.db.get_meta(NEWS_KEY)?.unwrap_or_default();
        let n = &mut self.mesh.news;
        n.prefs = rec.prefs.unwrap_or_default();
        n.last_ok = rec.last_ok.into_iter().collect();
        n.via = rec.via.into_iter().collect();
        if let Some((day, used)) = rec.gate {
            n.gate = core_news::gate::FetchGate::from_snapshot(day, used);
        }
        for id in self.mesh.engine.item_ids() {
            if let Some(it) = self.mesh.engine.item(&id) {
                if it.class == RelayClass::News {
                    let env = it.envelope.clone();
                    self.mesh.news.note(&env, now);
                }
            }
        }
        // Stories found at start are not "new" alerts.
        self.mesh.news.alerts.clear();
        let tags = self.mesh.news.tags();
        self.mesh.engine.set_news_tags(tags);
        Ok(())
    }

    pub(crate) fn save_news(&self) -> Result<(), FfiError> {
        self.db.put_meta(NEWS_KEY, &self.mesh.news.save_rec())?;
        Ok(())
    }

    fn story(&self, id: &[u8; 16], it: &NewsItem) -> NewsStory {
        let via = self.mesh.news.via.get(id).copied();
        let src = core_news::source(&it.src);
        NewsStory {
            id_hex: hex::encode(id),
            topic: it.topic.clone(),
            lang: it.lang.clone(),
            city: it.city.clone(),
            title: it.title.clone(),
            summary: it.summary.clone(),
            publisher: it.publisher.clone(),
            source_name: src.map(|s| s.name).unwrap_or("RSS").to_string(),
            source_id: it.src.clone(),
            link: it.link.clone(),
            published_ms: it.ts,
            expires_ms: it.expires_at().unwrap_or(it.ts),
            via: match via {
                Some(Via::Peer(p)) => Some(
                    self.contacts
                        .get(&p)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| short_peer(&p)),
                ),
                _ => None,
            },
            fetched_here: via == Some(Via::Here),
            emergency: it.topic == NewsTopic::Emergencies.code(),
        }
    }

    fn news_view(&self, tab: &str, now: u64) -> Result<Vec<NewsStory>, FfiError> {
        let n = &self.mesh.news;
        let langs: BTreeSet<&str> = n.prefs.langs.iter().map(String::as_str).collect();
        let subscribed: BTreeSet<NewsChannel> = n.channels().into_iter().collect();
        let city = n
            .prefs
            .city
            .as_deref()
            .and_then(|c| core_news::normalize_city(c).ok());
        let muted: BTreeSet<&str> = n.prefs.muted.iter().map(String::as_str).collect();
        let topic = match tab {
            "top" | "local" => None,
            t => Some(NewsTopic::from_code(t).ok_or_else(|| invalid("unknown news tab"))?),
        };
        let mut v: Vec<(&[u8; 16], &NewsItem)> = n
            .cache
            .iter()
            .filter(|(_, it)| it.expires_at().map(|e| e > now).unwrap_or(false))
            .filter(|(_, it)| langs.contains(it.lang.as_str()))
            .filter(|(_, it)| match (tab, topic) {
                ("top", _) => {
                    !muted.contains(it.topic.as_str())
                        && it
                            .channel()
                            .map(|c| subscribed.contains(&c))
                            .unwrap_or(false)
                }
                ("local", _) => city.is_some() && it.city == city,
                (_, Some(NewsTopic::Local)) => city.is_some() && it.city == city,
                (_, Some(t)) => it.topic == t.code(),
                _ => false,
            })
            .collect();
        // Pinned emergency alerts first (Top, Local), then the city's stories
        // in Top, then newest first. No engagement ranking.
        let pin = |it: &NewsItem| matches!(tab, "top" | "local") && it.topic == "emergencies";
        let local = |it: &NewsItem| tab == "top" && city.is_some() && it.city == city;
        v.sort_by(|(ia, a), (ib, b)| {
            pin(b)
                .cmp(&pin(a))
                .then(local(b).cmp(&local(a)))
                .then(b.ts.cmp(&a.ts))
                .then(ia.cmp(ib))
        });
        v.truncate(MAX_VIEW);
        Ok(v.into_iter().map(|(id, it)| self.story(id, it)).collect())
    }
}

/// Topic codes in tab order (after Top and Local).
#[uniffi::export]
pub fn news_topic_codes() -> Vec<String> {
    [
        "world",
        "politics",
        "tech",
        "sport",
        "health",
        "emergencies",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Supported news languages.
#[uniffi::export]
pub fn news_lang_codes() -> Vec<String> {
    NewsLang::ALL.iter().map(|l| l.code().to_string()).collect()
}

/// Minimum time between two fetches of one feed.
/// Open product question (docs/NEWS.md): REQUIREMENTS says any phone with internet fetches;
/// this build lets only Gateway phones fetch. Flip this one constant to change it.
pub const NEWS_FETCH_GATEWAY_ONLY: bool = true;

/// May this phone fetch News RSS at all? (`gateway_enabled` = Settings › Gateway is on.)
/// The network rules (Wi-Fi-only default, metered warning, daily cap) apply on top via
/// `news_fetch_plan`.
#[uniffi::export]
pub fn news_may_fetch(gateway_enabled: bool) -> bool {
    gateway_enabled || !NEWS_FETCH_GATEWAY_ONLY
}

#[uniffi::export]
pub fn news_fetch_interval_ms() -> u64 {
    FETCH_INTERVAL_MS
}

#[uniffi::export]
impl OmcpNode {
    pub fn news_prefs(&self) -> NewsPrefs {
        self.inner.lock().mesh.news.prefs.clone()
    }

    /// Save interests; the phone then subscribes only to the matching
    /// channel tags.
    pub fn news_set_prefs(&self, prefs: NewsPrefs) -> Result<NewsPrefs, FfiError> {
        let p = validate(&prefs)?;
        let mut st = self.inner.lock();
        st.mesh.news.prefs = p.clone();
        let tags = st.mesh.news.tags();
        st.mesh.engine.set_news_tags(tags);
        st.mesh.tags_for = None;
        crate::mesh::refresh_wants(&mut st, now_ms())?;
        st.save_news()?;
        Ok(p)
    }

    /// Public Google News RSS feed URLs this phone should fetch now (call
    /// only when it has internet; fetch each with a plain HTTPS GET, no API
    /// key, and pass the body to [`Self::news_ingest_rss`]). Each returned
    /// URL is not returned again for 5 min unless ingested.
    pub fn news_due_feeds(&self, now_ms: u64) -> Vec<String> {
        let mut st = self.inner.lock();
        let n = &mut st.mesh.news;
        let mut due = Vec::new();
        for c in n.channels() {
            let u = c.feed_url();
            let ok = n.last_ok.get(&u).copied().unwrap_or(0);
            let tried = n.last_try.get(&u).copied().unwrap_or(0);
            if now_ms >= ok.saturating_add(FETCH_INTERVAL_MS)
                && now_ms >= tried.saturating_add(FETCH_RETRY_MS)
            {
                n.last_try.insert(u.clone(), now_ms);
                due.push(u);
            }
        }
        due
    }

    /// Gateway-gated variant of [`Self::news_due_feeds`]: applies the
    /// gateway rules (Wi-Fi-only by default, metered warns, 500 MB/day cap)
    /// before handing out feeds. Apps should call this, not `news_due_feeds`.
    pub fn news_fetch_plan(
        &self,
        network: crate::Network,
        wifi_only: bool,
        now_ms: u64,
    ) -> NewsFetchPlan {
        use core_news::gate::FetchBlock;
        use omcp_core::gateway::policy::{GatewayPolicy, NetworkKind};
        let policy = GatewayPolicy {
            wifi_only,
            ..GatewayPolicy::default()
        };
        let net = match network {
            crate::Network::Offline => NetworkKind::Offline,
            crate::Network::Wifi { metered } => NetworkKind::Wifi { metered },
            crate::Network::Cellular { metered } => NetworkKind::Cellular { metered },
        };
        let d = self
            .inner
            .lock()
            .mesh
            .news
            .gate
            .decide(&policy, net, now_ms);
        let feeds = if d.allowed {
            self.news_due_feeds(now_ms)
        } else {
            Vec::new()
        };
        let used = self.inner.lock().mesh.news.gate.used_today(now_ms);
        NewsFetchPlan {
            feeds,
            metered_warning: d.metered_warning,
            blocked: d.blocked.map(|b| {
                match b {
                    FetchBlock::Offline => "offline",
                    FetchBlock::WifiOnly => "wifi_only",
                    FetchBlock::DailyCapReached => "daily_cap",
                }
                .to_string()
            }),
            used_today_bytes: used,
            daily_cap_bytes: policy.daily_byte_cap,
        }
    }

    /// The RSS ingest path: `feed_url` must be one of the subscribed
    /// channels' feeds (as returned by [`Self::news_due_feeds`]). New
    /// stories are sealed into news envelopes and handed to the mesh; stories
    /// already seen (same source + link) are skipped. Returns the number of
    /// new stories.
    pub fn news_ingest_rss(
        &self,
        feed_url: String,
        body: String,
        now_ms: u64,
    ) -> Result<u32, FfiError> {
        let mut st = self.inner.lock();
        st.mesh.news.gate.record(body.len() as u64, now_ms);
        // Persist the counter now: the cap must hold even if the body is rejected or the app dies.
        st.save_news()?;
        let ch = st
            .mesh
            .news
            .channels()
            .into_iter()
            .find(|c| c.feed_url() == feed_url)
            .ok_or_else(|| invalid("not a subscribed news feed"))?;
        let items = core_news::items_from_feed(&ch, &feed_url, &body, now_ms)
            .map_err(|_| invalid("unreadable news feed"))?;
        let mut new = 0u32;
        for it in items {
            let ttl = st.mesh.engine.new_ttl();
            let Ok(env) = core_news::seal_news(&it, ttl) else {
                continue;
            };
            let id = env.envelope_id;
            let (ev, fresh) = st.mesh.engine.submit_news(env.clone(), now_ms)?;
            if fresh {
                new += 1;
                st.mesh.news.via.entry(id).or_insert(Via::Here);
                st.mesh.news.note(&env, now_ms);
            }
            st.mesh.news.on_events(&ev, now_ms);
            st.route(ev)?;
        }
        st.mesh.news.last_ok.insert(feed_url, now_ms);
        st.save_news()?;
        Ok(new)
    }

    /// Stories for a tab: `top`, `local`, or a topic code. Expired stories
    /// are never shown.
    pub fn news_feed(&self, tab: String) -> Result<Vec<NewsStory>, FfiError> {
        self.inner.lock().news_view(&tab, now_ms())
    }

    pub fn news_story(&self, id_hex: String) -> Option<NewsStory> {
        let st = self.inner.lock();
        let id: [u8; 16] = hex::decode(&id_hex).ok()?.try_into().ok()?;
        let it = st.mesh.news.cache.get(&id)?;
        Some(st.story(&id, it))
    }

    pub fn news_status(&self) -> NewsStatus {
        let st = self.inner.lock();
        let s = st.mesh.engine.stats();
        let now = now_ms();
        NewsStatus {
            stories: st
                .mesh
                .news
                .cache
                .values()
                .filter(|i| i.expires_at().map(|e| e > now).unwrap_or(false))
                .count() as u32,
            storage_bytes: s.news_bytes,
            storage_cap_bytes: st.mesh.engine.news_cap(),
            last_fetch_ms: st.mesh.news.last_ok.values().copied().max(),
            neighbours: s.neighbours,
            rejected: s.news_rejected,
        }
    }

    /// New Emergencies stories on subscribed channels since the last call,
    /// for a notification. Empty when alerts are turned off.
    pub fn news_take_alerts(&self) -> Vec<NewsStory> {
        let mut st = self.inner.lock();
        let ids = std::mem::take(&mut st.mesh.news.alerts);
        if !st.mesh.news.prefs.alerts_notify {
            return Vec::new();
        }
        ids.iter()
            .filter_map(|id| st.mesh.news.cache.get(id).map(|it| st.story(id, it)))
            .collect()
    }
}

/// Prefix of a shared news card sent as normal chat/community text (NW6).
pub const NEWS_CARD_PREFIX: &str = "hopla-news/1\n";

/// A news card parsed from a message: the app renders it as a card and the
/// replies under that message form its discussion thread (same as a post).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NewsCard {
    pub id_hex: String,
    pub source_id: String,
    pub publisher: String,
    pub link: String,
    pub title: String,
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Card text for sharing a story into a chat or community (send it with
/// the normal send path; community moderation applies as for any post).
#[uniffi::export]
pub fn news_card_text(story: NewsStory) -> String {
    format!(
        "{NEWS_CARD_PREFIX}{}\n{}\n{}\n{}\n{}",
        one_line(&story.id_hex),
        one_line(&story.source_id),
        one_line(&story.publisher),
        one_line(&story.link),
        one_line(&story.title)
    )
}

/// Parse a received message as a news card; `None` for ordinary text or
/// a card whose link is not https.
#[uniffi::export]
pub fn parse_news_card(text: String) -> Option<NewsCard> {
    let rest = text.strip_prefix(NEWS_CARD_PREFIX)?;
    let f: Vec<&str> = rest.splitn(5, '\n').collect();
    if f.len() != 5 || f[0].len() != 32 || !f[0].bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if !f[3].starts_with("https://") {
        return None;
    }
    Some(NewsCard {
        id_hex: f[0].into(),
        source_id: f[1].into(),
        publisher: f[2].into(),
        link: f[3].into(),
        title: f[4].into(),
    })
}

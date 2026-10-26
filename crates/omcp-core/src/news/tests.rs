use super::*;

/// 2026-10-24 20:00:00 GMT.
pub(crate) const NOW: u64 = 1_792_872_000_000;
pub(crate) const FIXTURE: &str = include_str!("fixtures/google_news_world_en.xml");

pub(crate) fn world_en() -> NewsChannel {
    NewsChannel::topic(NewsLang::En, NewsTopic::World).unwrap()
}

pub(crate) fn fixture_items() -> Vec<NewsItem> {
    let ch = world_en();
    items_from_feed(&ch, &ch.feed_url(), FIXTURE, NOW).unwrap()
}

/// Re-seal `item` after `f` changes it, bypassing [`seal_news`]'s checks
/// (what a modified client could send).
fn forge(item: &NewsItem, f: impl FnOnce(&mut NewsItem, &mut EnvelopeV1)) -> EnvelopeV1 {
    let mut it = item.clone();
    let mut env = seal_news(item, 8).unwrap();
    f(&mut it, &mut env);
    let ch = NewsChannel::parse(&channel_in_aad(&env.header_aad).unwrap().unwrap()).unwrap();
    reseal(&it, &mut env, &ch);
    env
}

fn reseal(it: &NewsItem, env: &mut EnvelopeV1, ch: &NewsChannel) {
    let cbor = it.encode().unwrap();
    let mut plain = (cbor.len() as u32).to_be_bytes().to_vec();
    plain.extend_from_slice(&cbor);
    plain.resize(bucket_for(plain.len()), 0);
    let nonce = [7u8; NONCE_LEN];
    let ct = XChaCha20Poly1305::new_from_slice(&ch.key())
        .unwrap()
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: &aead_aad(env),
            },
        )
        .unwrap();
    env.ciphertext = nonce.to_vec();
    env.ciphertext.extend_from_slice(&ct);
}

#[test]
fn news_rss_parse_fixture() {
    let raw = rss::parse_rss(FIXTURE).unwrap();
    assert_eq!(raw.len(), 7);
    assert_eq!(
        raw[1].title,
        "Ministers meet on grain & fuel corridor - Northgate Daily"
    );
    assert_eq!(raw[0].source, "The Riverside Post");
    assert_eq!(
        raw[1].description,
        "Talks in Lakeport aim to reopen the corridor before winter, officials said on Saturday."
    );
    assert_eq!(raw[4].published_ms, None);

    let items = fixture_items();
    // Off-site link, undated, expired and duplicate items are skipped; newest first.
    let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Riverside council opens new flood barrier",
            "Island ferry service resumes after storm",
            "Ministers meet on grain & fuel corridor",
        ]
    );
    let a = &items[0];
    assert_eq!(a.publisher, "The Riverside Post");
    assert_eq!(a.src, "gn");
    assert_eq!(
        a.link,
        "https://news.google.com/rss/articles/CBMiFIXTURE0001?oc=5"
    );
    assert_eq!(a.ts, 1_792_870_200_000);
    assert_eq!(
        (a.lang.as_str(), a.topic.as_str(), a.city.as_ref()),
        ("en", "world", None)
    );
    // Descriptions that only repeat headline + publisher give no summary.
    assert_eq!(a.summary, "");
    assert_eq!(items[1].summary, "");
    assert!(items[2].summary.starts_with("Talks in Lakeport"));
    // +0200 offset read correctly (21:00 +0200 = 19:00 GMT).
    assert_eq!(items[1].ts, 1_792_868_400_000);
}

#[test]
fn news_feed_urls_are_public_google_news_rss() {
    let urls: Vec<String> = [
        world_en(),
        NewsChannel::topic(NewsLang::Fr, NewsTopic::Tech).unwrap(),
        NewsChannel::local(NewsLang::Fr, "  Saint  Omer ").unwrap(),
    ]
    .iter()
    .map(NewsChannel::feed_url)
    .collect();
    assert_eq!(
        urls,
        [
            "https://news.google.com/rss/headlines/section/topic/WORLD?hl=en-US&gl=US&ceid=US:en",
            "https://news.google.com/rss/headlines/section/topic/TECHNOLOGY?hl=fr&gl=FR&ceid=FR:fr",
            "https://news.google.com/rss/search?q=saint%20omer&hl=fr&gl=FR&ceid=FR:fr",
        ]
    );
    for t in NewsTopic::ALL {
        for l in NewsLang::ALL {
            let ch = if t == NewsTopic::Local {
                NewsChannel::local(l, "Riverside").unwrap()
            } else {
                NewsChannel::topic(l, t).unwrap()
            };
            let u = ch.feed_url();
            assert!(u.starts_with(GOOGLE_NEWS.feed_prefix), "{u}");
            assert!(!u.to_ascii_lowercase().contains("key="), "{u}");
        }
    }
}

#[test]
fn news_ingest_only_accepts_the_channel_feed_url() {
    let ch = world_en();
    for url in [
        "https://news.google.com/rss/headlines/section/topic/SPORTS?hl=en-US&gl=US&ceid=US:en",
        "https://evil.example/rss",
        "",
    ] {
        assert!(items_from_feed(&ch, url, FIXTURE, NOW).is_err(), "{url}");
    }
}

#[test]
fn news_channel_descriptors_and_tags() {
    let c = NewsChannel::local(NewsLang::En, "  RIVERSIDE   Town ").unwrap();
    assert_eq!(c.descriptor(), "en/local/riverside town");
    assert_eq!(NewsChannel::parse("en/local/riverside town").unwrap(), c);
    for bad in [
        "en/local/Riverside",
        "en/local",
        "en/world/x",
        "de/world",
        "en/top",
        "en/local/a/b",
    ] {
        assert!(NewsChannel::parse(bad).is_err(), "{bad}");
    }
    assert!(NewsChannel::topic(NewsLang::En, NewsTopic::Local).is_err());
    // Public, stable, and distinct per channel.
    assert_eq!(world_en().tag(), world_en().tag());
    assert_ne!(
        world_en().tag(),
        NewsChannel::topic(NewsLang::Fr, NewsTopic::World)
            .unwrap()
            .tag()
    );
    assert_ne!(
        NewsChannel::local(NewsLang::En, "Riverside").unwrap().tag(),
        NewsChannel::local(NewsLang::En, "Lakeport").unwrap().tag()
    );
}

#[test]
fn nw2_news_id_is_hash_of_source_and_link() {
    let items = fixture_items();
    let a = &items[0];
    // Same source + link from another phone (other fetch time, other summary): same id.
    let mut b = a.clone();
    b.summary = "Fetched elsewhere with another summary text.".into();
    assert_eq!(a.envelope_id(), b.envelope_id());
    assert_eq!(
        seal_news(a, 8).unwrap().envelope_id,
        seal_news(&b, 5).unwrap().envelope_id
    );
    assert_eq!(a.envelope_id(), news_envelope_id("gn", &a.link));
    assert_ne!(a.envelope_id(), news_envelope_id("gn", &items[1].link));
    assert_ne!(
        news_envelope_id("gn", &a.link),
        news_envelope_id("x", &a.link)
    );
}

#[test]
fn news_seal_open_round_trip_and_padding() {
    for it in fixture_items() {
        let env = seal_news(&it, 9).unwrap();
        assert!(is_news_envelope(&env));
        assert_eq!(env.created_ms, it.ts);
        assert_eq!(env.flags, flags::RELAY_OK | flags::STORE_OK);
        let padded = env.ciphertext.len() - NONCE_LEN - 16;
        assert!([256, 1024].contains(&padded), "§22.3 bucket: {padded}");
        assert!(env.encode().unwrap().len() <= 2048, "≤ 2 KB on the wire");
        assert_eq!(open_news(&env, NOW).unwrap(), it);
        assert_eq!(
            expires_from_aad(&env.header_aad).unwrap(),
            Some(it.ts + NEWS_EXPIRY_MS)
        );
    }
    // Expired at the receiver.
    let it = &fixture_items()[0];
    let env = seal_news(it, 9).unwrap();
    assert!(open_news(&env, it.ts + NEWS_EXPIRY_MS).is_err());
}

#[test]
fn news_publish_ban_rejects_items_not_from_the_rss_ingest_path() {
    let it = fixture_items()[0].clone();
    let ok = forge(&it, |_, _| {});
    assert!(
        open_news(&ok, NOW).is_ok(),
        "control: a faithful re-seal opens"
    );

    let cases: Vec<(&str, EnvelopeV1)> = vec![
        (
            "unregistered source",
            forge(&it, |i, e| {
                i.src = "user".into();
                e.envelope_id = news_envelope_id("user", &i.link);
            }),
        ),
        (
            "link not on the source host",
            forge(&it, |i, e| {
                i.link = "https://rumours.example/post/1".into();
                e.envelope_id = news_envelope_id("gn", &i.link);
            }),
        ),
        (
            "id not hash(source, link)",
            forge(&it, |_, e| e.envelope_id = [9; 16]),
        ),
        (
            "created_ms not the publication time",
            forge(&it, |_, e| e.created_ms += 1),
        ),
        (
            "published in the future",
            forge(&it, |i, e| {
                i.ts = NOW + MAX_CLOCK_SKEW_MS + 60_000;
                e.created_ms = i.ts;
            }),
        ),
        (
            "wrong channel topic",
            forge(&it, |i, _| i.topic = "sport".into()),
        ),
        (
            "citizen report topic",
            forge(&it, |i, _| i.topic = "reports".into()),
        ),
        (
            "priority flag",
            forge(&it, |_, e| e.flags |= flags::PRIORITY),
        ),
        (
            "expiry beyond 48 h",
            forge(&it, |i, e| {
                let mut aad = Vec::new();
                put_tag_in_aad(&mut aad, &world_en().tag());
                put_expires_in_aad(&mut aad, i.ts + 7 * 24 * 3_600_000);
                aad.extend_from_slice(&[AAD_TLV_NEWS_CHANNEL, 8]);
                aad.extend_from_slice(b"en/world");
                e.header_aad = aad;
            }),
        ),
        (
            "tag of another channel",
            forge(&it, |_, e| {
                let mut aad = Vec::new();
                put_tag_in_aad(
                    &mut aad,
                    &NewsChannel::topic(NewsLang::En, NewsTopic::Sport)
                        .unwrap()
                        .tag(),
                );
                put_expires_in_aad(&mut aad, it.ts + NEWS_EXPIRY_MS);
                aad.extend_from_slice(&[AAD_TLV_NEWS_CHANNEL, 8]);
                aad.extend_from_slice(b"en/world");
                e.header_aad = aad;
            }),
        ),
        ("empty title", forge(&it, |i, _| i.title.clear())),
    ];
    for (why, env) in cases {
        assert!(open_news(&env, NOW).is_err(), "{why}");
    }
    // Tampering after sealing breaks the AEAD.
    let mut t = seal_news(&it, 8).unwrap();
    let n = t.ciphertext.len();
    t.ciphertext[n - 1] ^= 1;
    assert!(open_news(&t, NOW).is_err());
    // A chat-like envelope on a news tag without the news TLV is not news.
    let mut chat = seal_news(&it, 8).unwrap();
    chat.header_aad.truncate(10 + 6);
    assert!(!is_news_envelope(&chat));
    assert!(open_news(&chat, NOW).is_err());
}

#[test]
fn news_emergencies_live_seven_days() {
    let ch = NewsChannel::topic(NewsLang::En, NewsTopic::Emergencies).unwrap();
    let mut it = fixture_items()[0].clone();
    it.topic = "emergencies".into();
    it.ts = NOW - 3 * 24 * 3_600_000;
    let env = seal_news(&it, 8).unwrap();
    assert_eq!(it.channel().unwrap(), ch);
    assert_eq!(
        expires_from_aad(&env.header_aad).unwrap(),
        Some(it.ts + EMERGENCY_EXPIRY_MS)
    );
    assert!(open_news(&env, NOW).is_ok());
}

#[test]
fn news_long_summary_is_trimmed_to_fit_one_kib() {
    let mut it = fixture_items()[2].clone();
    it.summary = "word ".repeat(80).trim().to_string();
    it.summary.truncate(MAX_SUMMARY);
    it.link = format!(
        "https://news.google.com/rss/articles/{}?oc=5",
        "C".repeat(500)
    );
    let env = seal_news(&it, 8).unwrap();
    assert_eq!(env.ciphertext.len(), NONCE_LEN + 1024 + 16);
    let back = open_news(&env, NOW).unwrap();
    assert!(back.summary.len() < it.summary.len());
}

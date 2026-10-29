# Hopla News (v0.4, experimental)

Spec: `apps/REQUIREMENTS.md` § "Hopla News" (draft, revised 2026-10-24); design §5R (v0.1.20).
PROTOCOL.md has no News section yet; wire choices are documented in `crates/omcp-core/src/news/mod.rs`.

## What it does
- **Source:** Google News RSS only (public feed URLs per language/topic/city, no API key).
  Sources are plain RSS URLs, so a later X news RSS feed needs no protocol change.
- **Fetch (gateway rules):** RSS is fetched only on phones with **Gateway** turned on, through
  `OmcpNode::news_fetch_plan`, which applies the gateway policy: Wi-Fi-only by default,
  metered networks warn (never blocked when the user allows mobile data), and fetched bytes count
  against a 500 MB/day cap. Each feed at most every 30 min; a failed fetch retries after 5 min.
- **Relay:** each story is one normal envelope on a public per-channel mailbox tag, relayed in the
  lowest relay class (after chats), outer expiry 48 h (Emergencies 7 d), ≤ 20 MB news storage, oldest first.
- **Dedup:** envelope id = hash(source ‖ link), so a story fetched by many phones is stored and relayed once.
- **No publishing:** the only producer is the RSS ingest path; relays and readers drop news envelopes
  that do not look like they came from it.
- **Share:** a story can be sent to a chat as a news card (`hopla-news/1` text, `news_card_text` /
  `parse_news_card`); replies under it are its thread.

## Requirement → tests
| ID | Tests |
|---|---|
| NW1 | ffi `nw1_offline_phone_catches_up_from_one_neighbour_within_60s`, `nw2_ingest_dedups_…` (catch-up part) |
| NW2 | core `nw2_news_id_is_hash_of_source_and_link`; ffi `nw2_ingest_dedups_by_source_and_link_and_relays_to_neighbours_once` |
| NW3 | core `nw3_news_is_classified_news_dropped_first_and_offered_after_chats`, `rl2_forwarding_priority_…`; ffi `nw3_news_rows_are_stored_in_the_news_class` |
| NW4 | core `nw4_news_storage_stays_within_its_cap_oldest_first`, `rl2_automatic_cap_drops_news_…` |
| NW5 | ffi `nw5_city_channel_subscribes_only_that_city` |
| NW6 | ffi `nw6_news_card_round_trips_through_a_chat_message` |
| Gateway rules | core `news_fetch_gate_wifi_only_metered_warning_and_daily_cap`; ffi `news_fetch_plan_applies_gateway_rules`; Android `NewsViewModelTest` |
| Prefs + byte cap persistence | ffi `news_prefs_and_fetch_byte_counter_survive_restart`; Android `NewsPersistenceTest` |
| Fetch role flag | ffi `news_fetch_role_is_one_flag`; Android `NewsPersistenceTest.fetchRoleIsTheRustFlag`; iOS `NewsFetchRulesTests` |
| No publish | core `news_publish_ban_*`; ffi `news_publish_ban_ingest_accepts_only_subscribed_google_news_feeds`; Android `tabsMatchDesignAndNoPublishApi` |

## Apps
- Bottom bar: Chats · News · Communities · You (Nearby is not a tab in these builds); Live
  ("Coming in v0.2") moved to the top of Communities.
- Android: News tab with chips, pinned emergency cards, reader with "Read on Google News", share to chat.
- iOS: News tab, reader, share to chat, News settings, and gateway fetching (`NewsFetchRules` in
  HoplaKit + `URLSession`, every 5 min while the News tab is open), mirroring Android.
- Settings (topics, languages, city, muted topics, emergency alerts) are stored by the Rust core in
  the encrypted store (`news_set_prefs`); Android reads/writes them through `NewsViewModel`.

## Spec gaps and decisions
- **Open product question:** REQUIREMENTS says "any phone with internet fetches; no dedicated gateway role".
  This build restricts fetching to Gateway-enabled phones. It is one constant,
  `NEWS_FETCH_GATEWAY_ONLY` in `crates/omcp-ffi/src/news.rs`, exposed as `news_may_fetch(gateway_enabled)`
  and used by both apps; flip it to let any phone with internet fetch (network rules still apply).
- NW1/NW3/NW5 timing is tested in-process only; the 60 s and chat-p95 claims are untested on real BLE hardware.
- NW6: Communities are "Coming in v0.2", so sharing goes to chats only; community threads/moderation are not implemented.
- The fetch byte counter `(day, bytes)` is persisted with the News record, so the 500 MB/day cap
  holds across restarts (`news_prefs_and_fetch_byte_counter_survive_restart`).
- Not done yet: "Use my location" (needs the Nearby location permission flow), the catch-up card
  (§5.116), skeleton loading cards, News onboarding step, Hebrew (draft copy, not shipped).
  iOS offers only English as a news language.
- No per-story signatures (spec rules them out): a modified client can forge an item that passes the ingest-shape checks.

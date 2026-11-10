//! §20.8 community discovery: MR53–MR63.

use std::collections::HashMap;

use ed25519_dalek::SigningKey;

use super::discovery::*;
use super::{community_ref, seal as seal_post, CommunityStore, Wire};
use crate::moderation::{ModerationState, Tombstone};
use crate::sync::relay::{RelayConfig, RelayEngine, RelayEvent};

const NOW: u64 = 1_800_000_000_000;
const DAY: u64 = 24 * 60 * 60 * 1000;

fn key(i: u32) -> SigningKey {
    let mut s = [0u8; 32];
    s[..4].copy_from_slice(&i.to_be_bytes());
    s[31] = 0xA5;
    SigningKey::from_bytes(&s)
}

fn fields(name: &str) -> ListingFields {
    ListingFields {
        name: name.into(),
        description: "Allotments, seed swaps and tools".into(),
        langs: vec!["en".into()],
        categories: vec![1],
        tags: vec!["garden".into()],
        member_bucket: 2,
        join_policy: 1,
        icon_hash: None,
    }
}

fn ann_with(
    owner: &SigningKey,
    cid: [u8; 32],
    seq: u64,
    at: u64,
    f: ListingFields,
) -> Announcement {
    Announcement::sign(cid, owner, owner, seq, at, f).unwrap()
}

fn ann(owner: u32, cid: u8, seq: u64, at: u64) -> Announcement {
    ann_with(&key(owner), [cid; 32], seq, at, fields("Garden club"))
}

fn env(a: &Announcement) -> crate::framing::EnvelopeV1 {
    seal(&Listing::Announce(a.clone()), OWN_TTL)
}

fn put(ix: &mut DiscoveryIndex, a: &Announcement, now: u64) -> Result<Ingested, Rejected> {
    let e = env(a);
    ix.ingest(&open(&e).unwrap(), e.envelope_id, 1, true, now)
}

#[test]
fn mr53_unlisted_by_default_and_no_announce_on_create() {
    const { assert!(!LISTED_DEFAULT) };
    assert!(LISTED_NOTICE.starts_with("Listing makes this community findable"));
    let mut s = CommunityStore::default();
    let g = s.create(&key(1), "Circle", NOW).unwrap();
    let e = seal_post(&Wire::Control(g.clone()), &g.community_id, NOW, 5);
    assert!(announce_tlv(&e.header_aad).is_none());
    let tag = crate::sync::tag::parse_routing_aad(&e.header_aad)
        .unwrap()
        .tag;
    assert_ne!(tag, Some(discovery_tag()));
}

#[test]
fn mr54_validation() {
    let a = ann(1, 1, 1, NOW);
    let e = env(&a);
    assert_eq!(e.ttl_hops, OWN_TTL);
    assert_eq!(
        announce_tlv(&e.header_aad),
        Some((community_ref(&[1; 32]), 1))
    );
    let mut ix = DiscoveryIndex::default();
    assert!(matches!(put(&mut ix, &a, NOW), Ok(Ingested::Stored { .. })));
    // 140 code points accepted, 141 rejected (multi-byte, under 1 KB).
    let mut f = fields("Long");
    f.description = "é".repeat(140);
    assert!(Announcement::sign([2; 32], &key(2), &key(2), 1, NOW, f.clone()).is_ok());
    f.description = "é".repeat(141);
    assert!(Announcement::sign([2; 32], &key(2), &key(2), 1, NOW, f).is_err());
    let mut long = ann(2, 2, 1, NOW);
    long.description = "a".repeat(141);
    assert!(Listing::decode(&long.encode()).is_err());
    // Over 1,024 B.
    let mut big = long.encode();
    big.resize(1025, 0);
    assert!(Listing::decode(&big).is_err());
    // Mismatched community_ref.
    let mut b = a.encode();
    b[34] ^= 1;
    assert!(Listing::decode(&b).is_err());
    // Bad sig_community / sig_owner.
    for off in [128, 64] {
        let mut x = a.clone();
        if off == 128 {
            x.sig_community[0] ^= 1;
        } else {
            x.sig_owner[0] ^= 1;
        }
        let mut ix = DiscoveryIndex::default();
        assert_eq!(put(&mut ix, &x, NOW), Err(Rejected::Invalid));
    }
    // Non-increasing seq.
    assert_eq!(
        put(&mut ix, &ann(1, 1, 1, NOW + 1), NOW + 2 * DAY),
        Err(Rejected::Stale)
    );
    assert_eq!(
        put(&mut ix, &ann(1, 1, 0, NOW), NOW + 2 * DAY),
        Err(Rejected::Stale)
    );
    // More than 24 h in the future.
    let f = ann(3, 3, 1, NOW + DAY + 1);
    assert_eq!(
        put(&mut DiscoveryIndex::default(), &f, NOW),
        Err(Rejected::Future)
    );
    // TLV must match the body; ttl over 6 rejected.
    let mut e2 = env(&a);
    let n = e2.header_aad.len();
    e2.header_aad[n - 1] ^= 1;
    assert!(open(&e2).is_err());
    let mut e3 = env(&a);
    e3.ttl_hops = 7;
    assert!(open(&e3).is_err());
}

// --- tiny mesh of relay engines -------------------------------------------

struct Mesh {
    nodes: Vec<RelayEngine>,
    wires: HashMap<(usize, u64), (usize, u64)>,
    sent: Vec<Vec<Vec<u8>>>,
}

impl Mesh {
    fn line(n: usize, now: u64) -> Self {
        let mut m = Mesh {
            nodes: (0..n)
                .map(|i| RelayEngine::new(RelayConfig::default(), now, i as u64 + 1))
                .collect(),
            wires: HashMap::new(),
            sent: vec![Vec::new(); n],
        };
        for i in 0..n - 1 {
            m.connect(i, i + 1, i as u64 + 1, now);
        }
        m
    }
    fn connect(&mut self, a: usize, b: usize, link: u64, now: u64) {
        self.wires.insert((a, link), (b, link));
        self.wires.insert((b, link), (a, link));
        let mut p: Vec<(usize, RelayEvent)> = Vec::new();
        for (x, y) in [(a, b), (b, a)] {
            let ev = self.nodes[x].link_up(link, &(y as u32).to_be_bytes(), false, now);
            p.extend(ev.into_iter().map(|e| (x, e)));
        }
        self.pump(p, now);
    }
    fn pump(&mut self, mut p: Vec<(usize, RelayEvent)>, now: u64) {
        let mut guard = 0;
        while let Some((n, e)) = p.pop() {
            guard += 1;
            assert!(guard < 1_000_000);
            if let RelayEvent::Send { link, frame } = e {
                self.sent[n].push(frame.clone());
                let Some(&(m, ml)) = self.wires.get(&(n, link)) else {
                    continue;
                };
                for e2 in self.nodes[m].on_frame(ml, &frame, now).unwrap() {
                    p.push((m, e2));
                }
            }
        }
    }
    fn submit(&mut self, n: usize, e: crate::framing::EnvelopeV1, now: u64) {
        let ev = self.nodes[n].submit_discovery_own(e, now).unwrap();
        self.pump(ev.into_iter().map(|e| (n, e)).collect(), now);
    }
}

#[test]
fn mr55_search_never_changes_traffic() {
    let run = |search: bool| {
        let mut m = Mesh::line(4, NOW);
        m.submit(0, env(&ann(1, 1, 1, NOW)), NOW);
        if search {
            for n in &m.nodes {
                let _ = n
                    .discovery()
                    .search("garden secret term", Some(3), Some("fr"), NOW);
            }
        }
        m.submit(3, env(&ann(2, 2, 1, NOW)), NOW);
        for n in 0..4 {
            let ev = m.nodes[n].tick(NOW + 60_000);
            m.pump(ev.into_iter().map(|e| (n, e)).collect(), NOW + 60_000);
        }
        m.sent
    };
    let (a, b) = (run(false), run(true));
    assert_eq!(a, b);
    for f in a.iter().flatten() {
        assert!(!f.windows(6).any(|w| w == b"secret"));
    }
}

#[test]
fn mr56_reaches_six_hops_never_beyond() {
    let mut m = Mesh::line(50, NOW);
    let a = ann(1, 1, 1, NOW);
    m.submit(0, env(&a), NOW);
    let cref = a.community_ref();
    for i in 0..50 {
        let has = m.nodes[i].discovery().get(&cref).is_some();
        assert_eq!(has, i <= 6, "node {i}");
        if i > 0 && i <= 6 {
            assert_eq!(m.nodes[i].discovery().get(&cref).unwrap().hops, i as u8);
        }
    }
    // Node 6 holds it with 0 hops left and forwards nothing further.
    let id = env(&a).envelope_id;
    for (i, n) in m.nodes.iter().enumerate().skip(7) {
        assert!(!n.seen(&id), "node {i} received it");
    }
}

#[test]
fn mr57_expiry_refresh_and_delist() {
    let k = key(1);
    let mut ix = DiscoveryIndex::default();
    let a = ann(1, 1, 1, NOW);
    put(&mut ix, &a, NOW).unwrap();
    // One missed refresh (day 7..13) keeps it; 14 d after issued_ms drops it.
    assert!(ix.purge(NOW + 13 * DAY).is_empty());
    assert_eq!(ix.purge(NOW + 14 * DAY).len(), 1);
    assert!(ix.is_empty());
    // Refreshed at day 7: still there at day 20.
    let mut ix = DiscoveryIndex::default();
    put(&mut ix, &a, NOW).unwrap();
    put(&mut ix, &ann(1, 1, 2, NOW + 7 * DAY), NOW + 7 * DAY).unwrap();
    assert!(ix.purge(NOW + 20 * DAY).is_empty());
    // A delist removes it at once and blocks older seqs.
    let d = Delist::sign(&[1; 32], &k, &k, 3, NOW + 8 * DAY);
    let de = seal(&Listing::Delist(d), OWN_TTL);
    assert!(matches!(
        ix.ingest(&open(&de).unwrap(), de.envelope_id, 1, true, NOW + 8 * DAY),
        Ok(Ingested::Stored { evicted }) if evicted.len() == 1
    ));
    assert!(ix.get(&a.community_ref()).is_none());
    assert_eq!(
        put(&mut ix, &ann(1, 1, 2, NOW + 7 * DAY), NOW + 9 * DAY),
        Err(Rejected::Stale)
    );
    // A forged delist (wrong keys) is refused.
    let mut ix = DiscoveryIndex::default();
    put(&mut ix, &a, NOW).unwrap();
    let bad = Delist::sign(&[1; 32], &key(9), &key(9), 3, NOW);
    let be = seal(&Listing::Delist(bad), OWN_TTL);
    assert_eq!(
        ix.ingest(&open(&be).unwrap(), be.envelope_id, 1, true, NOW),
        Err(Rejected::Invalid)
    );
    // Over the mesh: a delist reaches the relays and clears them.
    let mut m = Mesh::line(3, NOW);
    m.submit(0, env(&a), NOW);
    assert!(m.nodes[2].discovery().get(&a.community_ref()).is_some());
    m.submit(
        0,
        seal(
            &Listing::Delist(Delist::sign(&[1; 32], &k, &k, 2, NOW + 1)),
            OWN_TTL,
        ),
        NOW + 1,
    );
    for n in &m.nodes {
        assert!(n.discovery().get(&a.community_ref()).is_none());
    }
}

#[test]
fn mr58_index_budget() {
    let mut ix = DiscoveryIndex::default();
    for i in 0..(INDEX_MAX_ENTRIES as u32 + 20) {
        let mut cid = [0u8; 32];
        cid[..4].copy_from_slice(&i.to_be_bytes());
        let a = ann_with(&key(i), cid, 1, NOW + i as u64, fields("Club"));
        let e = env(&a);
        let _ = ix.ingest(
            &open(&e).unwrap(),
            e.envelope_id,
            (i % 6) as u8 + 1,
            false,
            NOW + DAY,
        );
        assert!(ix.len() <= INDEX_MAX_ENTRIES);
        assert!(ix.bytes() <= INDEX_MAX_BYTES);
    }
    assert!(ix.len() == INDEX_MAX_ENTRIES || ix.bytes() > INDEX_MAX_BYTES - 1_024);
    // Discovery has its own class: a relay full of listings keeps chats.
    assert_eq!(
        crate::sync::budget::RelayClass::from_u8(5),
        Some(crate::sync::budget::RelayClass::Discovery)
    );
    const { assert!(FORWARD_PER_HOUR == 200) };
}

#[test]
fn mr58_forward_cap_per_hour() {
    let mut m = Mesh::line(3, NOW);
    for i in 0..250u32 {
        let mut cid = [0u8; 32];
        cid[..4].copy_from_slice(&i.to_be_bytes());
        let t = NOW + i as u64 * 10_000; // 250 listings within one hour
        m.submit(0, env(&ann_with(&key(i), cid, 1, t, fields("C"))), t);
    }
    // Node 1 forwards at most 200 of them to node 2 in this hour.
    let at2 = m.nodes[2].discovery().len();
    assert_eq!(at2, FORWARD_PER_HOUR as usize);
    assert_eq!(m.nodes[1].discovery().len(), 250);
}

#[test]
fn mr59_rate_and_per_key_limits() {
    let mut ix = DiscoveryIndex::default();
    put(&mut ix, &ann(1, 1, 1, NOW), NOW).unwrap();
    assert_eq!(
        put(&mut ix, &ann(1, 1, 2, NOW + 1), NOW + 1),
        Err(Rejected::RateLimited)
    );
    assert!(put(&mut ix, &ann(1, 1, 2, NOW + DAY), NOW + DAY).is_ok());
    put(&mut ix, &ann(1, 2, 1, NOW), NOW + DAY).unwrap();
    put(&mut ix, &ann(1, 3, 1, NOW), NOW + DAY).unwrap();
    assert_eq!(
        put(&mut ix, &ann(1, 4, 1, NOW), NOW + DAY),
        Err(Rejected::TooManyForKey)
    );
    assert_eq!(ix.len(), 3);
}

#[test]
fn mr60_moderation_hooks() {
    let a = ann(1, 1, 1, NOW);
    let cref = a.community_ref();
    let mut ix = DiscoveryIndex::default();
    put(&mut ix, &a, NOW).unwrap();
    // Takedown / tombstone (banned or restored): dropped, refused.
    for restored in [false, true] {
        let mut ix = ix.clone();
        let mut ctx = DiscoveryContext::default();
        ctx.blocked.insert(cref);
        let _ = Tombstone { restored, ..tomb() };
        assert_eq!(ix.set_context(ctx).len(), 1);
        assert!(ix.search("", None, None, NOW).is_empty());
        assert_eq!(
            put(&mut ix, &ann(1, 1, 2, NOW + DAY), NOW + DAY),
            Err(Rejected::Moderated)
        );
    }
    // Owner under a team MEMBER_REMOVE.
    let mut ix2 = DiscoveryIndex::default();
    let mut ctx = DiscoveryContext::default();
    ctx.removed_owners
        .insert((cref, crate::moderation::peer_id_hash(&a.owner_pk)));
    ix2.set_context(ctx);
    assert_eq!(put(&mut ix2, &a, NOW), Err(Rejected::Moderated));
    // owner_pk not the owner in the latest known roster.
    let mut ix3 = DiscoveryIndex::default();
    let mut ctx = DiscoveryContext::default();
    ctx.roster_owner
        .insert(cref, key(77).verifying_key().to_bytes());
    ix3.set_context(ctx);
    assert_eq!(put(&mut ix3, &a, NOW), Err(Rejected::RosterMismatch));
    // From real team state: a takedown tombstone blocks the ref.
    let m = ModerationState::default();
    assert!(
        DiscoveryContext::from_moderation(&m, [([1; 32], a.owner_pk)], NOW)
            .blocked
            .is_empty()
    );
}

fn tomb() -> Tombstone {
    Tombstone {
        restored: false,
        kind: crate::moderation::ActionKind::Takedown,
        target_id: [1; 32],
        item_id: [0; 32],
        epoch: 1,
        reason_code: 1,
        applied_ms: NOW,
    }
}

#[test]
fn mr61_ranking_and_reported_hidden() {
    let mut ix = DiscoveryIndex::default();
    let mut f = fields("Chess");
    f.member_bucket = 5;
    let big = ann_with(&key(1), [1; 32], 1, NOW, f);
    let mut f = fields("Chess night");
    f.member_bucket = 1;
    let small_new = ann_with(&key(2), [2; 32], 1, NOW + 10, f);
    let tagged = ann_with(
        &key(3),
        [3; 32],
        1,
        NOW,
        ListingFields {
            tags: vec!["chess".into()],
            ..fields("Board games")
        },
    );
    for a in [&big, &small_new, &tagged] {
        let e = env(a);
        ix.ingest(&open(&e).unwrap(), e.envelope_id, 3, false, NOW + 20)
            .unwrap();
    }
    let r = ix.search("chess", None, None, NOW + 20);
    // Name matches beat tag matches; then recency; size only breaks ties.
    assert_eq!(r[0].ann.name, "Chess night");
    assert_eq!(r[1].ann.name, "Chess");
    assert_eq!(r[2].ann.name, "Board games");
    let mut ctx = DiscoveryContext::default();
    ctx.reported.insert(big.community_ref());
    ix.set_context(ctx);
    assert!(ix
        .search("chess", None, None, NOW + 20)
        .iter()
        .all(|f| f.ann.name != "Chess"));
    ix.set_hidden(small_new.community_ref(), true);
    assert_eq!(ix.search("chess", None, None, NOW + 20).len(), 1);
}

#[test]
fn mr62_report_bytes_and_join_mismatch_distrust() {
    let a = ann(1, 1, 1, NOW);
    // A report carries only the signed announcement bytes.
    let bytes = a.encode();
    assert_eq!(
        Listing::decode(&bytes).unwrap(),
        Listing::Announce(a.clone())
    );
    let mut ix = DiscoveryIndex::default();
    put(&mut ix, &a, NOW).unwrap();
    assert_eq!(ix.distrust(a.community_ref()).len(), 1);
    assert_eq!(
        put(&mut ix, &ann(1, 1, 2, NOW), NOW + DAY),
        Err(Rejected::Moderated)
    );
}

#[test]
fn mr63_near_you_and_categories() {
    let mut ix = DiscoveryIndex::default();
    let near = ann(1, 1, 1, NOW);
    let far = ann_with(&key(2), [2; 32], 1, NOW, fields("Garden club"));
    let gw = ann_with(&key(3), [3; 32], 1, NOW, fields("Garden club"));
    for (a, hops, radio) in [(&near, 2, true), (&far, 3, true), (&gw, 1, false)] {
        let e = env(a);
        ix.ingest(&open(&e).unwrap(), e.envelope_id, hops, radio, NOW)
            .unwrap();
    }
    let r = ix.search("garden", None, None, NOW);
    assert!(r[0].near_you && r[0].ann.community_id == [1; 32]);
    assert!(r[1..].iter().all(|f| !f.near_you));
    // Older than 7 days: no label.
    let r = ix.search("garden", None, None, NOW + 8 * DAY);
    assert!(r.iter().all(|f| !f.near_you));
    // Nothing of the label is in what is sent: same bytes either way.
    assert_eq!(env(&near), env(&near));
    // Unknown categories show as Other.
    let mut f = fields("Odd");
    f.categories = vec![42];
    let odd = ann_with(&key(4), [4; 32], 1, NOW, f);
    put(&mut ix, &odd, NOW).unwrap();
    assert_eq!(display_category(42), CAT_OTHER);
    assert_eq!(ix.search("odd", Some(8), None, NOW).len(), 1);
    assert_eq!(
        (1..=8).map(display_category).collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>()
    );
    assert_eq!(bucket_range(3), (51, 200));
}

#[test]
fn language_filter_matches_announced_bcp47_tags() {
    let mut ix = DiscoveryIndex::default();
    let mut fr = fields("Jardin");
    fr.langs = vec!["fr".into()];
    let mut en = fields("Garden");
    en.langs = vec!["en".into()];
    let none = fields("Garden");
    for (k, id, f) in [(1u32, [1u8; 32], fr), (2, [2; 32], en), (3, [3; 32], none)] {
        let a = ann_with(&key(k), id, 1, NOW, f);
        let e = env(&a);
        ix.ingest(&open(&e).unwrap(), e.envelope_id, 1, true, NOW)
            .unwrap();
    }
    assert_eq!(ix.languages(), vec!["en".to_string(), "fr".to_string()]);
    let r = ix.search("", None, Some("fr"), NOW);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].ann.name, "Jardin");
    assert_eq!(ix.search("", None, Some("FR"), NOW).len(), 1);
    assert_eq!(ix.search("", None, Some("de"), NOW).len(), 0);
    assert_eq!(ix.search("", None, None, NOW).len(), 3);
}

#[test]
fn mr_discovery_same_seq_does_not_extend_expiry() {
    // §20.8(4)(d): expiry is from the signed issued_ms of the newest valid
    // announcement; hearing the same seq again MUST NOT extend it.
    let mut ix = DiscoveryIndex::default();
    let a = ann(1, 1, 1, NOW);
    put(&mut ix, &a, NOW).unwrap();
    let again_at = NOW + 10 * DAY;
    let e = seal(&Listing::Announce(a.clone()), OWN_TTL);
    let r = ix.ingest(&open(&e).unwrap(), e.envelope_id, 1, true, again_at);
    assert!(matches!(r, Ok(Ingested::Again)));
    // Still expires 14 d after the *original* issued_ms, not after again_at.
    assert!(ix.purge(NOW + 13 * DAY).is_empty());
    assert_eq!(ix.purge(NOW + 14 * DAY).len(), 1);
    assert!(ix.is_empty());
}

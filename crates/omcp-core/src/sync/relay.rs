//! Mesh relay engine: §22 sync wired onto BLE gossip (PROTOCOL §8.3, §8.4,
//! §22.1, §22.4, §22.7(1), (9), (11), (12)) — **experimental (v0.2)**.
//!
//! Transport-agnostic and clock-injected: the caller owns the links (one
//! handle per authenticated, link-encrypted neighbour session) and feeds every
//! *mesh frame* it receives into [`RelayEngine::on_frame`]. The engine answers
//! with [`RelayEvent`]s: frames to send on a link, envelopes addressed to the
//! local user, and carry-store changes to persist.
//!
//! The engine only ever handles opaque [`EnvelopeV1`]s. It routes on the
//! `header_aad` TLVs (`0x54` chat tag, `0x56` recipient sub-tags) and never
//! needs, or gets, a key that opens a ciphertext: relays never see plaintext.
//!
//! # Mesh frames (inside link PDU `0x03` `MESH_FRAME`, §21.9 / §22.8(2))
//!
//! | first byte | frame | |
//! |---|---|---|
//! | `0x48` | `HELLO` (6 B) | §22.7(12) |
//! | `0x52` | `DIGEST_REQ` (6 B) | §22.7(12) |
//! | `0x44` | `DATA`: `0x44 ‖ EnvelopeV1::encode()` | §22.8(2) |
//! | `0x45` | `DELIVERY_ACK`: `0x45 ‖ msg_hash[0..16] ‖ EnvelopeV1::encode()` | §22.8(2), (3) |
//! | `0x64` | `DIGEST_DELTA`: `0x64 ‖ mode ‖ body` | §22.8(2) |
//!
//! `DIGEST_DELTA` mode `0x00` = `DigestDelta::encode()`, mode `0x01` = the
//! full `CuckooFilter::encode()` (link-up / resync). §22.8(2) assigns no
//! separate full-digest type; the mode byte is this implementation's
//! convention (spec gap).
//!
//! `DELIVERY_ACK`: the 16-byte `msg_hash` (first 16 B of BLAKE2b-256 over the
//! acked envelope's ciphertext) is in the clear so relays can delete the
//! acked envelope; the attached envelope carries the end-to-end MACed copy
//! (MLS application message to the original sender's sub-tag) and is routed
//! like `DATA`. Relays cannot authenticate the clear hash, so they only drop
//! envelopes for others, never the original sender's own copy.
//!
//! # Behaviour
//!
//! - **Link up**: full digest + `HELLO` to the new neighbour (§22.7(11)).
//! - **Digests**: on change, a delta followed by `HELLO{HAS_DELTA}` to every
//!   neighbour; a `HELLO` whose `digest_id` does not match our copy of that
//!   neighbour's digest triggers `DIGEST_REQ(0)` (full).
//! - **Forwarding**: [`plan_forward`] — flood up to 32 visible peers, above that
//!   want > lowest carry distance > scoped flood within `ttl_hops`. Each
//!   forward costs one hop. A peer is never sent the same envelope twice.
//! - **Carry store**: envelopes for others are kept (store-and-forward, §8.4)
//!   under the §22.4 [`RelayStore`] budget and eviction order, expiring at the
//!   envelope's `outer_expiry` (TLV `0x58`, §22.8(2)) or `created + 72 h`,
//!   never later than 7 days. Our own outbound envelopes are kept outside the
//!   relay budget until they expire.
//! - **Dedup**: a seen set keyed by `envelope_id` that lives as long as the
//!   longest possible envelope lifetime (7 days, capped in size).
//! - **Rate limit**: one token bucket per neighbour (64 envelopes/s, §22.4).
//! - **Relay off**: own envelopes are still sent and our own are still
//!   delivered; nothing is stored or forwarded for others.
//!
//! `outer_expiry` on the wire: `header_aad` TLV `0x58` (len 4, u32 BE unix
//! seconds, §21.8). Relays drop expired envelopes and never keep one longer
//! than now + 7 d (a later `outer_expiry` is clamped, not rejected).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::budget::{
    InsertOutcome, RelayBudget, RelayStore, StoredEnvelope, DEFAULT_EXPIRY_MS, MAX_EXPIRY_MS,
};
use super::cuckoo::{CuckooFilter, DigestDelta};
use super::hello::{digest_id32, HelloTimer, LinkControl, DIGEST_ID_FULL, DIGEST_REFRESH_MS};
use super::routing::{
    adaptive_ttl, plan_forward, CarryTable, ForwardPlan, HyperLogLog, TokenBucket,
};
use super::tag::{parse_routing_aad, MailboxTag, MAX_CARRY_DISTANCE};
use super::SplitMix64;
use crate::error::{OmcpError, Result};
use crate::framing::EnvelopeV1;
use crate::gateway::policy::NetworkKind;

/// `DATA` mesh frame (one envelope).
pub const FRAME_DATA: u8 = 0x44;
/// `DELIVERY_ACK` mesh frame.
pub const FRAME_DELIVERY_ACK: u8 = 0x45;
/// `DIGEST_DELTA` mesh frame.
pub const FRAME_DIGEST_DELTA: u8 = 0x64;
/// `DIGEST_DELTA` body mode: delta against the receiver's copy.
pub const DELTA_MODE_DELTA: u8 = 0x00;
/// `DIGEST_DELTA` body mode: full filter.
pub const DELTA_MODE_FULL: u8 = 0x01;
/// `header_aad` TLV `outer_expiry` (u32 BE unix seconds, §21.8).
pub const AAD_TLV_OUTER_EXPIRY: u8 = 0x58;
/// Length of `msg_hash` in a `DELIVERY_ACK`.
pub const MSG_HASH_LEN: usize = 16;
/// Minimum padded size of our want-digest (decoys hide the real count).
pub const DIGEST_MIN_ENTRIES: usize = 32;
/// Dedup memory cap (entries).
pub const MAX_SEEN: usize = 16_384;

/// Relay settings (user-facing toggle + storage cap).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayConfig {
    pub enabled: bool,
    pub budget: RelayBudget,
}

impl Default for RelayConfig {
    /// Relay on with the §22.4 battery/cellular budget (50 MB, 20 MB/day).
    fn default() -> Self {
        Self {
            enabled: true,
            budget: RelayBudget::for_conditions(NetworkKind::Cellular { metered: true }, false),
        }
    }
}

/// One stored envelope (relayed for others, or our own outbound).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarryItem {
    pub envelope: EnvelopeV1,
    pub expires_at: u64,
    pub hop_count: u8,
    /// Our own outbound envelope (not counted against the relay budget).
    pub own: bool,
    /// Set for a `DELIVERY_ACK` envelope: the acked `msg_hash`.
    pub ack_of: Option<[u8; MSG_HASH_LEN]>,
}

/// What the caller must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayEvent {
    /// Send `frame` on link `link` (inside its link AEAD).
    Send { link: u64, frame: Vec<u8> },
    /// An envelope addressed to one of our wanted tags arrived on `link`.
    Deliver {
        link: u64,
        tag: MailboxTag,
        envelope: EnvelopeV1,
        /// `Some(msg_hash)` when it arrived as a `DELIVERY_ACK`.
        ack_of: Option<[u8; MSG_HASH_LEN]>,
    },
    /// Our own envelope was handed to a neighbour for the first time.
    Handed([u8; 16]),
    /// Persist (or overwrite) this carry item.
    Stored(CarryItem),
    /// Drop this carry item from persistent storage (evicted / expired).
    Removed([u8; 16]),
}

#[derive(Debug)]
struct Neighbour {
    peer: Vec<u8>,
    digest: Option<CuckooFilter>,
    bucket: TokenBucket,
}

/// Snapshot counters for settings / diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RelayStats {
    pub carried: u32,
    pub carried_bytes: u64,
    pub own_pending: u32,
    pub neighbours: u32,
    pub forwarded_today_bytes: u64,
}

/// The per-device mesh relay.
#[derive(Debug)]
pub struct RelayEngine {
    cfg: RelayConfig,
    rng: SplitMix64,
    wants: BTreeSet<MailboxTag>,
    carry: CarryTable,
    advertised: BTreeSet<MailboxTag>,
    digest: CuckooFilter,
    hello: HelloTimer,
    last_full_ms: u64,
    store: RelayStore,
    items: HashMap<[u8; 16], CarryItem>,
    neighbours: BTreeMap<u64, Neighbour>,
    sent_to: HashMap<[u8; 16], HashSet<Vec<u8>>>,
    seen: HashMap<[u8; 16], u64>,
    /// Acked `msg_hash` → keep-until (drop later copies).
    acked: HashMap<[u8; MSG_HASH_LEN], u64>,
    hll: HyperLogLog,
    /// Tests: pretend the mesh has this many visible peers.
    visible_override: Option<usize>,
}

fn frame_with(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + body.len());
    v.push(kind);
    v.extend_from_slice(body);
    v
}

/// `msg_hash` (§22.8(3)): first 16 bytes of BLAKE2b-256 over the ciphertext.
pub fn msg_hash(ciphertext: &[u8]) -> [u8; MSG_HASH_LEN] {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(32).expect("32 is a valid BLAKE2b output size");
    h.update(ciphertext);
    let mut out = [0u8; 32];
    h.finalize_variable(&mut out).expect("output length");
    let mut m = [0u8; MSG_HASH_LEN];
    m.copy_from_slice(&out[..MSG_HASH_LEN]);
    m
}

fn full_digest_frame(f: &CuckooFilter) -> Vec<u8> {
    let mut v = vec![FRAME_DIGEST_DELTA, DELTA_MODE_FULL];
    v.extend_from_slice(&f.encode());
    v
}

/// Outer expiry (ms) from `header_aad` TLV `0x58`, if present.
pub fn expires_from_aad(aad: &[u8]) -> Result<Option<u64>> {
    let mut i = 0;
    while i + 2 <= aad.len() {
        let (t, l) = (aad[i], aad[i + 1] as usize);
        let v = aad
            .get(i + 2..i + 2 + l)
            .ok_or(OmcpError::Sync("aad tlv truncated"))?;
        if t == AAD_TLV_OUTER_EXPIRY {
            let b: [u8; 4] = v
                .try_into()
                .map_err(|_| OmcpError::Sync("outer_expiry tlv length"))?;
            return Ok(Some(u64::from(u32::from_be_bytes(b)) * 1000));
        }
        i += 2 + l;
    }
    Ok(None)
}

/// Append TLV `0x58` (`outer_expiry`) for `expires_at_ms` (rounded down to
/// whole seconds, saturating at `u32::MAX`).
pub fn put_expires_in_aad(aad: &mut Vec<u8>, expires_at_ms: u64) {
    let secs = u32::try_from(expires_at_ms / 1000).unwrap_or(u32::MAX);
    aad.push(AAD_TLV_OUTER_EXPIRY);
    aad.push(4);
    aad.extend_from_slice(&secs.to_be_bytes());
}

/// Effective store expiry: the envelope's own `expires_at`, else
/// `created + 72 h`; never beyond `now + 7 d`.
pub fn effective_expiry(env: &EnvelopeV1, now_ms: u64) -> Result<u64> {
    let req = expires_from_aad(&env.header_aad)?
        .unwrap_or(env.created_ms.saturating_add(DEFAULT_EXPIRY_MS));
    Ok(req.min(now_ms.saturating_add(MAX_EXPIRY_MS)))
}

impl RelayEngine {
    pub fn new(cfg: RelayConfig, now_ms: u64, seed: u64) -> Self {
        let mut rng = SplitMix64(seed);
        let digest = CuckooFilter::build_digest(&[], DIGEST_MIN_ENTRIES, &mut rng)
            .expect("empty digest always fits");
        Self {
            cfg,
            rng,
            wants: BTreeSet::new(),
            carry: CarryTable::default(),
            advertised: BTreeSet::new(),
            digest,
            hello: HelloTimer::new(now_ms),
            last_full_ms: now_ms,
            store: RelayStore::new(cfg.budget),
            items: HashMap::new(),
            neighbours: BTreeMap::new(),
            sent_to: HashMap::new(),
            seen: HashMap::new(),
            acked: HashMap::new(),
            hll: HyperLogLog::default(),
            visible_override: None,
        }
    }

    pub fn config(&self) -> RelayConfig {
        self.cfg
    }

    /// Test hook: force the visible-peer count used by the forwarding rule.
    pub fn set_visible_peers_override(&mut self, n: Option<usize>) {
        self.visible_override = n;
    }

    /// Change settings; disabling the relay drops everything carried for
    /// others, a smaller cap evicts per §22.4.
    pub fn set_config(&mut self, cfg: RelayConfig) -> Vec<RelayEvent> {
        self.cfg = cfg;
        let mut ev = Vec::new();
        if cfg.enabled {
            for id in self.store.set_budget(cfg.budget) {
                self.items.remove(&id);
                self.sent_to.remove(&id);
                ev.push(RelayEvent::Removed(id));
            }
        } else {
            let others: Vec<[u8; 16]> = self
                .items
                .iter()
                .filter(|(_, i)| !i.own)
                .map(|(k, _)| *k)
                .collect();
            for id in others {
                self.items.remove(&id);
                self.store.take(&id);
                self.sent_to.remove(&id);
                ev.push(RelayEvent::Removed(id));
            }
            self.store.set_budget(cfg.budget);
        }
        ev
    }

    fn meta(it: &CarryItem) -> StoredEnvelope {
        StoredEnvelope {
            id: it.envelope.envelope_id,
            size: it
                .envelope
                .encode()
                .map(|b| b.len() as u64)
                .unwrap_or(u64::MAX),
            priority: false,
            expires_at: it.expires_at,
            hop_count: it.hop_count,
        }
    }

    pub fn stats(&self) -> RelayStats {
        RelayStats {
            carried: self.items.values().filter(|i| !i.own).count() as u32,
            carried_bytes: self.store.used(),
            own_pending: self.items.values().filter(|i| i.own).count() as u32,
            neighbours: self.neighbours.len() as u32,
            forwarded_today_bytes: self.store.forwarded_today(),
        }
    }

    /// Adaptive TTL for a new envelope (§22.1), from the HyperLogLog estimate.
    pub fn new_ttl(&self) -> u8 {
        adaptive_ttl(self.hll.estimate().max(self.neighbours.len() as f64 + 1.0))
    }

    /// Whether `id` was already seen (delivered, carried or sent).
    pub fn seen(&self, id: &[u8; 16]) -> bool {
        self.seen.contains_key(id)
    }

    /// Remember `id` (e.g. delivered over a direct link) until `until_ms`.
    pub fn mark_seen(&mut self, id: [u8; 16], until_ms: u64) {
        if self.seen.len() >= MAX_SEEN {
            if let Some(old) = self.seen.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k) {
                self.seen.remove(&old);
            }
        }
        let e = self.seen.entry(id).or_insert(until_ms);
        *e = (*e).max(until_ms);
    }

    /// Reload persisted state (no events).
    pub fn restore(&mut self, item: CarryItem, now_ms: u64) {
        if item.expires_at <= now_ms {
            return;
        }
        let id = item.envelope.envelope_id;
        if !item.own {
            if !self.cfg.enabled {
                return;
            }
            if !matches!(
                self.store.insert(Self::meta(&item), now_ms),
                InsertOutcome::Stored { .. }
            ) {
                return;
            }
        }
        self.mark_seen(id, item.expires_at);
        self.items.insert(id, item);
    }

    /// Ids of carried / own items.
    pub fn item_ids(&self) -> Vec<[u8; 16]> {
        self.items.keys().copied().collect()
    }

    pub fn item(&self, id: &[u8; 16]) -> Option<&CarryItem> {
        self.items.get(id)
    }

    /// Our own outbound copy was delivered directly: stop offering it.
    pub fn cancel_own(&mut self, id: &[u8; 16]) -> Vec<RelayEvent> {
        match self.items.get(id) {
            Some(it) if it.own => {
                self.items.remove(id);
                self.sent_to.remove(id);
                vec![RelayEvent::Removed(*id)]
            }
            _ => Vec::new(),
        }
    }

    /// Replace the set of tags we want (our own sub-tags). Broadcasts a
    /// digest change.
    pub fn set_wants(&mut self, wants: BTreeSet<MailboxTag>, now_ms: u64) -> Vec<RelayEvent> {
        self.wants = wants;
        self.refresh_digest(now_ms)
    }

    pub fn wants(&self, tag: &MailboxTag) -> bool {
        self.wants.contains(tag)
    }

    /// Current `HELLO` beacon (also usable as BLE advertisement service data).
    pub fn beacon(&self) -> [u8; 6] {
        LinkControl::hello_for(&self.digest, false).encode()
    }

    /// Our current want-digest.
    pub fn digest(&self) -> &CuckooFilter {
        &self.digest
    }

    fn visible(&self) -> usize {
        self.visible_override
            .unwrap_or_else(|| self.hll.estimate().round() as usize + 1)
    }

    /// A new authenticated neighbour session.
    pub fn link_up(&mut self, link: u64, peer: &[u8], now_ms: u64) -> Vec<RelayEvent> {
        self.hll.insert(peer);
        self.neighbours.insert(
            link,
            Neighbour {
                peer: peer.to_vec(),
                digest: None,
                bucket: TokenBucket::default_neighbour(now_ms),
            },
        );
        self.hello.set_neighbours(self.neighbours.len());
        self.hello.on_neighbourhood_change(now_ms);
        vec![
            RelayEvent::Send {
                link,
                frame: full_digest_frame(&self.digest),
            },
            RelayEvent::Send {
                link,
                frame: self.beacon().to_vec(),
            },
        ]
    }

    /// Frames for `link` that were queued but never transmitted (the link
    /// dropped first): forget that the neighbour has these envelopes so they
    /// are offered again next time. Call before [`Self::link_down`].
    pub fn unsent(&mut self, link: u64, frames: &[Vec<u8>]) {
        let Some(peer) = self.neighbours.get(&link).map(|n| n.peer.clone()) else {
            return;
        };
        for f in frames {
            let body = match f.split_first() {
                Some((&FRAME_DATA, b)) => b,
                Some((&FRAME_DELIVERY_ACK, b)) if b.len() >= MSG_HASH_LEN => &b[MSG_HASH_LEN..],
                _ => continue,
            };
            if let Ok(env) = EnvelopeV1::decode(body) {
                if let Some(s) = self.sent_to.get_mut(&env.envelope_id) {
                    s.remove(&peer);
                }
            }
        }
    }

    pub fn link_down(&mut self, link: u64, now_ms: u64) {
        if self.neighbours.remove(&link).is_some() {
            self.hello.set_neighbours(self.neighbours.len());
            self.hello.on_neighbourhood_change(now_ms);
        }
    }

    /// Hand our own sealed envelope to the mesh.
    pub fn submit_own(&mut self, envelope: EnvelopeV1, now_ms: u64) -> Result<Vec<RelayEvent>> {
        self.submit(envelope, None, now_ms)
    }

    /// Send our `DELIVERY_ACK` for `msg_hash` (the envelope carries the
    /// end-to-end MACed ack to the original sender's sub-tag).
    pub fn submit_ack(
        &mut self,
        envelope: EnvelopeV1,
        msg_hash: [u8; MSG_HASH_LEN],
        now_ms: u64,
    ) -> Result<Vec<RelayEvent>> {
        let mut ev = Vec::new();
        self.note_ack(&msg_hash, now_ms, &mut ev);
        ev.extend(self.submit(envelope, Some(msg_hash), now_ms)?);
        Ok(ev)
    }

    fn submit(
        &mut self,
        envelope: EnvelopeV1,
        ack_of: Option<[u8; MSG_HASH_LEN]>,
        now_ms: u64,
    ) -> Result<Vec<RelayEvent>> {
        parse_routing_aad(&envelope.header_aad)?;
        let expires_at = effective_expiry(&envelope, now_ms)?;
        let id = envelope.envelope_id;
        let item = CarryItem {
            envelope,
            expires_at,
            hop_count: 0,
            own: true,
            ack_of,
        };
        self.mark_seen(id, expires_at);
        self.items.insert(id, item.clone());
        let mut ev = vec![RelayEvent::Stored(item)];
        self.forward(&id, None, now_ms, &mut ev);
        Ok(ev)
    }

    /// Periodic work: hello beacons, expiry, 30-min digest refresh.
    pub fn tick(&mut self, now_ms: u64) -> Vec<RelayEvent> {
        let mut ev = Vec::new();
        let expired: Vec<[u8; 16]> = self
            .items
            .iter()
            .filter(|(_, i)| i.expires_at <= now_ms)
            .map(|(k, _)| *k)
            .collect();
        for id in expired {
            if let Some(it) = self.items.remove(&id) {
                if !it.own {
                    self.store.take(&id);
                }
            }
            self.sent_to.remove(&id);
            ev.push(RelayEvent::Removed(id));
        }
        self.seen.retain(|_, t| *t > now_ms);
        self.acked.retain(|_, t| *t > now_ms);
        ev.extend(self.refresh_digest(now_ms));
        if now_ms >= self.last_full_ms + DIGEST_REFRESH_MS {
            self.last_full_ms = now_ms;
            let full = full_digest_frame(&self.digest);
            for l in self.neighbours.keys() {
                ev.push(RelayEvent::Send {
                    link: *l,
                    frame: full.clone(),
                });
            }
        }
        if !self.neighbours.is_empty() && self.hello.poll(now_ms) {
            let b = self.beacon().to_vec();
            for l in self.neighbours.keys() {
                ev.push(RelayEvent::Send {
                    link: *l,
                    frame: b.clone(),
                });
            }
        }
        ev
    }

    /// Recompute the advertised set (wants ∪ carry tags) and, on change,
    /// broadcast a delta + `HELLO{HAS_DELTA}`.
    fn refresh_digest(&mut self, now_ms: u64) -> Vec<RelayEvent> {
        let nd: Vec<&CuckooFilter> = self
            .neighbours
            .values()
            .filter_map(|n| n.digest.as_ref())
            .collect();
        self.carry.update_from_neighbours(&nd, now_ms);
        let mut next: BTreeSet<MailboxTag> = self.wants.clone();
        next.extend(self.carry.advertised());
        if next == self.advertised {
            return Vec::new();
        }
        let old = self.digest.clone();
        let mut ok = true;
        for t in self.advertised.difference(&next) {
            ok &= self.digest.remove_keep_padding(t, &mut self.rng).is_ok();
        }
        for t in next.difference(&self.advertised) {
            ok &= self.digest.insert_keep_padding(t, &mut self.rng).is_ok();
        }
        if !ok {
            let tags: Vec<MailboxTag> = next.iter().copied().collect();
            match CuckooFilter::build_digest(&tags, DIGEST_MIN_ENTRIES, &mut self.rng) {
                Ok(f) => self.digest = f,
                Err(_) => return Vec::new(),
            }
        }
        self.advertised = next;
        let mut ev = Vec::new();
        let (frame, has_delta) = match old.delta_to(&self.digest) {
            Some(d) => {
                let mut f = vec![FRAME_DIGEST_DELTA, DELTA_MODE_DELTA];
                f.extend_from_slice(&d.encode());
                (f, true)
            }
            None => (full_digest_frame(&self.digest), false),
        };
        let hello = LinkControl::hello_for(&self.digest, has_delta).encode();
        for l in self.neighbours.keys() {
            ev.push(RelayEvent::Send {
                link: *l,
                frame: frame.clone(),
            });
            ev.push(RelayEvent::Send {
                link: *l,
                frame: hello.to_vec(),
            });
        }
        ev
    }

    /// One mesh frame from neighbour `link`.
    pub fn on_frame(&mut self, link: u64, frame: &[u8], now_ms: u64) -> Result<Vec<RelayEvent>> {
        if !self.neighbours.contains_key(&link) {
            return Err(OmcpError::Sync("frame from unknown link"));
        }
        let (&kind, body) = frame
            .split_first()
            .ok_or(OmcpError::Sync("empty mesh frame"))?;
        let mut ev = Vec::new();
        match kind {
            super::hello::TYPE_HELLO | super::hello::TYPE_DIGEST_REQ => {
                match LinkControl::decode(frame)? {
                    LinkControl::Hello { digest_id, .. } => {
                        let n = &self.neighbours[&link];
                        if n.digest.as_ref().map(digest_id32) != Some(digest_id) {
                            ev.push(RelayEvent::Send {
                                link,
                                frame: LinkControl::DigestReq {
                                    has_delta: false,
                                    digest_id: DIGEST_ID_FULL,
                                }
                                .encode()
                                .to_vec(),
                            });
                        }
                    }
                    LinkControl::DigestReq { .. } => ev.push(RelayEvent::Send {
                        link,
                        frame: full_digest_frame(&self.digest),
                    }),
                }
            }
            FRAME_DIGEST_DELTA if body.first() == Some(&DELTA_MODE_FULL) => {
                let f = CuckooFilter::decode(&body[1..])?;
                if let Some(n) = self.neighbours.get_mut(&link) {
                    n.digest = Some(f);
                }
                self.after_digest(link, now_ms, &mut ev);
            }
            FRAME_DIGEST_DELTA if body.first() == Some(&DELTA_MODE_DELTA) => {
                let d = DigestDelta::decode(&body[1..])?;
                let applied = self
                    .neighbours
                    .get_mut(&link)
                    .and_then(|n| n.digest.as_mut())
                    .map(|f| f.apply_delta(&d).is_ok())
                    .unwrap_or(false);
                if applied {
                    self.after_digest(link, now_ms, &mut ev);
                } else {
                    ev.push(RelayEvent::Send {
                        link,
                        frame: LinkControl::DigestReq {
                            has_delta: false,
                            digest_id: DIGEST_ID_FULL,
                        }
                        .encode()
                        .to_vec(),
                    });
                }
            }
            FRAME_DATA => {
                let env = EnvelopeV1::decode(body)?;
                self.on_envelope(link, env, None, now_ms, &mut ev)?;
            }
            FRAME_DELIVERY_ACK => {
                if body.len() < MSG_HASH_LEN {
                    return Err(OmcpError::Sync("delivery ack truncated"));
                }
                let (h, rest) = body.split_at(MSG_HASH_LEN);
                let h: [u8; MSG_HASH_LEN] = h.try_into().expect("16");
                let env = EnvelopeV1::decode(rest)?;
                self.note_ack(&h, now_ms, &mut ev);
                self.on_envelope(link, env, Some(h), now_ms, &mut ev)?;
            }
            _ => return Err(OmcpError::Sync("unknown mesh frame")),
        }
        Ok(ev)
    }

    fn after_digest(&mut self, link: u64, now_ms: u64, ev: &mut Vec<RelayEvent>) {
        ev.extend(self.refresh_digest(now_ms));
        // Store-and-forward: offer what we hold to the (re)described neighbour.
        let mut ids: Vec<[u8; 16]> = self.items.keys().copied().collect();
        ids.sort_by_key(|id| self.items[id].envelope.created_ms);
        for id in ids {
            self.forward_to_one(&id, link, now_ms, ev);
        }
    }

    /// A `DELIVERY_ACK` for `h` passed by: drop envelopes we carry for others
    /// with that hash and refuse later copies. Our own copy stays until the
    /// end-to-end ack is verified ([`Self::cancel_own`]).
    fn note_ack(&mut self, h: &[u8; MSG_HASH_LEN], now_ms: u64, ev: &mut Vec<RelayEvent>) {
        let hit: Vec<[u8; 16]> = self
            .items
            .iter()
            .filter(|(_, i)| !i.own && i.ack_of.is_none() && msg_hash(&i.envelope.ciphertext) == *h)
            .map(|(k, _)| *k)
            .collect();
        for id in hit {
            self.items.remove(&id);
            self.store.take(&id);
            self.sent_to.remove(&id);
            ev.push(RelayEvent::Removed(id));
        }
        if self.acked.len() >= MAX_SEEN {
            if let Some(old) = self.acked.iter().min_by_key(|(_, t)| **t).map(|(k, _)| *k) {
                self.acked.remove(&old);
            }
        }
        self.acked.insert(*h, now_ms.saturating_add(MAX_EXPIRY_MS));
    }

    /// Whether a `DELIVERY_ACK` for `h` was seen.
    pub fn is_acked(&self, h: &[u8; MSG_HASH_LEN]) -> bool {
        self.acked.contains_key(h)
    }

    /// Our own (non-ack) item whose ciphertext hashes to `h`.
    pub fn own_by_hash(&self, h: &[u8; MSG_HASH_LEN]) -> Option<[u8; 16]> {
        self.items
            .iter()
            .find(|(_, i)| i.own && i.ack_of.is_none() && msg_hash(&i.envelope.ciphertext) == *h)
            .map(|(k, _)| *k)
    }

    fn on_envelope(
        &mut self,
        link: u64,
        env: EnvelopeV1,
        ack_of: Option<[u8; MSG_HASH_LEN]>,
        now_ms: u64,
        ev: &mut Vec<RelayEvent>,
    ) -> Result<()> {
        let admitted = self
            .neighbours
            .get_mut(&link)
            .map(|n| n.bucket.admit(now_ms))
            .unwrap_or(false);
        if !admitted {
            return Ok(()); // rate-limited: drop (§22.4)
        }
        let id = env.envelope_id;
        if let Some(n) = self.neighbours.get(&link) {
            // The sender has it: never send it back.
            self.sent_to.entry(id).or_default().insert(n.peer.clone());
        }
        if self.seen(&id) {
            return Ok(());
        }
        let routing = parse_routing_aad(&env.header_aad)?;
        let expires_at = effective_expiry(&env, now_ms)?;
        if expires_at <= now_ms {
            return Ok(());
        }
        let tags = routing.route_tags();
        if let Some(t) = tags.iter().find(|t| self.wants.contains(*t)) {
            self.mark_seen(id, expires_at);
            for t in &tags {
                self.carry.note_activity(*t, now_ms);
            }
            ev.push(RelayEvent::Deliver {
                link,
                tag: *t,
                envelope: env,
                ack_of,
            });
            ev.extend(self.refresh_digest(now_ms));
            return Ok(());
        }
        if !self.cfg.enabled || env.ttl_hops == 0 || tags.is_empty() {
            return Ok(());
        }
        if ack_of.is_none() && self.acked.contains_key(&msg_hash(&env.ciphertext)) {
            self.mark_seen(id, expires_at);
            return Ok(()); // already delivered: do not carry
        }
        let item = CarryItem {
            hop_count: MAX_CARRY_DISTANCE.saturating_sub(env.ttl_hops),
            envelope: env,
            expires_at,
            own: false,
            ack_of,
        };
        match self.store.insert(Self::meta(&item), now_ms) {
            InsertOutcome::Rejected => {
                self.mark_seen(id, expires_at);
                return Ok(());
            }
            InsertOutcome::Stored { evicted } => {
                for e in evicted {
                    self.items.remove(&e);
                    self.sent_to.remove(&e);
                    ev.push(RelayEvent::Removed(e));
                }
            }
        }
        self.mark_seen(id, expires_at);
        self.items.insert(id, item.clone());
        ev.push(RelayEvent::Stored(item));
        self.forward(&id, Some(link), now_ms, ev);
        Ok(())
    }

    /// Plan and send `id` per §22.7(9) to the current neighbours.
    fn forward(&mut self, id: &[u8; 16], from: Option<u64>, now_ms: u64, ev: &mut Vec<RelayEvent>) {
        let Some(item) = self.items.get(id) else {
            return;
        };
        let Ok(routing) = parse_routing_aad(&item.envelope.header_aad) else {
            return;
        };
        let tags = routing.route_tags();
        let links: Vec<u64> = self.neighbours.keys().copied().collect();
        let digests: Vec<Option<&CuckooFilter>> = links
            .iter()
            .map(|l| self.neighbours[l].digest.as_ref())
            .collect();
        let from_idx = from.and_then(|f| links.iter().position(|l| *l == f));
        let plan = plan_forward(
            &tags,
            item.envelope.ttl_hops,
            &digests,
            from_idx,
            self.visible(),
        );
        let targets: Vec<u64> = match plan {
            ForwardPlan::Flood(v) | ForwardPlan::Interest(v) | ForwardPlan::ScopedFlood(v) => {
                v.into_iter().map(|i| links[i]).collect()
            }
            ForwardPlan::Carry(i) => vec![links[i]],
            ForwardPlan::Hold => Vec::new(),
        };
        for l in targets {
            self.send_item(id, l, &tags, now_ms, ev);
        }
    }

    /// Offer `id` to one neighbour if the forwarding rule would pick it.
    fn forward_to_one(&mut self, id: &[u8; 16], link: u64, now_ms: u64, ev: &mut Vec<RelayEvent>) {
        let Some(item) = self.items.get(id) else {
            return;
        };
        let Ok(routing) = parse_routing_aad(&item.envelope.header_aad) else {
            return;
        };
        let tags = routing.route_tags();
        let links: Vec<u64> = self.neighbours.keys().copied().collect();
        let digests: Vec<Option<&CuckooFilter>> = links
            .iter()
            .map(|l| self.neighbours[l].digest.as_ref())
            .collect();
        let picked = match plan_forward(
            &tags,
            item.envelope.ttl_hops,
            &digests,
            None,
            self.visible(),
        ) {
            ForwardPlan::Flood(v) | ForwardPlan::Interest(v) | ForwardPlan::ScopedFlood(v) => {
                v.into_iter().any(|i| links[i] == link)
            }
            ForwardPlan::Carry(i) => links[i] == link,
            ForwardPlan::Hold => false,
        };
        if picked {
            self.send_item(id, link, &tags, now_ms, ev);
        }
    }

    fn send_item(
        &mut self,
        id: &[u8; 16],
        link: u64,
        tags: &[MailboxTag],
        now_ms: u64,
        ev: &mut Vec<RelayEvent>,
    ) {
        let Some(n) = self.neighbours.get(&link) else {
            return;
        };
        let peer = n.peer.clone();
        if self.sent_to.get(id).is_some_and(|s| s.contains(&peer)) {
            return;
        }
        let Some(item) = self.items.get(id) else {
            return;
        };
        let mut env = item.envelope.clone();
        let own = item.own;
        let ack_of = item.ack_of;
        if !own {
            if env.ttl_hops == 0 {
                return;
            }
            env.ttl_hops -= 1;
        }
        let Ok(bytes) = env.encode() else {
            return;
        };
        if !own && self.store.try_forward(bytes.len() as u64, now_ms).is_err() {
            return; // daily forwarding budget spent (§22.4)
        }
        let first = self.sent_to.get(id).is_none_or(|s| s.is_empty());
        self.sent_to.entry(*id).or_default().insert(peer);
        for t in tags {
            self.carry.note_activity(*t, now_ms);
        }
        let frame = match ack_of {
            Some(h) => {
                let mut f = Vec::with_capacity(1 + MSG_HASH_LEN + bytes.len());
                f.push(FRAME_DELIVERY_ACK);
                f.extend_from_slice(&h);
                f.extend_from_slice(&bytes);
                f
            }
            None => frame_with(FRAME_DATA, &bytes),
        };
        ev.push(RelayEvent::Send { link, frame });
        if own && ack_of.is_none() && first {
            ev.push(RelayEvent::Handed(*id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::flags;
    use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
    use crate::sync::tag::{put_subtags_in_aad, put_tag_in_aad};

    fn env(id: u8, sub: MailboxTag, ttl: u8, created: u64) -> EnvelopeV1 {
        let mut aad = Vec::new();
        put_tag_in_aad(&mut aad, &[9; 8]);
        put_subtags_in_aad(&mut aad, &[sub]).unwrap();
        EnvelopeV1 {
            version: ENVELOPE_VERSION_V1,
            suite_id: SUITE_OMCP_2026_A,
            envelope_id: [id; 16],
            flags: flags::RELAY_OK | flags::STORE_OK,
            ttl_hops: ttl,
            created_ms: created,
            header_aad: aad,
            ciphertext: vec![0xCC; 256],
        }
    }

    /// Deliver every Send between engines connected by `links` (link id →
    /// (engine index, peer link id)) until quiet. Returns deliveries per node.
    fn pump(
        nodes: &mut [RelayEngine],
        wires: &HashMap<(usize, u64), (usize, u64)>,
        mut pending: Vec<(usize, RelayEvent)>,
        now: u64,
    ) -> Vec<(usize, EnvelopeV1)> {
        let mut got = Vec::new();
        let mut guard = 0;
        while let Some((n, e)) = pending.pop() {
            guard += 1;
            assert!(guard < 100_000, "pump did not converge");
            match e {
                RelayEvent::Send { link, frame } => {
                    let Some(&(m, ml)) = wires.get(&(n, link)) else {
                        continue;
                    };
                    for e2 in nodes[m].on_frame(ml, &frame, now).unwrap() {
                        pending.push((m, e2));
                    }
                }
                RelayEvent::Deliver { envelope, .. } => got.push((n, envelope)),
                _ => {}
            }
        }
        got
    }

    fn connect(
        nodes: &mut [RelayEngine],
        wires: &mut HashMap<(usize, u64), (usize, u64)>,
        a: usize,
        b: usize,
        link: u64,
        now: u64,
    ) -> Vec<(usize, EnvelopeV1)> {
        wires.insert((a, link), (b, link));
        wires.insert((b, link), (a, link));
        let mut p: Vec<(usize, RelayEvent)> = Vec::new();
        p.extend(
            nodes[a]
                .link_up(link, &[b as u8], now)
                .into_iter()
                .map(|e| (a, e)),
        );
        p.extend(
            nodes[b]
                .link_up(link, &[a as u8], now)
                .into_iter()
                .map(|e| (b, e)),
        );
        pump(nodes, wires, p, now)
    }

    fn disconnect(
        nodes: &mut [RelayEngine],
        wires: &mut HashMap<(usize, u64), (usize, u64)>,
        a: usize,
        b: usize,
        link: u64,
    ) {
        wires.remove(&(a, link));
        wires.remove(&(b, link));
        nodes[a].link_down(link, 0);
        nodes[b].link_down(link, 0);
    }

    #[test]
    fn store_and_forward_chain_a_b_c_never_in_range() {
        let now = 1_000_000;
        let sub_c = [0xC0; 8];
        let mut nodes: Vec<RelayEngine> = (0..3)
            .map(|i| RelayEngine::new(RelayConfig::default(), now, i))
            .collect();
        nodes[2].set_wants([sub_c].into(), now);
        let mut wires = HashMap::new();
        // A meets B; A submits for C; B stores it.
        connect(&mut nodes, &mut wires, 0, 1, 1, now);
        let ev = nodes[0].submit_own(env(1, sub_c, 8, now), now).unwrap();
        let got = pump(
            &mut nodes,
            &wires,
            ev.into_iter().map(|e| (0, e)).collect(),
            now,
        );
        assert!(got.is_empty());
        assert_eq!(nodes[1].stats().carried, 1);
        disconnect(&mut nodes, &mut wires, 0, 1, 1);
        // Later B meets C: delivered once, with one hop spent.
        let got = connect(&mut nodes, &mut wires, 1, 2, 2, now + 60_000);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, 2);
        assert_eq!(got[0].1.envelope_id, [1; 16]);
        assert_eq!(got[0].1.ttl_hops, 7);
        // Re-meeting does not deliver twice.
        disconnect(&mut nodes, &mut wires, 1, 2, 2);
        assert!(connect(&mut nodes, &mut wires, 1, 2, 3, now + 120_000).is_empty());
    }

    #[test]
    fn relay_off_does_not_carry_but_still_delivers_own() {
        let now = 5;
        let sub_b = [0xB0; 8];
        let mut nodes: Vec<RelayEngine> = (0..2)
            .map(|i| RelayEngine::new(RelayConfig::default(), now, i))
            .collect();
        nodes[1].set_config(RelayConfig {
            enabled: false,
            ..RelayConfig::default()
        });
        nodes[1].set_wants([sub_b].into(), now);
        let mut wires = HashMap::new();
        connect(&mut nodes, &mut wires, 0, 1, 1, now);
        let mut p: Vec<(usize, RelayEvent)> = Vec::new();
        p.extend(
            nodes[0]
                .submit_own(env(1, sub_b, 8, now), now)
                .unwrap()
                .into_iter()
                .map(|e| (0, e)),
        );
        p.extend(
            nodes[0]
                .submit_own(env(2, [0xEE; 8], 8, now), now)
                .unwrap()
                .into_iter()
                .map(|e| (0, e)),
        );
        let got = pump(&mut nodes, &wires, p, now);
        assert_eq!(got.len(), 1);
        assert_eq!(nodes[1].stats().carried, 0);
    }

    #[test]
    fn forwarding_order_want_then_carry_then_scoped_flood() {
        // B has neighbours W (wants t), K (carries t at d=1), X (nothing).
        let now = 10;
        let t = [0x77; 8];
        let mut b = RelayEngine::new(RelayConfig::default(), now, 1);
        b.set_visible_peers_override(Some(100));
        for l in [1u64, 2, 3, 4] {
            b.link_up(l, &[l as u8], now);
        }
        let mut rng = SplitMix64(3);
        let want = CuckooFilter::build_digest(&[t], 32, &mut rng).unwrap();
        let carry =
            CuckooFilter::build_digest(&[super::super::tag::carry_tag(&t, 1)], 32, &mut rng)
                .unwrap();
        let none = CuckooFilter::build_digest(&[], 32, &mut rng).unwrap();
        let sends = |ev: &[RelayEvent]| -> Vec<u64> {
            ev.iter()
                .filter_map(|e| match e {
                    RelayEvent::Send { link, frame } if frame[0] == FRAME_DATA => Some(*link),
                    _ => None,
                })
                .collect()
        };
        b.on_frame(2, &full_digest_frame(&carry), now).unwrap();
        b.on_frame(3, &full_digest_frame(&none), now).unwrap();
        b.on_frame(4, &full_digest_frame(&none), now).unwrap();
        // no want: lowest carry (link 2)
        let ev = b
            .on_frame(
                1,
                &frame_with(FRAME_DATA, &env(1, t, 8, now).encode().unwrap()),
                now,
            )
            .unwrap();
        assert_eq!(sends(&ev), vec![2]);
        // want beats carry
        b.on_frame(3, &full_digest_frame(&want), now).unwrap();
        let ev = b
            .on_frame(
                1,
                &frame_with(FRAME_DATA, &env(2, t, 8, now).encode().unwrap()),
                now,
            )
            .unwrap();
        assert_eq!(sends(&ev), vec![3]);
        // unknown tag: scoped flood to everyone but the sender
        let ev = b
            .on_frame(
                1,
                &frame_with(FRAME_DATA, &env(3, [0x55; 8], 8, now).encode().unwrap()),
                now,
            )
            .unwrap();
        assert_eq!(sends(&ev), vec![2, 3, 4]);
    }

    #[test]
    fn expiry_tlv_cap_and_digest_resync() {
        let now = 1_000_000;
        let mut a = RelayEngine::new(
            RelayConfig {
                enabled: true,
                budget: RelayBudget {
                    storage_bytes: 700,
                    daily_forward_bytes: None,
                },
            },
            now,
            1,
        );
        a.link_up(1, &[1], now);
        let mut e = env(1, [1; 8], 8, now);
        put_expires_in_aad(&mut e.header_aad, now + 10_000);
        assert_eq!(&e.header_aad[e.header_aad.len() - 6..][..2], &[0x58, 4]);
        let ev = a
            .on_frame(1, &frame_with(FRAME_DATA, &e.encode().unwrap()), now)
            .unwrap();
        assert!(ev.iter().any(|e| matches!(e, RelayEvent::Stored(_))));
        let ev = a.tick(now + 11_000);
        assert!(ev.contains(&RelayEvent::Removed([1; 16])));
        assert_eq!(a.stats().carried, 0);
        // cap: two ~330 B envelopes fit in 700 B, the third evicts the oldest-expiring
        for i in 2..5u8 {
            let e = env(i, [1; 8], 8, now + i as u64);
            a.on_frame(1, &frame_with(FRAME_DATA, &e.encode().unwrap()), now)
                .unwrap();
            assert!(a.stats().carried_bytes <= 700);
        }
        assert_eq!(a.stats().carried, 2);
        assert!(a.item(&[2; 16]).is_none());
        // Mismatched hello → full digest request.
        let ev = a.on_frame(1, &[0x48, 0, 1, 2, 3, 4], now).unwrap();
        assert!(ev.iter().any(
            |e| matches!(e, RelayEvent::Send { frame, .. } if frame == &vec![0x52, 0, 0, 0, 0, 0])
        ));
        assert!(a.on_frame(1, &[0x33], now).is_err());
        assert!(a.on_frame(9, &[0x48, 0, 0, 0, 0, 1], now).is_err());
    }

    #[test]
    fn delivery_ack_deletes_relay_copies_and_reaches_sender() {
        let now = 1_000_000;
        let (sub_a, sub_c) = ([0xA0; 8], [0xC0; 8]);
        let mut nodes: Vec<RelayEngine> = (0..4)
            .map(|i| RelayEngine::new(RelayConfig::default(), now, i))
            .collect();
        nodes[0].set_wants([sub_a].into(), now);
        nodes[2].set_wants([sub_c].into(), now);
        let mut wires = HashMap::new();
        // A gives the message to B and to D (two carriers).
        connect(&mut nodes, &mut wires, 0, 1, 1, now);
        connect(&mut nodes, &mut wires, 0, 3, 4, now);
        let ev = nodes[0].submit_own(env(1, sub_c, 8, now), now).unwrap();
        assert!(ev.contains(&RelayEvent::Handed([1; 16])));
        pump(
            &mut nodes,
            &wires,
            ev.into_iter().map(|e| (0, e)).collect(),
            now,
        );
        assert_eq!(nodes[1].stats().carried, 1);
        assert_eq!(nodes[3].stats().carried, 1);
        disconnect(&mut nodes, &mut wires, 0, 1, 1);
        disconnect(&mut nodes, &mut wires, 0, 3, 4);
        // B meets C: delivered; C acks to A's sub-tag through B.
        let got = connect(&mut nodes, &mut wires, 1, 2, 2, now + 10);
        let h = msg_hash(&got[0].1.ciphertext);
        let ev = nodes[2]
            .submit_ack(env(2, sub_a, 8, now + 10), h, now + 10)
            .unwrap();
        pump(
            &mut nodes,
            &wires,
            ev.into_iter().map(|e| (2, e)).collect(),
            now + 10,
        );
        assert!(
            nodes[1].item(&[1; 16]).is_none(),
            "B dropped the acked copy"
        );
        assert!(nodes[1].item(&[2; 16]).is_some(), "B carries the ack");
        disconnect(&mut nodes, &mut wires, 1, 2, 2);
        // D meets B: D learns the ack and drops its copy too.
        connect(&mut nodes, &mut wires, 3, 1, 5, now + 20);
        assert!(nodes[3].item(&[1; 16]).is_none());
        // B meets A: the ack is delivered as an ack; A's own copy stays until verified.
        let mut p: Vec<(usize, RelayEvent)> = Vec::new();
        wires.insert((1, 6), (0, 6));
        wires.insert((0, 6), (1, 6));
        p.extend(
            nodes[1]
                .link_up(6, &[0], now + 30)
                .into_iter()
                .map(|e| (1, e)),
        );
        p.extend(
            nodes[0]
                .link_up(6, &[1], now + 30)
                .into_iter()
                .map(|e| (0, e)),
        );
        let mut acks = Vec::new();
        while let Some((n, e)) = p.pop() {
            match e {
                RelayEvent::Send { link, frame } => {
                    if let Some(&(m, ml)) = wires.get(&(n, link)) {
                        p.extend(
                            nodes[m]
                                .on_frame(ml, &frame, now + 30)
                                .unwrap()
                                .into_iter()
                                .map(|e| (m, e)),
                        );
                    }
                }
                RelayEvent::Deliver {
                    ack_of: Some(a), ..
                } if n == 0 => acks.push(a),
                _ => {}
            }
        }
        assert_eq!(acks, vec![h]);
        assert_eq!(nodes[0].own_by_hash(&h), Some([1; 16]));
        nodes[0].cancel_own(&[1; 16]);
        assert_eq!(nodes[0].own_by_hash(&h), None);
    }
}

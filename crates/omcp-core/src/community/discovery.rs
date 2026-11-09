//! Community discovery (PROTOCOL §20.8; REQUIREMENTS TM25–TM31).
//!
//! Listed communities announce a signed [`Announcement`] (or withdraw it with
//! a [`Delist`]) in public envelopes on one well-known discovery tag, with
//! header TLV `0x5D` `community_announce` (version, community_ref, seq) so
//! relays can drop delisted, superseded and taken-down listings without
//! parsing the body. Every node keeps a bounded local [`DiscoveryIndex`]
//! (2 MB / 5,000 entries); browsing and search run only against it
//! ([`DiscoveryIndex::search`]) and nothing about a search ever leaves the
//! device. Listed is **off by default** ([`LISTED_DEFAULT`]).
//!
//! Private groups are never announced: an announcement only carries the
//! fields of one open community.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

use super::{community_ref, verify};
use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::moderation::{h256, peer_id_hash, ActionKind, ModerationState};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{parse_routing_aad, put_tag_in_aad, MailboxTag};

/// MR53 / TM27: a new community is never listed unless its owner opts in.
pub const LISTED_DEFAULT: bool = false;
/// Item 9: shown before the first announcement (`comm-listed-notice`).
pub const LISTED_NOTICE: &str = "Listing makes this community findable by anyone nearby and by phones that pass it on. Its name, description, tags and approximate size become public. Posts in communities are public.";

pub const AAD_TLV_COMMUNITY_ANNOUNCE: u8 = 0x5D;
const TLV_LEN: u8 = 41;
const DOMAIN: &[u8] = b"OMCP-community-announce-v1\x00";
const TAG_DOMAIN: &[u8] = b"OMCP-discovery-tag-v1\x00";
const ID_DOMAIN: &[u8] = b"OMCP-announce-id-v1\x00";
const T_ANNOUNCE: u8 = 0x01;
const T_DELIST: u8 = 0x02;
pub const VERSION: u8 = 1;

pub const MAX_PAYLOAD: usize = 1024;
pub const MAX_NAME_BYTES: usize = 64;
pub const MAX_DESC_BYTES: usize = 280;
pub const MAX_DESC_CHARS: usize = 140;
pub const MAX_LANGS: usize = 3;
pub const MAX_TAGS: usize = 5;
pub const MAX_TAG_BYTES: usize = 24;
/// Item 4(b): discovery envelopes never carry more than 6 hops.
pub const MAX_TTL: u8 = 6;
/// Own announcements start with 5 hops left, so they reach 6 hops.
pub const OWN_TTL: u8 = 5;
const DAY: u64 = 24 * 60 * 60 * 1000;
pub const REFRESH_MS: u64 = 7 * DAY;
pub const EXPIRY_MS: u64 = 14 * DAY;
/// §20.8(4)(a): discovery-tag ranges to a neighbour at most once per 6 h
/// unless the local discovery index changed.
pub const DISCOVERY_PACING_MS: u64 = 6 * 60 * 60 * 1000;
pub const DELIST_KEEP_MS: u64 = 14 * DAY;
pub const FUTURE_SKEW_MS: u64 = DAY;
pub const SEQ_RATE_MS: u64 = DAY;
pub const INDEX_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const INDEX_MAX_ENTRIES: usize = 5_000;
pub const MAX_PER_KEY: usize = 3;
/// Item 5(b): discovery envelopes forwarded per hour, in total.
pub const FORWARD_PER_HOUR: u32 = 200;
pub const NEAR_HOPS: u8 = 2;
pub const NEAR_MS: u64 = 7 * DAY;

/// The 8 categories (TM31). Codes 9–255 are shown as [`CAT_OTHER`].
pub const CAT_OTHER: u8 = 8;
pub fn display_category(code: u8) -> u8 {
    if (1..=8).contains(&code) {
        code
    } else {
        CAT_OTHER
    }
}

/// Bucket 1..=5 → member range shown instead of a count (TM26).
pub fn bucket_range(b: u8) -> (u32, u32) {
    match b {
        1 => (1, 10),
        2 => (11, 50),
        3 => (51, 200),
        4 => (201, 1_000),
        _ => (1_001, 5_000),
    }
}

/// Owner's bucket for an approximate member count.
pub fn bucket_for(members: usize) -> u8 {
    match members {
        0..=10 => 1,
        11..=50 => 2,
        51..=200 => 3,
        201..=1_000 => 4,
        _ => 5,
    }
}

pub fn discovery_tag() -> MailboxTag {
    h256(&[TAG_DOMAIN])[..8].try_into().expect("8")
}

pub fn announce_id(cref: &[u8; 32], seq: u64) -> [u8; 32] {
    h256(&[ID_DOMAIN, cref, &seq.to_be_bytes()])
}

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Announcement {
    pub community_id: [u8; 32],
    pub community_pk: [u8; 32],
    pub owner_pk: [u8; 32],
    pub seq: u64,
    pub issued_ms: u64,
    pub name: String,
    pub description: String,
    pub langs: Vec<String>,
    pub categories: Vec<u8>,
    pub tags: Vec<String>,
    pub member_bucket: u8,
    pub join_policy: u8,
    pub icon_hash: Option<[u8; 32]>,
    #[serde(with = "super::sig64")]
    pub sig_community: [u8; 64],
    #[serde(with = "super::sig64")]
    pub sig_owner: [u8; 64],
}

/// Fields the owner sets (the rest is signed in).
#[derive(Debug, Clone, Default)]
pub struct ListingFields {
    pub name: String,
    pub description: String,
    pub langs: Vec<String>,
    pub categories: Vec<u8>,
    pub tags: Vec<String>,
    pub member_bucket: u8,
    pub join_policy: u8,
    pub icon_hash: Option<[u8; 32]>,
}

fn put_str8(b: &mut Vec<u8>, s: &str) {
    b.push(s.len() as u8);
    b.extend_from_slice(s.as_bytes());
}

impl Announcement {
    pub fn community_ref(&self) -> [u8; 32] {
        community_ref(&self.community_id)
    }

    /// Fields after the payload type byte, without signatures.
    fn fields(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(512);
        b.push(VERSION);
        b.extend_from_slice(&self.community_id);
        b.extend_from_slice(&self.community_ref());
        b.extend_from_slice(&self.community_pk);
        b.extend_from_slice(&self.owner_pk);
        b.extend_from_slice(&self.seq.to_be_bytes());
        b.extend_from_slice(&self.issued_ms.to_be_bytes());
        put_str8(&mut b, &self.name);
        b.extend_from_slice(&(self.description.len() as u16).to_be_bytes());
        b.extend_from_slice(self.description.as_bytes());
        b.push(self.langs.len() as u8);
        for l in &self.langs {
            put_str8(&mut b, l);
        }
        b.push(self.categories.len() as u8);
        b.extend_from_slice(&self.categories);
        b.push(self.tags.len() as u8);
        for t in &self.tags {
            put_str8(&mut b, t);
        }
        b.push(self.member_bucket);
        b.push(self.join_policy);
        match &self.icon_hash {
            Some(h) => {
                b.push(1);
                b.extend_from_slice(h);
            }
            None => b.push(0),
        }
        b
    }

    fn signed(&self) -> Vec<u8> {
        [DOMAIN, &self.fields()].concat()
    }

    /// Item 2 field limits (MR54).
    pub fn check_fields(&self) -> Result<()> {
        let n = self.name.trim();
        if n.is_empty() || self.name.len() > MAX_NAME_BYTES {
            return err("announce name");
        }
        if self.description.len() > MAX_DESC_BYTES
            || self.description.chars().count() > MAX_DESC_CHARS
        {
            return err("announce description");
        }
        if self.langs.len() > MAX_LANGS
            || self
                .langs
                .iter()
                .any(|l| l.is_empty() || l.len() > 8 || !l.is_ascii())
        {
            return err("announce langs");
        }
        if self.categories.is_empty() || self.categories.len() > 2 {
            return err("announce categories");
        }
        if self.tags.len() > MAX_TAGS
            || self
                .tags
                .iter()
                .any(|t| t.is_empty() || t.len() > MAX_TAG_BYTES || *t != t.to_lowercase())
        {
            return err("announce tags");
        }
        if !(1..=5).contains(&self.member_bucket) || !(1..=2).contains(&self.join_policy) {
            return err("announce bucket/policy");
        }
        Ok(())
    }

    /// Build and sign (community key, then owner key over that signature).
    pub fn sign(
        community_id: [u8; 32],
        community_key: &SigningKey,
        owner_key: &SigningKey,
        seq: u64,
        issued_ms: u64,
        f: ListingFields,
    ) -> Result<Self> {
        let mut a = Self {
            community_id,
            community_pk: community_key.verifying_key().to_bytes(),
            owner_pk: owner_key.verifying_key().to_bytes(),
            seq,
            issued_ms,
            name: f.name.trim().to_string(),
            description: f.description,
            langs: f.langs,
            categories: f.categories,
            tags: f
                .tags
                .into_iter()
                .map(|t| t.trim().to_lowercase())
                .collect(),
            member_bucket: f.member_bucket,
            join_policy: f.join_policy,
            icon_hash: f.icon_hash,
            sig_community: [0; 64],
            sig_owner: [0; 64],
        };
        a.check_fields()?;
        let body = a.signed();
        a.sig_community = community_key.sign(&body).to_bytes();
        a.sig_owner = owner_key
            .sign(&[body.as_slice(), &a.sig_community].concat())
            .to_bytes();
        if a.encode().len() > MAX_PAYLOAD {
            return err("announce too large");
        }
        Ok(a)
    }

    pub fn verify_sigs(&self) -> bool {
        let body = self.signed();
        verify(&self.community_pk, &body, &self.sig_community)
            && verify(
                &self.owner_pk,
                &[body.as_slice(), &self.sig_community].concat(),
                &self.sig_owner,
            )
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![T_ANNOUNCE];
        b.extend_from_slice(&self.fields());
        b.extend_from_slice(&self.sig_community);
        b.extend_from_slice(&self.sig_owner);
        b
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delist {
    pub community_ref: [u8; 32],
    pub seq: u64,
    pub issued_ms: u64,
    #[serde(with = "super::sig64")]
    pub sig_community: [u8; 64],
    #[serde(with = "super::sig64")]
    pub sig_owner: [u8; 64],
}

impl Delist {
    fn signed(&self) -> Vec<u8> {
        let mut b = DOMAIN.to_vec();
        b.push(VERSION);
        b.extend_from_slice(&self.community_ref);
        b.extend_from_slice(&self.seq.to_be_bytes());
        b.extend_from_slice(&self.issued_ms.to_be_bytes());
        b
    }
    pub fn sign(
        community_id: &[u8; 32],
        community_key: &SigningKey,
        owner_key: &SigningKey,
        seq: u64,
        issued_ms: u64,
    ) -> Self {
        let mut d = Self {
            community_ref: community_ref(community_id),
            seq,
            issued_ms,
            sig_community: [0; 64],
            sig_owner: [0; 64],
        };
        let body = d.signed();
        d.sig_community = community_key.sign(&body).to_bytes();
        d.sig_owner = owner_key
            .sign(&[body.as_slice(), &d.sig_community].concat())
            .to_bytes();
        d
    }
    pub fn verify_sigs(&self, community_pk: &[u8; 32], owner_pk: &[u8; 32]) -> bool {
        let body = self.signed();
        verify(community_pk, &body, &self.sig_community)
            && verify(
                owner_pk,
                &[body.as_slice(), &self.sig_community].concat(),
                &self.sig_owner,
            )
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![T_DELIST];
        b.extend_from_slice(&self.signed()[DOMAIN.len()..]);
        b.extend_from_slice(&self.sig_community);
        b.extend_from_slice(&self.sig_owner);
        b
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // short-lived, one per envelope
pub enum Listing {
    Announce(Announcement),
    Delist(Delist),
}

impl Listing {
    pub fn community_ref(&self) -> [u8; 32] {
        match self {
            Self::Announce(a) => a.community_ref(),
            Self::Delist(d) => d.community_ref,
        }
    }
    pub fn seq(&self) -> u64 {
        match self {
            Self::Announce(a) => a.seq,
            Self::Delist(d) => d.seq,
        }
    }
    pub fn issued_ms(&self) -> u64 {
        match self {
            Self::Announce(a) => a.issued_ms,
            Self::Delist(d) => d.issued_ms,
        }
    }
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Announce(a) => a.encode(),
            Self::Delist(d) => d.encode(),
        }
    }

    /// Strict parse (sizes and field limits; not signatures or state).
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() > MAX_PAYLOAD {
            return err("announce too large");
        }
        let mut r = Rd { b, i: 0 };
        let t = r.u8()?;
        if r.u8()? != VERSION {
            return err("announce version");
        }
        let l = match t {
            T_ANNOUNCE => {
                let community_id = r.a32()?;
                let cref = r.a32()?;
                if cref != community_ref(&community_id) {
                    return err("announce community_ref");
                }
                let community_pk = r.a32()?;
                let owner_pk = r.a32()?;
                let seq = r.u64()?;
                let issued_ms = r.u64()?;
                let n = r.u8()? as usize;
                let name = r.str(n)?;
                let dl = u16::from_be_bytes([r.u8()?, r.u8()?]) as usize;
                let description = r.str(dl)?;
                let mut langs = Vec::new();
                for _ in 0..r.u8()? {
                    let n = r.u8()? as usize;
                    langs.push(r.str(n)?);
                }
                let nc = r.u8()? as usize;
                let categories = r.take(nc)?.to_vec();
                let mut tags = Vec::new();
                for _ in 0..r.u8()? {
                    let n = r.u8()? as usize;
                    tags.push(r.str(n)?);
                }
                let member_bucket = r.u8()?;
                let join_policy = r.u8()?;
                let icon_hash = match r.u8()? {
                    0 => None,
                    1 => Some(r.a32()?),
                    _ => return err("announce icon"),
                };
                let a = Announcement {
                    community_id,
                    community_pk,
                    owner_pk,
                    seq,
                    issued_ms,
                    name,
                    description,
                    langs,
                    categories,
                    tags,
                    member_bucket,
                    join_policy,
                    icon_hash,
                    sig_community: r.a64()?,
                    sig_owner: r.a64()?,
                };
                a.check_fields()?;
                Listing::Announce(a)
            }
            T_DELIST => Listing::Delist(Delist {
                community_ref: r.a32()?,
                seq: r.u64()?,
                issued_ms: r.u64()?,
                sig_community: r.a64()?,
                sig_owner: r.a64()?,
            }),
            _ => return err("announce type"),
        };
        if r.i != b.len() {
            return err("announce trailing bytes");
        }
        Ok(l)
    }
}

struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self
            .b
            .get(self.i..self.i + n)
            .ok_or(OmcpError::Sync("announce truncated"))?;
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn a32(&mut self) -> Result<[u8; 32]> {
        Ok(self.take(32)?.try_into().expect("32"))
    }
    fn a64(&mut self) -> Result<[u8; 64]> {
        Ok(self.take(64)?.try_into().expect("64"))
    }
    fn str(&mut self, n: usize) -> Result<String> {
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| OmcpError::Sync("announce utf-8"))
    }
}

/// `0x5D` value: version, community_ref, seq.
pub fn announce_tlv(aad: &[u8]) -> Option<([u8; 32], u64)> {
    let mut i = 0;
    while i + 2 <= aad.len() {
        let (t, l) = (aad[i], aad[i + 1] as usize);
        let v = aad.get(i + 2..i + 2 + l)?;
        if t == AAD_TLV_COMMUNITY_ANNOUNCE {
            if l != TLV_LEN as usize || v[0] != VERSION {
                return None;
            }
            return Some((
                v[1..33].try_into().expect("32"),
                u64::from_be_bytes(v[33..41].try_into().expect("8")),
            ));
        }
        i += 2 + l;
    }
    None
}

/// Public discovery envelope (RELAY_OK | STORE_OK, never PRIORITY).
pub fn seal(l: &Listing, ttl_hops: u8) -> EnvelopeV1 {
    let body = l.encode();
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &discovery_tag());
    let until = match l {
        Listing::Announce(a) => a.issued_ms.saturating_add(EXPIRY_MS),
        Listing::Delist(d) => d.issued_ms.saturating_add(DELIST_KEEP_MS),
    };
    put_expires_in_aad(&mut header_aad, until);
    header_aad.extend_from_slice(&[AAD_TLV_COMMUNITY_ANNOUNCE, TLV_LEN, VERSION]);
    header_aad.extend_from_slice(&l.community_ref());
    header_aad.extend_from_slice(&l.seq().to_be_bytes());
    let id = h256(&[b"OMCP-discovery-env-v1\x00", &body]);
    EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: id[..16].try_into().expect("16"),
        flags: flags::RELAY_OK | flags::STORE_OK,
        ttl_hops: ttl_hops.min(MAX_TTL),
        created_ms: l.issued_ms(),
        header_aad,
        ciphertext: body,
    }
}

/// Parse a discovery envelope: tag, TLV and body must agree.
pub fn open(env: &EnvelopeV1) -> Result<Listing> {
    if parse_routing_aad(&env.header_aad)?.tag != Some(discovery_tag()) {
        return err("not the discovery tag");
    }
    if env.ttl_hops > MAX_TTL || env.flags & flags::PRIORITY != 0 {
        return err("discovery ttl/flags");
    }
    let id = h256(&[b"OMCP-discovery-env-v1\x00", &env.ciphertext]);
    if env.envelope_id[..] != id[..16] {
        return err("discovery envelope id");
    }
    let l = Listing::decode(&env.ciphertext)?;
    if announce_tlv(&env.header_aad) != Some((l.community_ref(), l.seq())) {
        return err("discovery TLV");
    }
    Ok(l)
}

/// Moderation and roster facts the index checks (item 8).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryContext {
    /// community_refs under a team takedown or with any community tombstone.
    pub blocked: BTreeSet<[u8; 32]>,
    /// (community_ref, owner member_id) under a team MEMBER_REMOVE.
    pub removed_owners: BTreeSet<([u8; 32], [u8; 32])>,
    /// Latest known roster: community_ref → founder / owner key.
    pub roster_owner: BTreeMap<[u8; 32], [u8; 32]>,
    /// Reported (or worse) communities, hidden from browse and search.
    pub reported: BTreeSet<[u8; 32]>,
}

impl DiscoveryContext {
    /// From team moderation state; `owners` are (community_id, owner_pk)
    /// pairs to check for a team MEMBER_REMOVE.
    pub fn from_moderation(
        m: &ModerationState,
        owners: impl IntoIterator<Item = ([u8; 32], [u8; 32])>,
        now_ms: u64,
    ) -> Self {
        let mut c = Self::default();
        for t in m.tombstones() {
            if t.kind == ActionKind::Takedown {
                c.blocked.insert(community_ref(&t.target_id));
            }
        }
        for (id, pk) in owners {
            let mid = peer_id_hash(&pk);
            if m.team_member_removal(&id, &mid, now_ms).is_some() {
                c.removed_owners.insert((community_ref(&id), mid));
            }
        }
        c
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub ann: Announcement,
    pub envelope_id: [u8; 16],
    /// Hop count when first received (eviction, ranking).
    pub hops: u8,
    /// Last time this phone heard it itself over the local radio within
    /// [`NEAR_HOPS`] (never sent anywhere).
    pub near_heard_ms: Option<u64>,
    /// When the current `seq` was accepted (rate limit 5(c)).
    pub accepted_ms: u64,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DelistRec {
    seq: u64,
    until_ms: u64,
    keys: ([u8; 32], [u8; 32]),
    envelope_id: [u8; 16],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    Invalid,
    Expired,
    Future,
    Stale,
    RateLimited,
    TooManyForKey,
    Moderated,
    RosterMismatch,
    Full,
    UnknownDelist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ingested {
    /// New or newer: store and relay; `evicted` envelopes are dropped.
    Stored { evicted: Vec<[u8; 16]> },
    /// Same listing heard again (only the local near-you data changed).
    Again,
}

/// One search / browse result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found<'a> {
    pub ann: &'a Announcement,
    pub near_you: bool,
    pub hops: u8,
    pub score: u32,
}

/// Local on-device discovery index (item 5(a)).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryIndex {
    entries: BTreeMap<[u8; 32], Entry>,
    delists: BTreeMap<[u8; 32], DelistRec>,
    hidden: BTreeSet<[u8; 32]>,
    ctx: DiscoveryContext,
    /// Refs this node stopped relaying after a failed join check (item 6).
    #[serde(default)]
    distrusted: BTreeSet<[u8; 32]>,
}

impl DiscoveryIndex {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn bytes(&self) -> usize {
        self.entries.values().map(|e| e.size).sum()
    }
    pub fn get(&self, cref: &[u8; 32]) -> Option<&Entry> {
        self.entries.get(cref)
    }
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }
    /// Envelope ids this index still vouches for.
    pub fn holds(&self, id: &[u8; 16]) -> bool {
        self.entries.values().any(|e| e.envelope_id == *id)
            || self.delists.values().any(|d| d.envelope_id == *id)
    }

    fn removed(&mut self, cref: &[u8; 32], out: &mut Vec<[u8; 16]>) {
        if let Some(e) = self.entries.remove(cref) {
            out.push(e.envelope_id);
        }
    }

    /// Validate one received listing (item 6) and apply it.
    pub fn ingest(
        &mut self,
        l: &Listing,
        envelope_id: [u8; 16],
        hops: u8,
        via_radio: bool,
        now_ms: u64,
    ) -> std::result::Result<Ingested, Rejected> {
        let cref = l.community_ref();
        if self.ctx.blocked.contains(&cref) || self.distrusted.contains(&cref) {
            return Err(Rejected::Moderated);
        }
        if l.issued_ms() > now_ms.saturating_add(FUTURE_SKEW_MS) {
            return Err(Rejected::Future);
        }
        let mut evicted = Vec::new();
        match l {
            Listing::Delist(d) => {
                let keys = match (self.entries.get(&cref), self.delists.get(&cref)) {
                    (Some(e), _) => (e.ann.community_pk, e.ann.owner_pk),
                    (None, Some(r)) => r.keys,
                    _ => return Err(Rejected::UnknownDelist),
                };
                if !d.verify_sigs(&keys.0, &keys.1) {
                    return Err(Rejected::Invalid);
                }
                let held = self.entries.get(&cref).map(|e| e.ann.seq);
                let dl = self.delists.get(&cref).map(|r| r.seq);
                if held.max(dl).is_some_and(|s| d.seq <= s) {
                    return if dl == Some(d.seq) {
                        Ok(Ingested::Again)
                    } else {
                        Err(Rejected::Stale)
                    };
                }
                if d.issued_ms.saturating_add(DELIST_KEEP_MS) <= now_ms {
                    return Err(Rejected::Expired);
                }
                self.removed(&cref, &mut evicted);
                if let Some(old) = self.delists.insert(
                    cref,
                    DelistRec {
                        seq: d.seq,
                        until_ms: d.issued_ms.saturating_add(DELIST_KEEP_MS),
                        keys,
                        envelope_id,
                    },
                ) {
                    evicted.push(old.envelope_id);
                }
                Ok(Ingested::Stored { evicted })
            }
            Listing::Announce(a) => {
                if !a.verify_sigs() {
                    return Err(Rejected::Invalid);
                }
                if a.issued_ms.saturating_add(EXPIRY_MS) <= now_ms {
                    return Err(Rejected::Expired);
                }
                if self
                    .ctx
                    .removed_owners
                    .contains(&(cref, peer_id_hash(&a.owner_pk)))
                {
                    return Err(Rejected::Moderated);
                }
                // owner_pk must match the known roster owner; community_pk is
                // the community identity key (§20.1) and may differ after
                // §23.10.13 separates it from the owner's key.
                if self
                    .ctx
                    .roster_owner
                    .get(&cref)
                    .is_some_and(|o| *o != a.owner_pk)
                {
                    return Err(Rejected::RosterMismatch);
                }
                if self.delists.get(&cref).is_some_and(|r| a.seq <= r.seq) {
                    return Err(Rejected::Stale);
                }
                let near = (via_radio && hops <= NEAR_HOPS).then_some(now_ms);
                if let Some(e) = self.entries.get_mut(&cref) {
                    if a.seq == e.ann.seq && *a == e.ann {
                        if near.is_some() {
                            e.near_heard_ms = near;
                        }
                        return Ok(Ingested::Again);
                    }
                    if a.seq <= e.ann.seq {
                        return Err(Rejected::Stale);
                    }
                    if now_ms.saturating_sub(e.accepted_ms) < SEQ_RATE_MS {
                        return Err(Rejected::RateLimited);
                    }
                }
                let same_key = self
                    .entries
                    .iter()
                    .filter(|(r, e)| {
                        **r != cref
                            && (e.ann.owner_pk == a.owner_pk
                                || e.ann.community_pk == a.community_pk)
                    })
                    .count();
                if same_key >= MAX_PER_KEY {
                    return Err(Rejected::TooManyForKey);
                }
                let prev_near = self.entries.get(&cref).and_then(|e| e.near_heard_ms);
                self.removed(&cref, &mut evicted);
                self.entries.insert(
                    cref,
                    Entry {
                        size: a.encode().len() + 96,
                        ann: a.clone(),
                        envelope_id,
                        hops,
                        near_heard_ms: near.or(prev_near),
                        accepted_ms: now_ms,
                    },
                );
                self.evict_to_budget(now_ms, &mut evicted);
                if !self.entries.contains_key(&cref) {
                    return Err(Rejected::Full);
                }
                Ok(Ingested::Stored { evicted })
            }
        }
    }

    /// Expired first, then the farthest, then the oldest `issued_ms`.
    fn evict_to_budget(&mut self, now_ms: u64, out: &mut Vec<[u8; 16]>) {
        self.purge_into(now_ms, out);
        while self.entries.len() > INDEX_MAX_ENTRIES || self.bytes() > INDEX_MAX_BYTES {
            let victim = *self
                .entries
                .iter()
                .max_by_key(|(_, e)| (e.hops, std::cmp::Reverse(e.ann.issued_ms)))
                .expect("non-empty")
                .0;
            self.removed(&victim, out);
        }
    }

    fn purge_into(&mut self, now_ms: u64, out: &mut Vec<[u8; 16]>) {
        let gone: Vec<[u8; 32]> = self
            .entries
            .iter()
            .filter(|(_, e)| e.ann.issued_ms.saturating_add(EXPIRY_MS) <= now_ms)
            .map(|(r, _)| *r)
            .collect();
        for r in gone {
            self.removed(&r, out);
        }
        self.delists.retain(|_, d| {
            let keep = d.until_ms > now_ms;
            if !keep {
                out.push(d.envelope_id);
            }
            keep
        });
    }

    /// Drop expired listings and delists; returns their envelope ids.
    pub fn purge(&mut self, now_ms: u64) -> Vec<[u8; 16]> {
        let mut out = Vec::new();
        self.purge_into(now_ms, &mut out);
        out
    }

    /// New moderation / roster facts; drops listings they forbid.
    pub fn set_context(&mut self, ctx: DiscoveryContext) -> Vec<[u8; 16]> {
        self.ctx = ctx;
        let mut out = Vec::new();
        let bad: Vec<[u8; 32]> = self
            .entries
            .iter()
            .filter(|(r, e)| {
                self.ctx.blocked.contains(*r)
                    || self
                        .ctx
                        .removed_owners
                        .contains(&(**r, peer_id_hash(&e.ann.owner_pk)))
                    || self
                        .ctx
                        .roster_owner
                        .get(*r)
                        .is_some_and(|o| *o != e.ann.owner_pk)
            })
            .map(|(r, _)| *r)
            .collect();
        for r in bad {
            self.removed(&r, &mut out);
        }
        let blocked = self.ctx.blocked.clone();
        self.delists.retain(|r, d| {
            let keep = !blocked.contains(r);
            if !keep {
                out.push(d.envelope_id);
            }
            keep
        });
        out
    }

    pub fn context(&self) -> &DiscoveryContext {
        &self.ctx
    }

    /// Local hide / unhide (never sent).
    pub fn set_hidden(&mut self, cref: [u8; 32], hidden: bool) {
        if hidden {
            self.hidden.insert(cref);
        } else {
            self.hidden.remove(&cref);
        }
    }

    /// Item 6: the joining client found `community_pk` / `owner_pk` do not
    /// match the roster. Drop it and stop relaying this ref.
    pub fn distrust(&mut self, cref: [u8; 32]) -> Vec<[u8; 16]> {
        self.distrusted.insert(cref);
        let mut out = Vec::new();
        self.removed(&cref, &mut out);
        out
    }

    pub fn near_you(e: &Entry, now_ms: u64) -> bool {
        e.near_heard_ms
            .is_some_and(|t| now_ms.saturating_sub(t) <= NEAR_MS)
    }

    /// Local search (item 7). Empty `query` browses. `category` filters by
    /// display category; `lang` by announced language.
    pub fn search(
        &self,
        query: &str,
        category: Option<u8>,
        lang: Option<&str>,
        now_ms: u64,
    ) -> Vec<Found<'_>> {
        let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
        let mut out: Vec<Found<'_>> = self
            .entries
            .iter()
            .filter(|(r, e)| {
                !self.hidden.contains(*r)
                    && !self.ctx.reported.contains(*r)
                    && !self.ctx.blocked.contains(*r)
                    && e.ann.issued_ms.saturating_add(EXPIRY_MS) > now_ms
            })
            .filter(|(_, e)| {
                category.is_none_or(|c| {
                    e.ann
                        .categories
                        .iter()
                        .any(|x| display_category(*x) == display_category(c))
                })
            })
            .filter(|(_, e)| {
                lang.is_none_or(|l| e.ann.langs.iter().any(|x| x.eq_ignore_ascii_case(l)))
            })
            .filter_map(|(_, e)| {
                let name = e.ann.name.to_lowercase();
                let desc = e.ann.description.to_lowercase();
                let mut score = 0u32;
                for w in &words {
                    let s = if name.contains(w.as_str()) {
                        3
                    } else if e.ann.tags.iter().any(|t| t.contains(w.as_str())) {
                        2
                    } else if desc.contains(w.as_str()) {
                        1
                    } else {
                        return None;
                    };
                    score += s;
                }
                Some(Found {
                    ann: &e.ann,
                    near_you: Self::near_you(e, now_ms),
                    hops: e.hops,
                    score,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(b.near_you.cmp(&a.near_you))
                .then(a.hops.cmp(&b.hops))
                .then(b.ann.issued_ms.cmp(&a.ann.issued_ms))
                .then(b.ann.member_bucket.cmp(&a.ann.member_bucket))
        });
        out
    }
}

/// Per-neighbour pacing for discovery-tag ranges (§20.8(4)(a)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveryPacing {
    /// Monotonic generation; bump when the local index changes.
    pub gen: u64,
    /// link → (last_sent_ms, gen_at_send).
    last: BTreeMap<u64, (u64, u64)>,
}

impl DiscoveryPacing {
    pub fn bump(&mut self) {
        self.gen = self.gen.saturating_add(1);
    }

    /// Whether discovery ranges may be included for `link` at `now_ms`.
    pub fn may_send(&self, link: u64, now_ms: u64) -> bool {
        match self.last.get(&link) {
            None => true,
            Some((sent, g)) if *g != self.gen => true,
            Some((sent, _)) => now_ms.saturating_sub(*sent) >= DISCOVERY_PACING_MS,
        }
    }

    pub fn mark_sent(&mut self, link: u64, now_ms: u64) {
        self.last.insert(link, (now_ms, self.gen));
    }
}

#[cfg(test)]
mod pacing_tests {
    use super::*;

    #[test]
    fn discovery_ranges_paced_per_neighbour_unless_index_changed() {
        let mut p = DiscoveryPacing::default();
        assert!(p.may_send(1, 0));
        p.mark_sent(1, 0);
        assert!(!p.may_send(1, DISCOVERY_PACING_MS - 1));
        assert!(p.may_send(1, DISCOVERY_PACING_MS));
        assert!(p.may_send(2, 0), "other neighbour unaffected");
        p.mark_sent(1, DISCOVERY_PACING_MS);
        p.bump(); // local index changed
        assert!(p.may_send(1, DISCOVERY_PACING_MS + 1));
    }
}

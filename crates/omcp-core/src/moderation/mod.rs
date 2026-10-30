//! App-team moderation (PROTOCOL §23.9, §23.10; REQUIREMENTS TM1–TM12).
//!
//! # Trust
//!
//! The app ships a [`TeamPolicy`]: a whitelist of team PeerID hashes
//! (`BLAKE2b-256(Ed25519 public key)`) and a threshold `k`. The policy only
//! changes with an app release, never over the mesh. The shipped file is
//! `config/team_moderation.conf`; its production `team_key` line stays empty
//! until v1.0, so release builds ignore every team action (§23.10.8).
//!
//! # Actions and signed bodies
//!
//! Kinds 1 `TAKEDOWN` and 2 `RESTORE` (whole community) keep the v1 body:
//!
//! ```text
//! "OMCP-team-mod-v1" ‖ 0x00 ‖ kind u8 ‖ community_id[32] ‖ epoch u64 BE
//!   ‖ reason_code u8 (0 for RESTORE) ‖ note_len u16 BE ‖ note
//! ```
//!
//! Kinds 3–8 use the v2 body (`TeamActionV2`, §23.10.7):
//!
//! ```text
//! "OMCP-team-mod-v2" ‖ 0x00 ‖ kind u8 ‖ target_id[32] ‖ item_id[32]
//!   ‖ epoch u64 BE ‖ reason_code u8 ‖ evidence_hash[32] ‖ note_len u16 BE ‖ note
//! ```
//!
//! 3 `GROUP_TAKEDOWN` / 4 `GROUP_RESTORE` target a private group by its
//! [`group_ref`] only (never members, senders, epochs or secrets, MR18);
//! 5 `POST_TAKEDOWN`, 6 `TOPIC_TAKEDOWN`, 7 `LIVE_TAKEDOWN` and 8
//! `ITEM_RESTORE` target `(community_id, item_id)`.
//!
//! Wire form: the body fields without the domain prefix, then
//! `n u8 ‖ n × (public_key[32] ‖ ed25519_signature[64])`.
//!
//! # Ordering, erasure and tombstones
//!
//! Each `(target, item)` has one moderation epoch; an action applies only if
//! its epoch is strictly greater. Every takedown is a full erasure on the
//! device (the caller deletes content, media, keys and backup portions); this
//! module records the [`Tombstone`] that must be applied before any backup
//! restore. Restores only lift the ban; content never comes back.
//! Tombstones are kept while the takedown is in force (≥ 2 years, §23.10.2(4)).
//!
//! # Envelope
//!
//! A public [`EnvelopeV1`] with `flags = RELAY_OK | STORE_OK | PRIORITY`, the
//! well-known tag [`team_mod_tag`], `header_aad` TLV `0x5A` `team_mod`
//! (len 1, value = body version 1 or 2) and an outer expiry of 30 d.
//!
//! # Scope (TM4, MR10)
//!
//! No action ever touches a 1:1 chat or reads any private chat. Team keys
//! never get an MLS role (§23.10.5): see [`TeamPolicy::reject_team_identities`].

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{parse_routing_aad, put_tag_in_aad, MailboxTag};

pub mod evidence;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_groups;

const DOMAIN_V1: &[u8] = b"OMCP-team-mod-v1\x00";
const DOMAIN_V2: &[u8] = b"OMCP-team-mod-v2\x00";
const ID_DOMAIN: &[u8] = b"OMCP-team-mod-id-v1\x00";
const TAG_DOMAIN: &[u8] = b"OMCP-team-mod-tag-v1\x00";
const GROUP_REF_DOMAIN: &[u8] = b"OMCP-group-ref-v1\x00";
/// `header_aad` TLV `team_mod` (§21.8, §23.10.9).
pub const AAD_TLV_TEAM_MOD: u8 = 0x5A;
/// Outer expiry of a moderation envelope (§23.9(2)).
pub const TEAM_MOD_EXPIRY_MS: u64 = 30 * 24 * 60 * 60 * 1000;
/// Tombstones are kept at least this long (§23.10.2(4)).
pub const TOMBSTONE_MIN_RETENTION_MS: u64 = 2 * 365 * 24 * 60 * 60 * 1000;
const ENVELOPE_FLAGS: u16 = flags::RELAY_OK | flags::STORE_OK | flags::PRIORITY;
const MAX_NOTE_V1: usize = 500;
const MAX_NOTE_V2: usize = 512;
const MAX_SIGS: usize = 16;
const ZERO: [u8; 32] = [0u8; 32];

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

pub(crate) fn blake2b(parts: &[&[u8]], out: &mut [u8]) {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(out.len()).expect("valid BLAKE2b output size");
    for p in parts {
        h.update(p);
    }
    h.finalize_variable(out).expect("output size matches");
}

pub(crate) fn h256(parts: &[&[u8]]) -> [u8; 32] {
    let mut o = [0u8; 32];
    blake2b(parts, &mut o);
    o
}

/// PeerID hash of an Ed25519 public key: `BLAKE2b-256(key)` (§22).
pub fn peer_id_hash(public_key: &[u8; 32]) -> [u8; 32] {
    h256(&[public_key])
}

/// Plain BLAKE2b-256 of some bytes (e.g. a GroupContext hash).
pub fn peer_id_hash_bytes(b: &[u8]) -> [u8; 32] {
    h256(&[b])
}

/// `group_ref = BLAKE2b-256("OMCP-group-ref-v1" ‖ 0x00 ‖ group_id)` (§23.10).
pub fn group_ref(group_id: &[u8]) -> [u8; 32] {
    h256(&[GROUP_REF_DOMAIN, group_id])
}

/// The one public tag all compliant clients subscribe to.
pub fn team_mod_tag() -> MailboxTag {
    let mut o = [0u8; 8];
    blake2b(&[TAG_DOMAIN], &mut o);
    o
}

/// §23.10.4(3) reason categories (extends §23.9(7)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    Hate = 1,
    Illegal = 2,
    Terror = 3,
    Harassment = 4,
    Spam = 5,
    Csam = 6,
    Threats = 7,
    /// Restore after review or appeal (log category; not on a takedown).
    RestoreAfterReview = 8,
    Other = 9,
}

impl Reason {
    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            1 => Self::Hate,
            2 => Self::Illegal,
            3 => Self::Terror,
            4 => Self::Harassment,
            5 => Self::Spam,
            6 => Self::Csam,
            7 => Self::Threats,
            8 => Self::RestoreAfterReview,
            9 => Self::Other,
            _ => return None,
        })
    }
    pub fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    Takedown = 1,
    Restore = 2,
    GroupTakedown = 3,
    GroupRestore = 4,
    PostTakedown = 5,
    TopicTakedown = 6,
    LiveTakedown = 7,
    ItemRestore = 8,
}

impl ActionKind {
    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            1 => Self::Takedown,
            2 => Self::Restore,
            3 => Self::GroupTakedown,
            4 => Self::GroupRestore,
            5 => Self::PostTakedown,
            6 => Self::TopicTakedown,
            7 => Self::LiveTakedown,
            8 => Self::ItemRestore,
            _ => return None,
        })
    }
    /// Body version: 1 for kinds 1–2, 2 for kinds 3–8.
    pub fn version(self) -> u8 {
        if (self as u8) <= 2 {
            1
        } else {
            2
        }
    }
    pub fn is_takedown(self) -> bool {
        matches!(
            self,
            Self::Takedown
                | Self::GroupTakedown
                | Self::PostTakedown
                | Self::TopicTakedown
                | Self::LiveTakedown
        )
    }
    pub fn is_group(self) -> bool {
        matches!(self, Self::GroupTakedown | Self::GroupRestore)
    }
    /// Kinds that name an item inside a community (5–8).
    pub fn is_item(self) -> bool {
        (self as u8) >= 5
    }
}

/// Left-pad a short id (e.g. a 16-byte stream id) to 32 bytes (§23.10.6).
pub fn pad_id(id: &[u8]) -> [u8; 32] {
    let mut t = [0u8; 32];
    let n = id.len().min(32);
    t[32 - n..].copy_from_slice(&id[id.len() - n..]);
    t
}

/// One team signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSig {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

/// A signed team action (§23.9, §23.10.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamAction {
    pub kind: ActionKind,
    /// community_id (kinds 1, 2, 5–8) or group_ref (3, 4).
    pub target_id: [u8; 32],
    /// Zero for kinds 1–4; post/topic/stream id for 5–8.
    pub item_id: [u8; 32],
    /// `None` for restores.
    pub reason: Option<Reason>,
    /// BLAKE2b-256 of the evidence file, all-zero if none (v2 only).
    pub evidence_hash: [u8; 32],
    pub note: String,
    pub epoch: u64,
    pub sigs: Vec<TeamSig>,
}

impl TeamAction {
    fn new(
        kind: ActionKind,
        target_id: [u8; 32],
        item_id: [u8; 32],
        reason: Option<Reason>,
        note: &str,
        epoch: u64,
    ) -> Self {
        Self {
            kind,
            target_id,
            item_id,
            reason,
            evidence_hash: ZERO,
            note: note.to_string(),
            epoch,
            sigs: Vec::new(),
        }
    }
    pub fn takedown(community_id: [u8; 32], reason: Reason, note: &str, epoch: u64) -> Self {
        Self::new(
            ActionKind::Takedown,
            community_id,
            ZERO,
            Some(reason),
            note,
            epoch,
        )
    }
    pub fn restore(community_id: [u8; 32], epoch: u64) -> Self {
        Self::new(ActionKind::Restore, community_id, ZERO, None, "", epoch)
    }
    pub fn group_takedown(
        group_ref: [u8; 32],
        reason: Reason,
        evidence_hash: [u8; 32],
        note: &str,
        epoch: u64,
    ) -> Self {
        let mut a = Self::new(
            ActionKind::GroupTakedown,
            group_ref,
            ZERO,
            Some(reason),
            note,
            epoch,
        );
        a.evidence_hash = evidence_hash;
        a
    }
    pub fn group_restore(group_ref: [u8; 32], epoch: u64) -> Self {
        Self::new(ActionKind::GroupRestore, group_ref, ZERO, None, "", epoch)
    }
    pub fn post_takedown(
        community_id: [u8; 32],
        post_id: [u8; 32],
        reason: Reason,
        note: &str,
        epoch: u64,
    ) -> Self {
        Self::new(
            ActionKind::PostTakedown,
            community_id,
            post_id,
            Some(reason),
            note,
            epoch,
        )
    }
    pub fn topic_takedown(
        community_id: [u8; 32],
        topic_id: [u8; 32],
        reason: Reason,
        note: &str,
        epoch: u64,
    ) -> Self {
        Self::new(
            ActionKind::TopicTakedown,
            community_id,
            topic_id,
            Some(reason),
            note,
            epoch,
        )
    }
    /// Take down one live stream of a community (stops hosts, viewers, relays).
    pub fn live_takedown(
        community_id: [u8; 32],
        stream_id: [u8; 16],
        reason: Reason,
        note: &str,
        epoch: u64,
    ) -> Self {
        Self::new(
            ActionKind::LiveTakedown,
            community_id,
            pad_id(&stream_id),
            Some(reason),
            note,
            epoch,
        )
    }
    /// Lift the ban of an earlier kind 5/6/7 on `(community_id, item_id)`.
    pub fn item_restore(community_id: [u8; 32], item_id: [u8; 32], epoch: u64) -> Self {
        Self::new(
            ActionKind::ItemRestore,
            community_id,
            item_id,
            None,
            "",
            epoch,
        )
    }
    pub fn with_evidence(mut self, evidence_hash: [u8; 32]) -> Self {
        self.evidence_hash = evidence_hash;
        self
    }

    fn fields(&self) -> Vec<u8> {
        let mut b = vec![self.kind as u8];
        b.extend_from_slice(&self.target_id);
        if self.kind.version() == 2 {
            b.extend_from_slice(&self.item_id);
        }
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.push(self.reason.map(Reason::code).unwrap_or(0));
        if self.kind.version() == 2 {
            b.extend_from_slice(&self.evidence_hash);
        }
        b.extend_from_slice(&(self.note.len() as u16).to_be_bytes());
        b.extend_from_slice(self.note.as_bytes());
        b
    }

    /// The bytes each team key signs.
    pub fn signed_body(&self) -> Vec<u8> {
        let mut b = if self.kind.version() == 2 {
            DOMAIN_V2
        } else {
            DOMAIN_V1
        }
        .to_vec();
        b.extend_from_slice(&self.fields());
        b
    }

    /// Field rules independent of signatures.
    pub fn check_fields(&self) -> Result<()> {
        let max = if self.kind.version() == 2 {
            MAX_NOTE_V2
        } else {
            MAX_NOTE_V1
        };
        if self.note.len() > max {
            return err("team action note too long");
        }
        if self.kind.version() == 1 && (self.item_id != ZERO || self.evidence_hash != ZERO) {
            return err("v1 action has no item or evidence");
        }
        if self.kind.is_group() && self.item_id != ZERO {
            return err("group action item must be zero");
        }
        if self.kind.is_item() && self.item_id == ZERO {
            return err("item action needs an item id");
        }
        match (self.kind.is_takedown(), self.reason) {
            (true, None) => err("takedown needs a reason"),
            (true, Some(Reason::RestoreAfterReview)) => err("reason 8 is for restores"),
            (true, Some(Reason::Other)) if self.note.trim().is_empty() => {
                err("reason 9 (other) needs a note")
            }
            (false, Some(_)) => err("restore carries no reason"),
            _ => Ok(()),
        }
    }

    /// Add a signature (done offline by the team key holder).
    pub fn sign(&mut self, key: &SigningKey) {
        let signature = key.sign(&self.signed_body()).to_bytes();
        self.sigs.push(TeamSig {
            public_key: key.verifying_key().to_bytes(),
            signature,
        });
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.check_fields()?;
        if self.sigs.len() > MAX_SIGS {
            return err("too many team signatures");
        }
        let mut b = self.fields();
        b.push(self.sigs.len() as u8);
        for s in &self.sigs {
            b.extend_from_slice(&s.public_key);
            b.extend_from_slice(&s.signature);
        }
        Ok(b)
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader(b);
        let kind =
            ActionKind::from_code(r.u8()?).ok_or(OmcpError::Sync("unknown team action kind"))?;
        let v2 = kind.version() == 2;
        let target_id: [u8; 32] = r.take(32)?.try_into().expect("32");
        let item_id: [u8; 32] = if v2 {
            r.take(32)?.try_into().expect("32")
        } else {
            ZERO
        };
        let epoch = u64::from_be_bytes(r.take(8)?.try_into().expect("8"));
        let rc = r.u8()?;
        let reason = match (kind.is_takedown(), rc) {
            (false, 0) => None,
            (true, c) => Some(Reason::from_code(c).ok_or(OmcpError::Sync("unknown reason code"))?),
            _ => return err("restore carries no reason"),
        };
        let evidence_hash: [u8; 32] = if v2 {
            r.take(32)?.try_into().expect("32")
        } else {
            ZERO
        };
        let nl = u16::from_be_bytes(r.take(2)?.try_into().expect("2")) as usize;
        if nl > MAX_NOTE_V2 {
            return err("team action note too long");
        }
        let note = std::str::from_utf8(r.take(nl)?)
            .map_err(|_| OmcpError::Sync("team action note utf-8"))?
            .to_string();
        let n = r.u8()? as usize;
        if n > MAX_SIGS {
            return err("too many team signatures");
        }
        let mut sigs = Vec::with_capacity(n);
        for _ in 0..n {
            let public_key = r.take(32)?.try_into().expect("32");
            let signature = r.take(64)?.try_into().expect("64");
            sigs.push(TeamSig {
                public_key,
                signature,
            });
        }
        if !r.0.is_empty() {
            return err("trailing bytes in team action");
        }
        let a = Self {
            kind,
            target_id,
            item_id,
            reason,
            evidence_hash,
            note,
            epoch,
            sigs,
        };
        a.check_fields()?;
        Ok(a)
    }
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.0.len() < n {
            return err("team action truncated");
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
}

/// Team whitelist + threshold shipped with the app release (§23.9(1), (3)).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamPolicy {
    pub members: BTreeSet<[u8; 32]>,
    pub k: usize,
}

impl TeamPolicy {
    pub fn from_public_keys(keys: &[[u8; 32]], k: usize) -> Self {
        Self {
            members: keys.iter().map(peer_id_hash).collect(),
            k,
        }
    }

    /// Parse the shipped config (`config/team_moderation.conf`):
    /// `k = <n>` and one `team_key = <Ed25519 public key, 64 hex>` per member;
    /// `#` starts a comment. No team keys is valid and means "this app has no
    /// moderation team" (TM4, TM11): every action is ignored.
    pub fn from_config(text: &str) -> Result<Self> {
        let mut k = None;
        let mut keys = Vec::new();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((name, val)) = line.split_once('=') else {
                return err("team policy line");
            };
            match name.trim() {
                "k" => {
                    k = Some(
                        val.trim()
                            .parse::<usize>()
                            .map_err(|_| OmcpError::Sync("team threshold"))?,
                    )
                }
                "team_key" => {
                    let key = hex32(val.trim()).ok_or(OmcpError::Sync("team key hex"))?;
                    VerifyingKey::from_bytes(&key)
                        .map_err(|_| OmcpError::Sync("team key point"))?;
                    keys.push(key);
                }
                _ => return err("unknown team policy setting"),
            }
        }
        let k = k.unwrap_or(1);
        if !keys.is_empty() && (k == 0 || k > keys.len()) {
            return err("team threshold out of range");
        }
        Ok(Self::from_public_keys(&keys, k))
    }

    /// Hard rule (§23.10.5(1), MR17): refuse any MLS Add / KeyPackage /
    /// Welcome that carries a whitelisted team identity (PeerID hash).
    pub fn reject_team_identities<'a>(
        &self,
        identities: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<()> {
        for id in identities {
            if let Ok(h) = <[u8; 32]>::try_from(id) {
                if self.members.contains(&h) {
                    return err("team key may not be a group member");
                }
            }
        }
        Ok(())
    }

    /// Signers that count toward `k`: distinct whitelisted keys with a valid
    /// signature over the body (MR8). Returns their PeerID hashes.
    pub fn verify(&self, a: &TeamAction) -> std::result::Result<Vec<[u8; 32]>, Rejected> {
        if self.members.is_empty() || self.k == 0 {
            return Err(Rejected::NoTeam);
        }
        a.check_fields().map_err(|_| Rejected::Malformed)?;
        let body = a.signed_body();
        let mut ok = BTreeSet::new();
        let mut any_bad = false;
        let mut any_foreign = false;
        for s in &a.sigs {
            let h = peer_id_hash(&s.public_key);
            if !self.members.contains(&h) {
                any_foreign = true;
                continue;
            }
            let valid = VerifyingKey::from_bytes(&s.public_key)
                .and_then(|vk| vk.verify_strict(&body, &Signature::from_bytes(&s.signature)))
                .is_ok();
            if valid {
                ok.insert(h);
            } else {
                any_bad = true;
            }
        }
        if ok.len() >= self.k {
            Ok(ok.into_iter().collect())
        } else if any_bad {
            Err(Rejected::BadSignature)
        } else if any_foreign {
            Err(Rejected::NotWhitelisted)
        } else {
            Err(Rejected::BelowThreshold)
        }
    }
}

/// Why an action was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    NoTeam,
    Malformed,
    BadSignature,
    NotWhitelisted,
    BelowThreshold,
    /// Epoch not newer than the last applied action for this target.
    Replay,
}

/// One public log line (§23.9(5), §23.10.4(2)): only an evidence fingerprint
/// and a reason category, never evidence, names or members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub decision_id: [u8; 16],
    pub kind: ActionKind,
    pub target_id: [u8; 32],
    pub item_id: [u8; 32],
    pub reason_code: u8,
    pub evidence_hash: [u8; 32],
    pub note: String,
    pub epoch: u64,
    /// PeerID hashes of the team keys that signed.
    pub signers: Vec<[u8; 32]>,
    pub applied_ms: u64,
}

/// Moderation key space: communities (and their items) vs private groups.
const SPACE_COMMUNITY: u8 = 0;
const SPACE_GROUP: u8 = 1;
type ModKey = (u8, [u8; 32], [u8; 32]);

fn mod_key(a: &TeamAction) -> ModKey {
    let space = if a.kind.is_group() {
        SPACE_GROUP
    } else {
        SPACE_COMMUNITY
    };
    (space, a.target_id, a.item_id)
}

/// A takedown in force (§23.10.2(3d), §23.10.6). Kept in backups and applied
/// before any restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    pub kind: ActionKind,
    pub target_id: [u8; 32],
    pub item_id: [u8; 32],
    pub epoch: u64,
    pub reason_code: u8,
    pub applied_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ItemMod {
    kind: ActionKind,
    epoch: u64,
    removed: bool,
    reason_code: u8,
    note: String,
    applied_ms: u64,
}

/// Per-device moderation state (serde; the FFI keeps it in the encrypted store).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationState {
    items: BTreeMap<ModKey, ItemMod>,
    log: Vec<LogEntry>,
    /// Accepted actions as wire bytes, to re-gossip while in force.
    wire: BTreeMap<ModKey, Vec<u8>>,
}

/// What a client shows for a removed item (§23.9(4)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub reason: Reason,
    pub note: String,
    pub epoch: u64,
}

impl ModerationState {
    /// Verify and apply one action. On success the log gains a line; for a
    /// takedown the caller MUST then erase the target (see [`Self::tombstones`]).
    pub fn apply(
        &mut self,
        policy: &TeamPolicy,
        a: &TeamAction,
        now_ms: u64,
    ) -> std::result::Result<LogEntry, Rejected> {
        let signers = policy.verify(a)?;
        let key = mod_key(a);
        if let Some(c) = self.items.get(&key) {
            if a.epoch <= c.epoch {
                return Err(Rejected::Replay);
            }
        }
        let reason_code = a.reason.map(Reason::code).unwrap_or(0);
        self.items.insert(
            key,
            ItemMod {
                kind: a.kind,
                epoch: a.epoch,
                removed: a.kind.is_takedown(),
                reason_code,
                note: a.note.clone(),
                applied_ms: now_ms,
            },
        );
        let w = a.encode().map_err(|_| Rejected::Malformed)?;
        let decision_id = envelope_id(&w);
        self.wire.insert(key, w);
        let e = LogEntry {
            decision_id,
            kind: a.kind,
            target_id: a.target_id,
            item_id: a.item_id,
            reason_code,
            evidence_hash: a.evidence_hash,
            note: a.note.clone(),
            epoch: a.epoch,
            signers,
            applied_ms: now_ms,
        };
        self.log.push(e.clone());
        Ok(e)
    }

    fn removal_at(&self, key: &ModKey) -> Option<Removal> {
        let c = self.items.get(key)?;
        if !c.removed {
            return None;
        }
        Some(Removal {
            reason: Reason::from_code(c.reason_code)?,
            note: c.note.clone(),
            epoch: c.epoch,
        })
    }

    /// `Some` if compliant clients must erase and hide the community, refuse
    /// joins and posts, and relays must drop its envelopes.
    pub fn removal(&self, community_id: &[u8; 32]) -> Option<Removal> {
        self.removal_at(&(SPACE_COMMUNITY, *community_id, ZERO))
    }

    pub fn is_removed(&self, community_id: &[u8; 32]) -> bool {
        self.removal(community_id).is_some()
    }

    /// Private group takedown in force for this `group_ref`.
    pub fn group_removal(&self, group_ref: &[u8; 32]) -> Option<Removal> {
        self.removal_at(&(SPACE_GROUP, *group_ref, ZERO))
    }

    pub fn is_group_removed(&self, group_ref: &[u8; 32]) -> bool {
        self.group_removal(group_ref).is_some()
    }

    /// Post / topic / live takedown in force on `(community_id, item_id)`.
    pub fn item_removal(&self, community_id: &[u8; 32], item_id: &[u8; 32]) -> Option<Removal> {
        self.removal_at(&(SPACE_COMMUNITY, *community_id, *item_id))
    }

    pub fn is_item_removed(&self, community_id: &[u8; 32], item_id: &[u8; 32]) -> bool {
        self.item_removal(community_id, item_id).is_some()
    }

    fn removed_lives(&self) -> impl Iterator<Item = (&ModKey, &ItemMod)> {
        self.items
            .iter()
            .filter(|(_, c)| c.removed && c.kind == ActionKind::LiveTakedown)
    }

    /// `Some` if the live stream was taken down: hosts stop, viewers stop
    /// playing, relays stop forwarding its audio and video.
    pub fn live_removal(&self, stream_id: &[u8; 16]) -> Option<Removal> {
        let item = pad_id(stream_id);
        let (k, _) = self.removed_lives().find(|((_, _, i), _)| *i == item)?;
        self.removal_at(k)
    }

    pub fn is_live_removed(&self, stream_id: &[u8; 16]) -> bool {
        self.live_removal(stream_id).is_some()
    }

    /// Public audio tags of taken-down streams (relays drop these packets).
    pub fn removed_live_tags(&self) -> BTreeSet<[u8; 8]> {
        self.removed_lives()
            .map(|((_, _, i), _)| crate::live::stream_tag(&i[16..].try_into().expect("16")))
            .collect()
    }

    /// Whether a live may be played, hosted or relayed: neither the stream
    /// nor its community (if known) is taken down.
    pub fn live_allowed(&self, stream_id: &[u8; 16], community_id: Option<&[u8; 32]>) -> bool {
        !self.is_live_removed(stream_id) && self.relay_allows(community_id)
    }

    /// Relay rule (§23.9(4)): drop envelopes of removed communities.
    pub fn relay_allows(&self, community_id: Option<&[u8; 32]>) -> bool {
        community_id.map(|c| !self.is_removed(c)).unwrap_or(true)
    }

    /// Takedowns in force. Written into every backup and applied before any
    /// restore writes to the chat store (§23.10.2(4)).
    pub fn tombstones(&self) -> Vec<Tombstone> {
        self.items
            .iter()
            .filter(|(_, c)| c.removed)
            .map(|((_, t, i), c)| Tombstone {
                kind: c.kind,
                target_id: *t,
                item_id: *i,
                epoch: c.epoch,
                reason_code: c.reason_code,
                applied_ms: c.applied_ms,
            })
            .collect()
    }

    /// Merge tombstones from a backup being restored (highest epoch wins;
    /// a newer restore known on this device is kept).
    pub fn merge_tombstones(&mut self, ts: &[Tombstone]) {
        for t in ts {
            if !t.kind.is_takedown() {
                continue;
            }
            let space = if t.kind.is_group() {
                SPACE_GROUP
            } else {
                SPACE_COMMUNITY
            };
            let key = (space, t.target_id, t.item_id);
            if self
                .items
                .get(&key)
                .map(|c| c.epoch >= t.epoch)
                .unwrap_or(false)
            {
                continue;
            }
            self.items.insert(
                key,
                ItemMod {
                    kind: t.kind,
                    epoch: t.epoch,
                    removed: true,
                    reason_code: t.reason_code,
                    note: String::new(),
                    applied_ms: t.applied_ms,
                },
            );
        }
    }

    /// Transparency log, newest first.
    pub fn log(&self) -> Vec<LogEntry> {
        let mut v = self.log.clone();
        v.reverse();
        v
    }

    /// Latest accepted action per target (for re-gossip).
    pub fn current_actions(&self) -> Vec<Vec<u8>> {
        self.wire.values().cloned().collect()
    }
}

fn envelope_id(wire: &[u8]) -> [u8; 16] {
    let mut o = [0u8; 16];
    blake2b(&[ID_DOMAIN, wire], &mut o);
    o
}

/// Wrap a signed action in a public mesh envelope.
pub fn seal_team_action(a: &TeamAction, now_ms: u64, ttl_hops: u8) -> Result<EnvelopeV1> {
    let wire = a.encode()?;
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &team_mod_tag());
    put_expires_in_aad(&mut header_aad, now_ms.saturating_add(TEAM_MOD_EXPIRY_MS));
    header_aad.extend_from_slice(&[AAD_TLV_TEAM_MOD, 1, a.kind.version()]);
    Ok(EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: envelope_id(&wire),
        flags: ENVELOPE_FLAGS,
        ttl_hops,
        created_ms: now_ms,
        header_aad,
        ciphertext: wire,
    })
}

/// Whether the envelope claims to be a team moderation envelope.
pub fn is_team_mod_envelope(env: &EnvelopeV1) -> bool {
    let a = &env.header_aad;
    let mut i = 0;
    while i + 2 <= a.len() {
        if a[i] == AAD_TLV_TEAM_MOD {
            return true;
        }
        i += 2 + a[i + 1] as usize;
    }
    false
}

/// Parse a moderation envelope (signatures are checked by [`ModerationState::apply`]).
pub fn open_team_action(env: &EnvelopeV1) -> Result<TeamAction> {
    if !is_team_mod_envelope(env) {
        return err("not a team moderation envelope");
    }
    if parse_routing_aad(&env.header_aad)?.tag != Some(team_mod_tag()) {
        return err("team moderation envelope on the wrong tag");
    }
    if env.envelope_id != envelope_id(&env.ciphertext) {
        return err("team moderation envelope id");
    }
    TeamAction::decode(&env.ciphertext)
}

/// Parse 64 hex chars into 32 bytes.
pub fn hex32(s: &str) -> Option<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 64 {
        return None;
    }
    let nib = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut o = [0u8; 32];
    for (i, o) in o.iter_mut().enumerate() {
        *o = nib(b[2 * i])? << 4 | nib(b[2 * i + 1])?;
    }
    Some(o)
}

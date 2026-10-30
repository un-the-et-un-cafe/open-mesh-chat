//! App-team moderation: signed `TAKEDOWN` / `RESTORE` of public communities
//! (PROTOCOL §23.9, REQUIREMENTS TM1–TM4).
//!
//! # Trust
//!
//! The app ships a [`TeamPolicy`]: a whitelist of team PeerID hashes
//! (`BLAKE2b-256(Ed25519 public key)`) and a threshold `k`. The policy only
//! changes with an app release, never over the mesh. Hopla ships `k = 1` with
//! one maintainer key (see `config/team_moderation.json`, docs/MODERATION.md).
//!
//! # Signed body
//!
//! ```text
//! "OMCP-team-mod-v1" ‖ 0x00 ‖ kind u8 (1 TAKEDOWN, 2 RESTORE) ‖ community_id[32]
//!   ‖ epoch u64 BE ‖ reason_code u8 (0 for RESTORE) ‖ note_len u16 BE ‖ note (UTF-8)
//! ```
//!
//! Wire form of an action: the body fields without the domain prefix, then
//! `n u8 ‖ n × (public_key[32] ‖ ed25519_signature[64])`.
//!
//! # Ordering and replay
//!
//! Each community has one moderation epoch. An action is applied only if its
//! epoch is strictly greater than the last applied epoch for that community:
//! a replayed or older TAKEDOWN cannot undo a newer RESTORE, and a RESTORE only
//! lifts a TAKEDOWN with a lower epoch. The highest epoch always wins, so the
//! result does not depend on gossip order.
//!
//! # Envelope
//!
//! A public [`EnvelopeV1`] with `flags = RELAY_OK | STORE_OK | PRIORITY`, the
//! well-known tag [`team_mod_tag`] every compliant client wants, TLV `0x8F`
//! (marker) and an outer expiry of 30 d (relays cap storage at 7 d; the
//! sender re-sends while the takedown stays in force). The ciphertext field
//! carries the cleartext action: it is public and authenticated by the team
//! signatures, not by AEAD.
//!
//! # Scope (TM4, MR10)
//!
//! This module only ever answers "is community X removed?". It never sees,
//! reads or deletes 1:1 or group chats.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{parse_routing_aad, put_tag_in_aad, MailboxTag};

#[cfg(test)]
mod tests;

const DOMAIN: &[u8] = b"OMCP-team-mod-v1\x00";
const ID_DOMAIN: &[u8] = b"OMCP-team-mod-id-v1\x00";
const TAG_DOMAIN: &[u8] = b"OMCP-team-mod-tag-v1\x00";
/// `header_aad` TLV marking a team moderation envelope (experimental range).
pub const AAD_TLV_TEAM_MOD: u8 = 0x8F;
/// Outer expiry of a moderation envelope (§23.9(2)).
pub const TEAM_MOD_EXPIRY_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const ENVELOPE_FLAGS: u16 = flags::RELAY_OK | flags::STORE_OK | flags::PRIORITY;
const MAX_NOTE: usize = 500;
const MAX_SIGS: usize = 16;

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

fn blake2b(parts: &[&[u8]], out: &mut [u8]) {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(out.len()).expect("valid BLAKE2b output size");
    for p in parts {
        h.update(p);
    }
    h.finalize_variable(out).expect("output size matches");
}

/// PeerID hash of an Ed25519 public key: `BLAKE2b-256(key)` (§22).
pub fn peer_id_hash(public_key: &[u8; 32]) -> [u8; 32] {
    let mut o = [0u8; 32];
    blake2b(&[public_key], &mut o);
    o
}

/// The one public tag all compliant clients subscribe to.
pub fn team_mod_tag() -> MailboxTag {
    let mut o = [0u8; 8];
    blake2b(&[TAG_DOMAIN], &mut o);
    o
}

/// §23.9(7) reason codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reason {
    Hate = 1,
    Illegal = 2,
    Terror = 3,
    Harassment = 4,
    Spam = 5,
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
}

/// One team signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSig {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

/// A `TAKEDOWN` or `RESTORE` (§23.9(2), (6)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamAction {
    pub kind: ActionKind,
    pub community_id: [u8; 32],
    /// `None` for RESTORE.
    pub reason: Option<Reason>,
    pub note: String,
    pub epoch: u64,
    pub sigs: Vec<TeamSig>,
}

impl TeamAction {
    pub fn takedown(community_id: [u8; 32], reason: Reason, note: &str, epoch: u64) -> Self {
        Self {
            kind: ActionKind::Takedown,
            community_id,
            reason: Some(reason),
            note: note.to_string(),
            epoch,
            sigs: Vec::new(),
        }
    }

    pub fn restore(community_id: [u8; 32], epoch: u64) -> Self {
        Self {
            kind: ActionKind::Restore,
            community_id,
            reason: None,
            note: String::new(),
            epoch,
            sigs: Vec::new(),
        }
    }

    fn fields(&self) -> Vec<u8> {
        let mut b = vec![self.kind as u8];
        b.extend_from_slice(&self.community_id);
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.push(self.reason.map(Reason::code).unwrap_or(0));
        b.extend_from_slice(&(self.note.len() as u16).to_be_bytes());
        b.extend_from_slice(self.note.as_bytes());
        b
    }

    /// The bytes each team key signs.
    pub fn signed_body(&self) -> Vec<u8> {
        let mut b = DOMAIN.to_vec();
        b.extend_from_slice(&self.fields());
        b
    }

    /// Field rules independent of signatures.
    pub fn check_fields(&self) -> Result<()> {
        if self.note.len() > MAX_NOTE {
            return err("team action note too long");
        }
        match (self.kind, self.reason) {
            (ActionKind::Takedown, None) => err("takedown needs a reason"),
            (ActionKind::Takedown, Some(Reason::Other)) if self.note.trim().is_empty() => {
                err("reason 9 (other) needs a note")
            }
            (ActionKind::Restore, Some(_)) => err("restore carries no reason"),
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
        let kind = match r.u8()? {
            1 => ActionKind::Takedown,
            2 => ActionKind::Restore,
            _ => return err("unknown team action kind"),
        };
        let community_id: [u8; 32] = r.take(32)?.try_into().expect("32");
        let epoch = u64::from_be_bytes(r.take(8)?.try_into().expect("8"));
        let rc = r.u8()?;
        let reason = match (kind, rc) {
            (ActionKind::Restore, 0) => None,
            (ActionKind::Takedown, c) => {
                Some(Reason::from_code(c).ok_or(OmcpError::Sync("unknown reason code"))?)
            }
            _ => return err("restore carries no reason"),
        };
        let nl = u16::from_be_bytes(r.take(2)?.try_into().expect("2")) as usize;
        if nl > MAX_NOTE {
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
            community_id,
            reason,
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
    /// moderation team" (TM4): every action is ignored.
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
    /// Epoch not newer than the last applied action for this community.
    Replay,
}

/// One transparency log line (§23.9(5), MR9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub kind: ActionKind,
    pub community_id: [u8; 32],
    pub reason_code: u8,
    pub note: String,
    pub epoch: u64,
    /// PeerID hashes of the team keys that signed.
    pub signers: Vec<[u8; 32]>,
    pub applied_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CommunityMod {
    epoch: u64,
    removed: bool,
    reason_code: u8,
    note: String,
}

/// Per-device moderation state (serde; the FFI keeps it in the encrypted store).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModerationState {
    communities: BTreeMap<[u8; 32], CommunityMod>,
    log: Vec<LogEntry>,
    /// Accepted actions as wire bytes, to re-gossip while in force.
    #[serde(default)]
    wire: BTreeMap<[u8; 32], Vec<u8>>,
}

/// What a client shows for a removed community (§23.9(4)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    pub reason: Reason,
    pub note: String,
    pub epoch: u64,
}

impl ModerationState {
    /// Verify and apply one action. On success the log gains a line.
    pub fn apply(
        &mut self,
        policy: &TeamPolicy,
        a: &TeamAction,
        now_ms: u64,
    ) -> std::result::Result<LogEntry, Rejected> {
        let signers = policy.verify(a)?;
        let key = a.community_id;
        if let Some(c) = self.communities.get(&key) {
            if a.epoch <= c.epoch {
                return Err(Rejected::Replay);
            }
        }
        let reason_code = a.reason.map(Reason::code).unwrap_or(0);
        self.communities.insert(
            key,
            CommunityMod {
                epoch: a.epoch,
                removed: a.kind == ActionKind::Takedown,
                reason_code,
                note: a.note.clone(),
            },
        );
        if let Ok(w) = a.encode() {
            self.wire.insert(key, w);
        }
        let e = LogEntry {
            kind: a.kind,
            community_id: a.community_id,
            reason_code,
            note: a.note.clone(),
            epoch: a.epoch,
            signers,
            applied_ms: now_ms,
        };
        self.log.push(e.clone());
        Ok(e)
    }

    /// `Some` if compliant clients must hide the community, refuse joins and
    /// posts, and relays must drop its envelopes.
    pub fn removal(&self, community_id: &[u8; 32]) -> Option<Removal> {
        let c = self.communities.get(community_id)?;
        if !c.removed {
            return None;
        }
        Some(Removal {
            reason: Reason::from_code(c.reason_code)?,
            note: c.note.clone(),
            epoch: c.epoch,
        })
    }

    pub fn is_removed(&self, community_id: &[u8; 32]) -> bool {
        self.removal(community_id).is_some()
    }

    /// Relay rule (§23.9(4)): drop envelopes of removed communities.
    pub fn relay_allows(&self, community_id: Option<&[u8; 32]>) -> bool {
        community_id.map(|c| !self.is_removed(c)).unwrap_or(true)
    }

    /// Transparency log, newest first.
    pub fn log(&self) -> Vec<LogEntry> {
        let mut v = self.log.clone();
        v.reverse();
        v
    }

    /// Latest accepted action per community (for re-gossip).
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
    header_aad.extend_from_slice(&[AAD_TLV_TEAM_MOD, 1, 1]);
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

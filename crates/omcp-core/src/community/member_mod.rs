//! Removing a member from one open community (PROTOCOL §23.10.11; MR42–MR49).
//!
//! A [`CommunityModActionV1`] is signed by the actor's identity key (v1.0,
//! §20.1) and gossiped as a community control frame on the community's tag
//! with `header_aad` TLV `0x5C` `community_mod` (`version 1 ‖ community_ref`).
//!
//! Rules enforced here:
//! - Rank: `owner > admin > moderator > member`; an actor only acts on a
//!   strictly lower rank; nobody removes the owner; ROLE_REVOKE is owner only.
//! - Duration: action 1 lasts 1 h – 365 d ([`DURATION_PRESETS_S`]); no
//!   permanent community removal (only team `kind 9` may use 0).
//! - Abuse guard: moderators and admins may issue at most 10 removals per
//!   rolling hour and 30 per rolling 24 h per community (by signed
//!   `issued_ms`). Over-limit removals are ignored (not applied, not relayed)
//!   but counted; the owner is alerted above 5 in 24 h. The owner and the
//!   team are exempt.
//! - Effect: the removed member can still read; it cannot post, react, join
//!   topics or Live; others drop its new posts; re-join is refused. Nothing
//!   outside the named community changes. A team `kind 9` removal in force
//!   always beats a community action and cannot be lifted by one.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{community_ref, community_tag, Community};
use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::moderation::{h256, peer_id_hash, ActionKind, ModerationState, Reason};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{parse_routing_aad, put_tag_in_aad};

const DOMAIN: &[u8] = b"OMCP-community-mod-v1\x00";
/// `header_aad` TLV `community_mod` (§21.8), len 33.
pub const AAD_TLV_COMMUNITY_MOD: u8 = 0x5C;
pub const MIN_DURATION_S: u32 = 3_600;
pub const MAX_DURATION_S: u32 = 31_536_000;
/// App presets: 1 h, 24 h, 7 d, 30 d, 365 d (§23.10.11(2)).
pub const DURATION_PRESETS_S: [u32; 5] = [3_600, 86_400, 604_800, 2_592_000, 31_536_000];
pub const MAX_NOTE: usize = 512;
pub const LIMIT_PER_HOUR: usize = 10;
pub const LIMIT_PER_DAY: usize = 30;
/// Owner alert when a moderator/admin exceeds this many removals in 24 h.
pub const OWNER_ALERT_ABOVE: usize = 5;
const HOUR_MS: u64 = 3_600_000;
const DAY_MS: u64 = 24 * HOUR_MS;
const MOD_EXPIRY_MS: u64 = 30 * DAY_MS;

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

/// Community role, ordered by rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Role {
    Member = 0,
    Moderator = 1,
    Admin = 2,
    Owner = 3,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Moderator => "community moderator",
            Self::Admin => "community admin",
            Self::Owner => "community owner",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModAction {
    MemberRemove = 1,
    MemberRestore = 2,
    /// Owner only: demote a moderator/admin to member.
    RoleRevoke = 3,
    /// §23.10.13(2): owner handover; note holds the 64-byte accept signature.
    OwnerHandover = 4,
}

impl ModAction {
    pub fn from_code(c: u8) -> Option<Self> {
        Some(match c {
            1 => Self::MemberRemove,
            2 => Self::MemberRestore,
            3 => Self::RoleRevoke,
            4 => Self::OwnerHandover,
            _ => return None,
        })
    }
}

/// `CommunityModActionV1` (§23.10.11(2)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityModActionV1 {
    pub action: ModAction,
    pub community_ref: [u8; 32],
    pub member_pk: [u8; 32],
    pub member_id: [u8; 32],
    pub reason_code: u8,
    pub duration_s: u32,
    pub issued_ms: u64,
    pub epoch: u64,
    pub actor_pk: [u8; 32],
    pub note: Vec<u8>,
    pub sig: [u8; 64],
}

impl CommunityModActionV1 {
    /// Build and sign an action for `community_id`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_signed(
        actor: &SigningKey,
        action: ModAction,
        community_id: &[u8; 32],
        member_pk: [u8; 32],
        reason_code: u8,
        duration_s: u32,
        issued_ms: u64,
        epoch: u64,
        note: &[u8],
    ) -> Self {
        let mut a = Self {
            action,
            community_ref: community_ref(community_id),
            member_pk,
            member_id: peer_id_hash(&member_pk),
            reason_code,
            duration_s,
            issued_ms,
            epoch,
            actor_pk: actor.verifying_key().to_bytes(),
            note: note.to_vec(),
            sig: [0u8; 64],
        };
        a.sig = actor.sign(&a.signed_body()).to_bytes();
        a
    }

    fn fields(&self) -> Vec<u8> {
        let mut b = vec![self.action as u8];
        b.extend_from_slice(&self.community_ref);
        b.extend_from_slice(&self.member_pk);
        b.extend_from_slice(&self.member_id);
        b.push(self.reason_code);
        b.extend_from_slice(&self.duration_s.to_be_bytes());
        b.extend_from_slice(&self.issued_ms.to_be_bytes());
        b.extend_from_slice(&self.epoch.to_be_bytes());
        b.extend_from_slice(&self.actor_pk);
        b.extend_from_slice(&(self.note.len() as u16).to_be_bytes());
        b.extend_from_slice(&self.note);
        b
    }

    pub fn signed_body(&self) -> Vec<u8> {
        let mut b = DOMAIN.to_vec();
        b.extend_from_slice(&self.fields());
        b
    }

    /// Field rules (reason, duration, member_id, note).
    pub fn check_fields(&self) -> Result<()> {
        if self.member_id != peer_id_hash(&self.member_pk) {
            return err("member_id does not match member_pk");
        }
        if self.note.len() > MAX_NOTE {
            return err("note too long");
        }
        match self.action {
            ModAction::MemberRemove => {
                if !matches!(self.reason_code, 1..=7 | 9 | 10) {
                    return err("removal reason code");
                }
                if self.reason_code == 9 && self.note_text().trim().is_empty() {
                    return err("reason 9 needs a note");
                }
                if !(MIN_DURATION_S..=MAX_DURATION_S).contains(&self.duration_s) {
                    return err("community removal lasts 1 h to 365 d");
                }
            }
            ModAction::MemberRestore => {
                if self.reason_code != 8 || self.duration_s != 0 {
                    return err("restore carries reason 8 and no duration");
                }
            }
            ModAction::RoleRevoke => {
                if !matches!(self.reason_code, 0..=7 | 9 | 10) || self.duration_s != 0 {
                    return err("role revoke fields");
                }
            }
            ModAction::OwnerHandover => {
                if self.reason_code != 0 || self.duration_s != 0 || self.note.len() != 64 {
                    return err("owner handover needs 64-byte accept sig in note");
                }
            }
        }
        Ok(())
    }

    /// UTF-8 view of `note` when it is text (not the handover accept sig).
    pub fn note_text(&self) -> &str {
        std::str::from_utf8(&self.note).unwrap_or("")
    }

    pub fn sig_ok(&self) -> bool {
        VerifyingKey::from_bytes(&self.actor_pk)
            .and_then(|vk| vk.verify_strict(&self.signed_body(), &Signature::from_bytes(&self.sig)))
            .is_ok()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.fields();
        b.extend_from_slice(&self.sig);
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        const FIXED: usize = 1 + 32 * 3 + 1 + 4 + 8 + 8 + 32 + 2;
        if b.len() < FIXED + 64 {
            return err("community mod action truncated");
        }
        let a32 = |o: usize| -> [u8; 32] { b[o..o + 32].try_into().expect("32") };
        let action = ModAction::from_code(b[0]).ok_or(OmcpError::Sync("unknown mod action"))?;
        let nl = u16::from_be_bytes([b[FIXED - 2], b[FIXED - 1]]) as usize;
        if nl > MAX_NOTE || b.len() != FIXED + nl + 64 {
            return err("community mod action length");
        }
        let note = b[FIXED..FIXED + nl].to_vec();
        let a = Self {
            action,
            community_ref: a32(1),
            member_pk: a32(33),
            member_id: a32(65),
            reason_code: b[97],
            duration_s: u32::from_be_bytes(b[98..102].try_into().expect("4")),
            issued_ms: u64::from_be_bytes(b[102..110].try_into().expect("8")),
            epoch: u64::from_be_bytes(b[110..118].try_into().expect("8")),
            actor_pk: a32(118),
            note,
            sig: b[FIXED + nl..].try_into().expect("64"),
        };
        a.check_fields()?;
        Ok(a)
    }

    /// End of a removal (`issued_ms + duration_s`).
    pub fn end_ms(&self) -> u64 {
        self.issued_ms + self.duration_s as u64 * 1000
    }
}

/// Why a community action was not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModReject {
    Malformed,
    WrongCommunity,
    BadSignature,
    /// Actor not in the roster with a high enough rank, or target of equal
    /// or higher rank, or the owner.
    NotAllowed,
    /// Epoch not higher than the last applied for this member.
    Replay,
    /// Over the 10/h or 30/24 h limit: ignored and not relayed, but counted.
    OverLimit,
}

/// One line of the community's own log (§23.10.11(6)): no content, no evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityLogEntry {
    pub action: u8,
    pub member_id: [u8; 32],
    /// Target identity public key (needed to restore; serde-default for old logs).
    #[serde(default)]
    pub member_pk: [u8; 32],
    pub reason_code: u8,
    pub duration_s: u32,
    pub issued_ms: u64,
    pub actor_role: Role,
    pub actor_pk: [u8; 32],
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Current {
    epoch: u64,
    removed: bool,
    end_ms: u64,
    reason_code: u8,
    actor_role: Role,
    applied_ms: u64,
    wire: Vec<u8>,
}

/// Who removed a member: shown by role only, never by name (§23.10.11(4)(a)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovedBy {
    Team,
    Community(Role),
}

/// What the removed member's device shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRemoval {
    pub by: RemovedBy,
    pub reason_code: u8,
    /// `None` = permanent (team only).
    pub end_ms: Option<u64>,
    /// Local time the removal was applied (posts dated after it are dropped).
    pub applied_ms: u64,
}

impl MemberRemoval {
    /// "You were removed from <community> by a community moderator".
    pub fn notice(&self, community_name: &str) -> String {
        let by = match self.by {
            RemovedBy::Team => "the Hopla team",
            RemovedBy::Community(r) => match r {
                Role::Owner => "the community owner",
                Role::Admin => "a community admin",
                _ => "a community moderator",
            },
        };
        format!("You were removed from {community_name} by {by}")
    }
}

/// A moderator/admin over the owner-alert threshold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAlert {
    pub actor_pk: [u8; 32],
    /// Removals in the last 24 h, ignored over-limit attempts included.
    pub removals_24h: usize,
}

/// Per-device community removal state (kept with the community store).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberModState {
    current: BTreeMap<([u8; 32], [u8; 32]), Current>,
    /// (community_ref, actor_pk) → signed issued_ms of every removal seen.
    attempts: BTreeMap<([u8; 32], [u8; 32]), Vec<u64>>,
    log: BTreeMap<[u8; 32], Vec<CommunityLogEntry>>,
}

fn count_window(v: &[u64], at: u64, window: u64) -> usize {
    v.iter().filter(|t| **t <= at && at - **t < window).count()
}

/// Time used to judge removal end: the mesh checkpoint when known, otherwise
/// the local clock. For the removed member's own device, the earlier of the
/// two, so a clock set ahead never ends a removal early (§23.10.11(4)(f)).
pub fn effective_now(local_ms: u64, checkpoint_ms: Option<u64>, own_device: bool) -> u64 {
    match checkpoint_ms {
        Some(c) if own_device => c.min(local_ms),
        Some(c) => c,
        None => local_ms,
    }
}

impl MemberModState {
    /// Verify and apply one action against the community roster.
    pub fn apply(
        &mut self,
        com: &Community,
        a: &CommunityModActionV1,
        now_ms: u64,
    ) -> std::result::Result<CommunityLogEntry, ModReject> {
        a.check_fields().map_err(|_| ModReject::Malformed)?;
        let cref = community_ref(&com.id);
        if a.community_ref != cref {
            return Err(ModReject::WrongCommunity);
        }
        if !a.sig_ok() {
            return Err(ModReject::BadSignature);
        }
        let actor = com.role(&a.actor_pk).ok_or(ModReject::NotAllowed)?;
        let target = com.role(&a.member_pk).unwrap_or(Role::Member);
        match a.action {
            ModAction::RoleRevoke => {
                if actor != Role::Owner || !matches!(target, Role::Admin | Role::Moderator) {
                    return Err(ModReject::NotAllowed);
                }
            }
            ModAction::OwnerHandover => {
                if actor != Role::Owner || a.actor_pk != com.founder {
                    return Err(ModReject::NotAllowed);
                }
                if com.role(&a.member_pk).is_none() || a.member_pk == com.founder {
                    return Err(ModReject::NotAllowed);
                }
                let Ok(sig) = <[u8; 64]>::try_from(a.note.as_slice()) else {
                    return Err(ModReject::Malformed);
                };
                if !verify_owner_accept(&a.member_pk, &a.community_ref, a.epoch, &sig) {
                    return Err(ModReject::BadSignature);
                }
            }
            _ => {
                if actor < Role::Moderator || target >= actor {
                    return Err(ModReject::NotAllowed);
                }
            }
        }
        let key = (cref, a.member_id);
        if a.action != ModAction::RoleRevoke && a.action != ModAction::OwnerHandover {
            if let Some(c) = self.current.get(&key) {
                if a.epoch <= c.epoch {
                    return Err(ModReject::Replay);
                }
            }
        }
        if a.action == ModAction::MemberRemove && actor != Role::Owner {
            let v = self.attempts.entry((cref, a.actor_pk)).or_default();
            let over = count_window(v, a.issued_ms, HOUR_MS) >= LIMIT_PER_HOUR
                || count_window(v, a.issued_ms, DAY_MS) >= LIMIT_PER_DAY;
            v.push(a.issued_ms);
            let newest = v.iter().copied().max().unwrap_or(0);
            v.retain(|t| newest - *t < DAY_MS);
            if over {
                return Err(ModReject::OverLimit);
            }
        }
        if a.action != ModAction::RoleRevoke && a.action != ModAction::OwnerHandover {
            self.current.insert(
                key,
                Current {
                    epoch: a.epoch,
                    removed: a.action == ModAction::MemberRemove,
                    end_ms: a.end_ms(),
                    reason_code: a.reason_code,
                    actor_role: actor,
                    applied_ms: now_ms,
                    wire: a.encode(),
                },
            );
        }
        let e = CommunityLogEntry {
            action: a.action as u8,
            member_id: a.member_id,
            member_pk: a.member_pk,
            reason_code: a.reason_code,
            duration_s: a.duration_s,
            issued_ms: a.issued_ms,
            actor_role: actor,
            actor_pk: a.actor_pk,
            epoch: a.epoch,
        };
        self.log.entry(cref).or_default().push(e.clone());
        Ok(e)
    }

    /// Removal of `member_id` in `community_id` in force at `now_ms`
    /// (use [`effective_now`]). A team `kind 9` in force always wins; a team
    /// ITEM_RESTORE with a higher epoch also lifts a community removal.
    pub fn removal(
        &self,
        team: &ModerationState,
        community_id: &[u8; 32],
        member_id: &[u8; 32],
        now_ms: u64,
    ) -> Option<MemberRemoval> {
        if let Some((r, end)) = team.team_member_removal(community_id, member_id, now_ms) {
            return Some(MemberRemoval {
                by: RemovedBy::Team,
                reason_code: r.reason.code(),
                end_ms: end,
                applied_ms: team.item_applied_ms(community_id, member_id).unwrap_or(0),
            });
        }
        let c = self
            .current
            .get(&(community_ref(community_id), *member_id))?;
        if !c.removed || now_ms >= c.end_ms {
            return None;
        }
        if let Some((ActionKind::ItemRestore, e)) = team.item_kind_epoch(community_id, member_id) {
            if e > c.epoch {
                return None;
            }
        }
        Some(MemberRemoval {
            by: RemovedBy::Community(c.actor_role),
            reason_code: c.reason_code,
            end_ms: Some(c.end_ms),
            applied_ms: c.applied_ms,
        })
    }

    /// Community log, oldest first (open to members).
    pub fn log(&self, community_id: &[u8; 32]) -> Vec<CommunityLogEntry> {
        self.log
            .get(&community_ref(community_id))
            .cloned()
            .unwrap_or_default()
    }

    /// Owner's "Removals" view: every removal by a moderator or admin.
    pub fn removals_view(&self, community_id: &[u8; 32]) -> Vec<CommunityLogEntry> {
        self.log(community_id)
            .into_iter()
            .filter(|e| e.action == ModAction::MemberRemove as u8 && e.actor_role != Role::Owner)
            .collect()
    }

    /// MUST alert the owner when any moderator/admin exceeds 5 removals in
    /// 24 h (ignored over-limit attempts count).
    pub fn owner_alerts(&self, community_id: &[u8; 32], now_ms: u64) -> Vec<OwnerAlert> {
        let cref = community_ref(community_id);
        self.attempts
            .iter()
            .filter(|((c, _), _)| *c == cref)
            .map(|((_, a), v)| OwnerAlert {
                actor_pk: *a,
                removals_24h: count_window(v, now_ms, DAY_MS),
            })
            .filter(|x| x.removals_24h > OWNER_ALERT_ABOVE)
            .collect()
    }

    /// `(left this hour, left today)` for a moderator/admin (`comm-mod-limit`).
    pub fn quota_left(
        &self,
        community_id: &[u8; 32],
        actor_pk: &[u8; 32],
        now_ms: u64,
    ) -> (usize, usize) {
        let v = self
            .attempts
            .get(&(community_ref(community_id), *actor_pk))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        (
            LIMIT_PER_HOUR.saturating_sub(count_window(v, now_ms, HOUR_MS)),
            LIMIT_PER_DAY.saturating_sub(count_window(v, now_ms, DAY_MS)),
        )
    }

    /// Accepted actions still in force, for re-gossip.
    pub fn current_actions(&self, now_ms: u64) -> Vec<Vec<u8>> {
        self.current
            .values()
            .filter(|c| !c.removed || now_ms < c.end_ms)
            .map(|c| c.wire.clone())
            .collect()
    }

    /// Drop everything about one community (when it is left or taken down).
    pub fn forget(&mut self, community_id: &[u8; 32]) {
        let cref = community_ref(community_id);
        self.current.retain(|(c, _), _| *c != cref);
        self.attempts.retain(|(c, _), _| *c != cref);
        self.log.remove(&cref);
    }
}

fn envelope_id(body: &[u8]) -> [u8; 16] {
    h256(&[b"OMCP-community-mod-env-v1\x00", body])[..16]
        .try_into()
        .expect("16")
}

/// Wrap an action in a public envelope on the community tag with TLV `0x5C`.
/// Outer expiry 30 d or the end of the removal if sooner.
pub fn seal_mod_action(
    a: &CommunityModActionV1,
    community_id: &[u8; 32],
    now_ms: u64,
    ttl_hops: u8,
) -> EnvelopeV1 {
    let body = a.encode();
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &community_tag(community_id));
    let mut exp = now_ms.saturating_add(MOD_EXPIRY_MS);
    if a.action == ModAction::MemberRemove {
        exp = exp.min(a.end_ms());
    }
    put_expires_in_aad(&mut header_aad, exp);
    header_aad.extend_from_slice(&[AAD_TLV_COMMUNITY_MOD, 33, 1]);
    header_aad.extend_from_slice(&a.community_ref);
    EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: envelope_id(&body),
        flags: flags::RELAY_OK | flags::STORE_OK | flags::PRIORITY,
        ttl_hops,
        created_ms: now_ms,
        header_aad,
        ciphertext: body,
    }
}

/// The `0x5C` TLV's `community_ref`, if present with version 1.
pub fn mod_tlv(aad: &[u8]) -> Option<[u8; 32]> {
    let mut i = 0;
    while i + 2 <= aad.len() {
        let (t, l) = (aad[i], aad[i + 1] as usize);
        if t == AAD_TLV_COMMUNITY_MOD && l == 33 && i + 2 + l <= aad.len() && aad[i + 2] == 1 {
            return aad[i + 3..i + 35].try_into().ok();
        }
        i += 2 + l;
    }
    None
}

/// Parse a community moderation envelope; the TLV MUST match the body (MR49).
pub fn open_mod_action(env: &EnvelopeV1, community_id: &[u8; 32]) -> Result<CommunityModActionV1> {
    if parse_routing_aad(&env.header_aad)?.tag != Some(community_tag(community_id)) {
        return err("not this community's tag");
    }
    if env.envelope_id != envelope_id(&env.ciphertext) {
        return err("community mod envelope id");
    }
    let a = CommunityModActionV1::decode(&env.ciphertext)?;
    if mod_tlv(&env.header_aad) != Some(a.community_ref)
        || a.community_ref != community_ref(community_id)
    {
        return err("community_mod TLV");
    }
    Ok(a)
}

/// §23.10.4(3) reason label for notices (reason 10 shows the help link).
pub fn reason_label(code: u8) -> &'static str {
    match Reason::from_code(code) {
        Some(Reason::Hate) => "Hate",
        Some(Reason::Illegal) => "Illegal content",
        Some(Reason::Terror) => "Terrorism",
        Some(Reason::Harassment) => "Harassment",
        Some(Reason::Spam) => "Spam",
        Some(Reason::Csam) => "Child sexual abuse material",
        Some(Reason::Threats) => "Threats",
        Some(Reason::RestoreAfterReview) => "Restored after review",
        Some(Reason::Other) => "Other",
        Some(Reason::SelfHarm) => "Self-harm or suicide",
        None => "",
    }
}


const OWNER_ACCEPT_DOMAIN: &[u8] = b"OMCP-owner-accept-v1\x00";

/// Acceptance signature over community_ref || member_pk || owner_epoch (§23.10.13(2)(c)).
pub fn owner_accept_sig(
    new_owner: &SigningKey,
    community_ref: &[u8; 32],
    member_pk: &[u8; 32],
    owner_epoch: u64,
) -> [u8; 64] {
    let mut b = OWNER_ACCEPT_DOMAIN.to_vec();
    b.extend_from_slice(community_ref);
    b.extend_from_slice(member_pk);
    b.extend_from_slice(&owner_epoch.to_be_bytes());
    new_owner.sign(&b).to_bytes()
}

pub fn verify_owner_accept(
    member_pk: &[u8; 32],
    community_ref: &[u8; 32],
    owner_epoch: u64,
    sig: &[u8; 64],
) -> bool {
    let Ok(vk) = VerifyingKey::from_bytes(member_pk) else {
        return false;
    };
    let mut b = OWNER_ACCEPT_DOMAIN.to_vec();
    b.extend_from_slice(community_ref);
    b.extend_from_slice(member_pk);
    b.extend_from_slice(&owner_epoch.to_be_bytes());
    vk.verify_strict(&b, &Signature::from_bytes(sig)).is_ok()
}

/// Build a fully owner-signed `OWNER_HANDOVER` whose `note` is the candidate's
/// accept signature. The candidate must produce `accept_sig` before the owner
/// finalizes the offer, so the owner can sign the complete action offline for
/// the candidate to publish later.
pub fn build_signed_owner_handover(
    owner: &SigningKey,
    community_id: &[u8; 32],
    new_owner_pk: [u8; 32],
    owner_epoch: u64,
    issued_ms: u64,
    accept_sig: &[u8; 64],
) -> CommunityModActionV1 {
    CommunityModActionV1::new_signed(
        owner,
        ModAction::OwnerHandover,
        community_id,
        new_owner_pk,
        0,
        0,
        issued_ms,
        owner_epoch,
        accept_sig,
    )
}

/// Preliminary 1:1 request (no secrets). Candidate answers with an accept sig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerHandoverRequest {
    pub community_ref: [u8; 32],
    pub community_id: [u8; 32],
    pub owner_pk: [u8; 32],
    pub owner_epoch: u64,
    pub expires_ms: u64,
}

impl OwnerHandoverRequest {
    pub const TYPE: u8 = 0x02;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![Self::TYPE];
        b.extend_from_slice(&self.community_ref);
        b.extend_from_slice(&self.community_id);
        b.extend_from_slice(&self.owner_pk);
        b.extend_from_slice(&self.owner_epoch.to_be_bytes());
        b.extend_from_slice(&self.expires_ms.to_be_bytes());
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != 1 + 32 * 3 + 8 + 8 || b[0] != Self::TYPE {
            return err("owner handover request");
        }
        let a32 = |o: usize| -> [u8; 32] { b[o..o + 32].try_into().expect("32") };
        Ok(Self {
            community_ref: a32(1),
            community_id: a32(33),
            owner_pk: a32(65),
            owner_epoch: u64::from_be_bytes(b[97..105].try_into().expect("8")),
            expires_ms: u64::from_be_bytes(b[105..113].try_into().expect("8")),
        })
    }
}

/// Full 1:1 offer: community_sk + owner-signed OWNER_HANDOVER (§23.10.13(2)(b)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerHandoverOffer {
    pub community_ref: [u8; 32],
    pub community_sk: [u8; 32],
    pub owner_epoch: u64,
    pub expires_ms: u64,
    /// Pre-signed OWNER_HANDOVER action from the current owner (note = accept sig).
    pub action_bytes: Vec<u8>,
}

impl OwnerHandoverOffer {
    pub const TYPE: u8 = 0x01;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![Self::TYPE];
        b.extend_from_slice(&self.community_ref);
        b.extend_from_slice(&self.community_sk);
        b.extend_from_slice(&self.owner_epoch.to_be_bytes());
        b.extend_from_slice(&self.expires_ms.to_be_bytes());
        b.extend_from_slice(&(self.action_bytes.len() as u16).to_be_bytes());
        b.extend_from_slice(&self.action_bytes);
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < 1 + 32 + 32 + 8 + 8 + 2 {
            return err("owner handover offer truncated");
        }
        if b[0] != Self::TYPE {
            return err("owner handover offer type");
        }
        let a32 = |o: usize| -> [u8; 32] { b[o..o + 32].try_into().expect("32") };
        let al = u16::from_be_bytes([b[81], b[82]]) as usize;
        if b.len() != 83 + al {
            return err("owner handover offer length");
        }
        Ok(Self {
            community_ref: a32(1),
            community_sk: a32(33),
            owner_epoch: u64::from_be_bytes(b[65..73].try_into().expect("8")),
            expires_ms: u64::from_be_bytes(b[73..81].try_into().expect("8")),
            action_bytes: b[83..].to_vec(),
        })
    }

    /// Verify the owner signature, accept signature, and field binding.
    pub fn verify_chain(&self, now_ms: u64) -> Result<CommunityModActionV1> {
        if self.expires_ms <= now_ms {
            return err("owner handover offer expired");
        }
        let a = CommunityModActionV1::decode(&self.action_bytes)?;
        if a.action != ModAction::OwnerHandover
            || a.community_ref != self.community_ref
            || a.epoch != self.owner_epoch
            || a.note.len() != 64
        {
            return err("owner handover offer action mismatch");
        }
        if !a.sig_ok() {
            return err("owner handover owner signature");
        }
        let Ok(sig) = <[u8; 64]>::try_from(a.note.as_slice()) else {
            return err("owner handover accept note");
        };
        if !verify_owner_accept(&a.member_pk, &a.community_ref, a.epoch, &sig) {
            return err("owner handover accept signature");
        }
        Ok(a)
    }
}

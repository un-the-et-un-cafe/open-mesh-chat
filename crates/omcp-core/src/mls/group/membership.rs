//! Leaving and removal in private groups (PROTOCOL §23.10.12; MR50–MR52).
//!
//! - **Leave**: the leaver sends a Remove-self proposal and never commits it;
//!   its device marks the group *left* and erases the MLS state.
//! - **Bounded commit**: a remaining member commits a pending leave. The
//!   lowest-leaf online member commits at once; others after a random
//!   10–60 min; every member within 24 h; nobody sends application messages
//!   in the old epoch after a leave has been pending for 24 h.
//! - **Key-rotation log**: per-group `{epoch, cause, affected, committer}`
//!   kept inside the (encrypted) MLS member state, never sent outside the
//!   group, deleted with the group.
//! - **RemovalNotice**: sent as an MLS application message in the last epoch
//!   the removed member held (on the old-epoch tag,
//!   [`crate::sync::tag::EpochTagHistory::removal_notice_tag`]), before the
//!   commit is applied. It names the remover's *role*, never the remover.
//!
//! None of this applies to 1:1 chats and no team key is ever involved.

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::*;
use crate::moderation::{group_ref as mod_group_ref, peer_id_hash_bytes};

/// Control payload type `0x01` RemovalNotice (§23.10.12(2)(b)).
pub const REMOVAL_NOTICE_TYPE: u8 = 0x01;
const NOTICE_DOMAIN: &[u8] = b"OMCP-removal-notice-v1\x00";
pub const MAX_REASON_TEXT: usize = 280;
/// A pending leave MUST be committed within this long (§23.10.12(1)(c)).
pub const LEAVE_COMMIT_DEADLINE_MS: u64 = 24 * 60 * 60 * 1000;
pub const LEAVE_JITTER_MIN_MS: u64 = 10 * 60 * 1000;
pub const LEAVE_JITTER_MAX_MS: u64 = 60 * 60 * 1000;

/// Shown on the removed member's device.
pub const REMOVED_NOTICE_TEXT: &str = "You were removed from this group";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroupStatus {
    #[default]
    Active,
    /// This member left (read-only history, MLS state erased).
    Left,
    /// This member was removed (read-only history, MLS state erased).
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationCause {
    Join,
    Leave,
    Remove,
    Update,
    /// §23.10.13: admin list change.
    Admin,
}

/// One key-rotation log line (§23.10.12(1)(d)).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationEntry {
    /// The new epoch after the commit.
    pub epoch: u64,
    pub cause: RotationCause,
    /// member_id (BLAKE2b-256 of the credential identity) of the joined,
    /// left or removed member; `None` for an Update.
    pub affected: Option<[u8; 32]>,
    pub committer: Option<[u8; 32]>,
}

/// Remover's role in a RemovalNotice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoverRole {
    GroupAdmin = 1,
    Member = 2,
}

impl RemoverRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::GroupAdmin => "a group admin",
            Self::Member => "a group member",
        }
    }
}

/// `RemovalNotice { type 0x01, group_ref, removed_at_epoch, remover_role,
/// reason_text_len u16, reason_text, sig }` signed by the remover's leaf key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalNotice {
    pub group_ref: [u8; 32],
    pub removed_at_epoch: u64,
    pub remover_role: RemoverRole,
    pub reason_text: String,
    pub sig: [u8; 64],
}

impl RemovalNotice {
    fn fields(&self) -> Vec<u8> {
        let mut b = vec![REMOVAL_NOTICE_TYPE];
        b.extend_from_slice(&self.group_ref);
        b.extend_from_slice(&self.removed_at_epoch.to_be_bytes());
        b.push(self.remover_role as u8);
        b.extend_from_slice(&(self.reason_text.len() as u16).to_be_bytes());
        b.extend_from_slice(self.reason_text.as_bytes());
        b
    }
    fn signed_body(&self) -> Vec<u8> {
        let mut b = NOTICE_DOMAIN.to_vec();
        b.extend_from_slice(&self.fields());
        b
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.fields();
        b.extend_from_slice(&self.sig);
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        const FIXED: usize = 1 + 32 + 8 + 1 + 2;
        if b.len() < FIXED + 64 || b[0] != REMOVAL_NOTICE_TYPE {
            return Err(OmcpError::Mls("removal notice"));
        }
        let n = u16::from_be_bytes([b[42], b[43]]) as usize;
        if n > MAX_REASON_TEXT || b.len() != FIXED + n + 64 {
            return Err(OmcpError::Mls("removal notice length"));
        }
        let remover_role = match b[41] {
            1 => RemoverRole::GroupAdmin,
            2 => RemoverRole::Member,
            _ => return Err(OmcpError::Mls("removal notice role")),
        };
        Ok(Self {
            group_ref: b[1..33].try_into().expect("32"),
            removed_at_epoch: u64::from_be_bytes(b[33..41].try_into().expect("8")),
            remover_role,
            reason_text: std::str::from_utf8(&b[FIXED..FIXED + n])
                .map_err(|_| OmcpError::Mls("removal notice utf-8"))?
                .to_string(),
            sig: b[FIXED + n..].try_into().expect("64"),
        })
    }
    pub fn verify(&self, leaf_signature_key: &[u8]) -> bool {
        let Ok(pk) = <[u8; 32]>::try_from(leaf_signature_key) else {
            return false;
        };
        VerifyingKey::from_bytes(&pk)
            .and_then(|vk| vk.verify_strict(&self.signed_body(), &Signature::from_bytes(&self.sig)))
            .is_ok()
    }
    /// What the removed member's device shows: role only, never the name.
    pub fn display(&self) -> String {
        format!("{REMOVED_NOTICE_TEXT} by {}.", self.remover_role.label())
    }
}

/// Is `b` a private-group control payload (RemovalNotice)?
pub fn is_removal_notice(b: &[u8]) -> bool {
    RemovalNotice::decode(b).is_ok()
}

impl OmcpMlsMember {
    pub(super) fn leaf_member_id(&self, leaf: u32) -> Option<[u8; 32]> {
        let g = self.group.as_ref()?;
        g.member(LeafNodeIndex::new(leaf))
            .map(|c| peer_id_hash_bytes(c.serialized_content()))
    }

    pub(super) fn own_member_id(&self) -> [u8; 32] {
        peer_id_hash_bytes(&self.identity)
    }

    /// Log lines for the proposals a commit carries (computed before merge).
    pub(super) fn entries_for(
        &self,
        proposals: &[(Proposal, Sender)],
        committer: Option<[u8; 32]>,
        new_epoch: u64,
    ) -> Vec<RotationEntry> {
        let mut v = Vec::new();
        for (p, s) in proposals {
            let (cause, affected) = match p {
                Proposal::Add(a) => (
                    RotationCause::Join,
                    Some(peer_id_hash_bytes(
                        a.key_package()
                            .leaf_node()
                            .credential()
                            .serialized_content(),
                    )),
                ),
                Proposal::Remove(r) => {
                    let leave = matches!(s, Sender::Member(l) if *l == r.removed());
                    (
                        if leave {
                            RotationCause::Leave
                        } else {
                            RotationCause::Remove
                        },
                        self.leaf_member_id(r.removed().u32()),
                    )
                }
                Proposal::SelfRemove => (
                    RotationCause::Leave,
                    match s {
                        Sender::Member(l) => self.leaf_member_id(l.u32()),
                        _ => None,
                    },
                ),
                Proposal::GroupContextExtensions(_) => {
                    // §23.10.13(f): admin-list changes land as GCE.
                    (RotationCause::Admin, None)
                }
                _ => continue,
            };
            v.push(RotationEntry {
                epoch: new_epoch,
                cause,
                affected,
                committer,
            });
        }
        if v.is_empty() {
            v.push(RotationEntry {
                epoch: new_epoch,
                cause: RotationCause::Update,
                affected: None,
                committer,
            });
        }
        v
    }

    /// Mark this member state as a 1:1 chat (MR52): leave, removal and
    /// removal notices are then refused, sent or received.
    pub fn set_one_to_one(&mut self) {
        self.one_to_one = true;
    }

    pub fn is_one_to_one(&self) -> bool {
        self.one_to_one
    }

    pub(super) fn not_one_to_one(&self) -> Result<()> {
        if self.one_to_one {
            return Err(OmcpError::Mls("no leave or removal in a 1:1 chat"));
        }
        Ok(())
    }

    /// The group's key-rotation log, oldest first.
    pub fn rotation_log(&self) -> &[RotationEntry] {
        &self.rotation_log
    }

    pub fn status(&self) -> GroupStatus {
        self.status
    }

    /// Leave (§23.10.12(1)(a)–(b)): returns the Remove-self proposal to send;
    /// this device never commits it, marks the group left and erases the
    /// MLS state, keys and tags.
    pub fn leave(&mut self) -> Result<Vec<u8>> {
        self.not_one_to_one()?;
        let p = self.propose_leave()?;
        self.erase_group();
        self.status = GroupStatus::Left;
        Ok(p)
    }

    /// When a pending leave was first received while online.
    pub fn pending_leave_since(&self) -> Option<u64> {
        self.pending_leave_since
    }

    /// Should this member commit the pending leave now? `online_leaves` are
    /// the leaf indices of members known online (from the digest exchange);
    /// `jitter_ms` is a random 10–60 min chosen once per leave. The lowest
    /// online leaf commits at once; if nobody else is online this member is
    /// the lowest and commits after its first digest exchange.
    pub fn leave_commit_due(&self, now_ms: u64, online_leaves: &[u32], jitter_ms: u64) -> bool {
        let Some(since) = self.pending_leave_since else {
            return false;
        };
        let Ok(me) = self.own_leaf_index() else {
            return false;
        };
        let leavers = self.pending_leavers();
        let lowest = online_leaves
            .iter()
            .chain(std::iter::once(&me))
            .filter(|l| !leavers.contains(l))
            .min()
            .copied();
        if lowest == Some(me) {
            return true;
        }
        let wait = jitter_ms.clamp(LEAVE_JITTER_MIN_MS, LEAVE_JITTER_MAX_MS);
        now_ms.saturating_sub(since) >= wait.min(LEAVE_COMMIT_DEADLINE_MS)
    }

    pub(super) fn pending_leavers(&self) -> Vec<u32> {
        let Some(g) = self.group.as_ref() else {
            return Vec::new();
        };
        g.pending_proposals()
            .filter_map(|q| match (q.proposal(), q.sender()) {
                (Proposal::SelfRemove, Sender::Member(l)) => Some(l.u32()),
                (Proposal::Remove(r), Sender::Member(l)) if *l == r.removed() => Some(l.u32()),
                _ => None,
            })
            .collect()
    }

    /// MUST NOT send application messages in the old epoch once a leave has
    /// been pending for 24 h: commit first.
    pub fn may_send_app_message(&self, now_ms: u64) -> bool {
        self.pending_leave_since
            .map(|s| now_ms.saturating_sub(s) < LEAVE_COMMIT_DEADLINE_MS)
            .unwrap_or(true)
    }

    /// Commit the pending leave(s) (any remaining member may). Returns the commit.
    pub fn commit_pending_leave(&mut self) -> Result<Vec<u8>> {
        self.not_one_to_one()?;
        if self.pending_leave_since.is_none() {
            return Err(OmcpError::Mls("no pending leave"));
        }
        self.self_update()
    }

    /// Remove a member and build the RemovalNotice in the epoch the removed
    /// member still holds. Returns `(notice_message, commit)`: send the
    /// notice first, on the old-epoch tag, in the same batch as the commit.
    pub fn remove_member_with_notice(
        &mut self,
        leaf: u32,
        reason_text: &str,
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        self.not_one_to_one()?;
        if reason_text.len() > MAX_REASON_TEXT {
            return Err(OmcpError::Mls("reason text over 280 bytes"));
        }
        let group_ref = self.moderation_group_ref()?;
        let epoch = self.epoch()?;
        let remover_role = if self.own_leaf_index()? == 0 {
            RemoverRole::GroupAdmin
        } else {
            RemoverRole::Member
        };
        let mut n = RemovalNotice {
            group_ref,
            removed_at_epoch: epoch,
            remover_role,
            reason_text: reason_text.to_string(),
            sig: [0u8; 64],
        };
        let body = n.signed_body();
        let sig = openmls_traits::signatures::Signer::sign(&self.signer, &body)
            .map_err(|_| OmcpError::Mls("leaf key sign"))?;
        n.sig = sig
            .try_into()
            .map_err(|_| OmcpError::Mls("leaf key sign"))?;
        let notice = self.encrypt_app_message(&n.encode())?;
        let commit = self.remove_members(&[leaf])?;
        Ok((notice, commit))
    }

    /// Removed member: open a RemovalNotice message. On success the group is
    /// marked removed and its MLS state erased (§23.10.12(2)(c)).
    pub fn accept_removal_notice(&mut self, msg: &[u8]) -> Result<RemovalNotice> {
        self.not_one_to_one()?;
        let (plain, _, leaf, _) = self.decrypt_app_message_full(msg)?;
        let n = self.verify_removal_notice(&plain, leaf)?;
        self.mark_removed();
        Ok(n)
    }

    /// Check an already-decrypted notice from `leaf` (group_ref and leaf
    /// signature). Does not change state.
    pub fn verify_removal_notice(&self, plain: &[u8], leaf: u32) -> Result<RemovalNotice> {
        self.not_one_to_one()?;
        let n = RemovalNotice::decode(plain)?;
        let g = self.group.as_ref().ok_or(OmcpError::Mls("no group"))?;
        let sk = g
            .members()
            .find(|m| m.index.u32() == leaf)
            .map(|m| m.signature_key.clone())
            .ok_or(OmcpError::Mls("notice sender"))?;
        if n.group_ref != mod_group_ref(g.group_id().as_slice()) || !n.verify(&sk) {
            return Err(OmcpError::Mls("removal notice signature"));
        }
        Ok(n)
    }

    /// Mark the group removed and erase its MLS state (§23.10.12(2)(c)).
    pub fn mark_removed(&mut self) {
        if self.one_to_one {
            return;
        }
        self.erase_group();
        self.status = GroupStatus::Removed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;
    const MIN: u64 = 60_000;

    fn group3() -> (OmcpMlsMember, OmcpMlsMember, OmcpMlsMember) {
        let mut a = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
        let mut b = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
        let mut c = OmcpMlsMember::generate(b"carol".to_vec()).unwrap();
        a.create_group().unwrap();
        let (_c1, w) = a.add_members(&[b.take_key_package().unwrap()]).unwrap();
        b.join_from_welcome(&w).unwrap();
        let (c2, w2) = a.add_members(&[c.take_key_package().unwrap()]).unwrap();
        b.process_protocol_message_at(&c2, T0).unwrap();
        c.join_from_welcome(&w2).unwrap();
        (a, b, c)
    }

    fn id(n: &[u8]) -> [u8; 32] {
        peer_id_hash_bytes(n)
    }

    #[test]
    fn mr50_leave_proposal_bounded_commit_and_rotation_log() {
        let (mut a, mut b, mut c) = group3();
        let snapshot = c.export_state().unwrap();
        let prop = c.leave().unwrap();
        assert_eq!(c.status(), GroupStatus::Left);
        assert!(!c.has_group());
        a.process_protocol_message_at(&prop, T0).unwrap();
        b.process_protocol_message_at(&prop, T0).unwrap();
        assert_eq!(b.pending_leave_since(), Some(T0));
        // Lowest online leaf (alice, 0) commits at once; bob waits his jitter.
        assert!(a.leave_commit_due(T0, &[0, 1], 0));
        assert!(!b.leave_commit_due(T0, &[0, 1], 10 * MIN));
        assert!(b.leave_commit_due(T0 + 10 * MIN, &[0, 1], 10 * MIN));
        // Nobody else online: bob is the lowest online member and commits.
        assert!(b.leave_commit_due(T0, &[1], 30 * MIN));
        assert!(b.may_send_app_message(T0 + 23 * 60 * MIN));
        assert!(!b.may_send_app_message(T0 + 24 * 60 * MIN));
        // A non-admin remaining member commits it (auth from 0.1.30).
        let commit = b.commit_pending_leave().unwrap();
        a.process_protocol_message_at(&commit, T0 + MIN).unwrap();
        assert_eq!(a.member_count().unwrap(), 2);
        assert!(a.pending_leave_since().is_none());
        // The leaver can't decrypt anything new, even from a state snapshot.
        let mut old = OmcpMlsMember::restore(&snapshot).unwrap();
        let ct = a.encrypt_app_message(b"after").unwrap();
        assert!(old.decrypt_app_message(&ct).is_err());
        assert_eq!(b.decrypt_app_message(&ct).unwrap(), b"after");
        for m in [&a, &b] {
            let e = m.rotation_log().last().unwrap();
            assert_eq!(e.cause, RotationCause::Leave);
            assert_eq!(e.affected, Some(id(b"carol")));
            assert_eq!(e.committer, Some(id(b"bob")));
        }
        assert!(a
            .rotation_log()
            .iter()
            .any(|e| e.cause == RotationCause::Join));
        // Kept across a restart (inside the encrypted member state).
        let r = OmcpMlsMember::restore(&a.export_state().unwrap()).unwrap();
        assert_eq!(r.rotation_log(), a.rotation_log());
    }

    #[test]
    fn mr51_removal_notice_in_old_epoch_role_only_and_erasure() {
        let (mut a, mut b, mut c) = group3();
        let gid = a.group_id().unwrap();
        let c_leaf = c.own_leaf_index().unwrap();
        let (notice, commit) = a.remove_member_with_notice(c_leaf, "spam").unwrap();
        b.process_protocol_message_at(&commit, T0).unwrap();
        let n = c.accept_removal_notice(&notice).unwrap();
        assert_eq!(n.remover_role, RemoverRole::GroupAdmin);
        assert_eq!(n.reason_text, "spam");
        let shown = n.display();
        assert!(shown.contains("group admin") && !shown.contains("alice"));
        assert!(!n.encode().windows(5).any(|w| w == b"alice"));
        assert_eq!(c.status(), GroupStatus::Removed);
        assert!(!c.has_group());
        // Forensic check: no MLS storage entry for the group remains.
        let st = c.export_state().unwrap();
        assert!(st.group_id.is_none());
        assert!(!st
            .storage
            .iter()
            .any(|(k, v)| k.windows(gid.len()).any(|w| w == gid.as_slice())
                || v.windows(gid.len()).any(|w| w == gid.as_slice())));
        assert!(b
            .rotation_log()
            .iter()
            .any(|e| e.cause == RotationCause::Remove && e.affected == Some(id(b"carol"))));
        // Tampered notice is rejected.
        let mut bad = RemovalNotice::decode(&n.encode()).unwrap();
        bad.remover_role = RemoverRole::Member;
        assert!(!bad.verify(&[0u8; 32]));
    }

    #[test]
    fn mr51_removed_without_notice_detects_own_removal_in_commit() {
        let (mut a, mut b, mut c) = group3();
        let commit = a.remove_members(&[b.own_leaf_index().unwrap()]).unwrap();
        c.process_protocol_message_at(&commit, T0).unwrap();
        b.process_protocol_message_at(&commit, T0).unwrap();
        assert_eq!(b.status(), GroupStatus::Removed);
        assert!(!b.has_group());
        assert_eq!(c.status(), GroupStatus::Active);
    }

    fn pair() -> (OmcpMlsMember, OmcpMlsMember) {
        let mut a = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
        let mut b = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
        a.create_group().unwrap();
        let (_c, w) = a.add_members(&[b.take_key_package().unwrap()]).unwrap();
        b.join_from_welcome(&w).unwrap();
        (a, b)
    }

    #[test]
    fn mr52_one_to_one_refuses_leave_removal_and_notices() {
        // A notice made while it was still a group, delivered to a 1:1 state.
        let (mut a, mut b) = pair();
        let (notice, _) = a.remove_member_with_notice(1, "x").unwrap();
        let (mut a, mut b2) = pair();
        a.set_one_to_one();
        b2.set_one_to_one();
        assert!(a.leave().is_err() && a.has_group());
        assert!(a.propose_leave().is_err());
        assert!(a.remove_members(&[1]).is_err());
        assert!(a.remove_member_with_notice(1, "").is_err());
        assert!(a.commit_pending_leave().is_err());
        b.set_one_to_one();
        assert!(b.accept_removal_notice(&notice).is_err());
        assert_eq!(b.status(), GroupStatus::Active);
        b2.mark_removed();
        assert!(b2.has_group());
        // A peer that isn't flagged still can't push a leave or removal into
        // a flagged 1:1: the proposal and the commit are refused.
        let (mut x, mut y) = pair();
        y.set_one_to_one();
        let prop = x.propose_leave().unwrap();
        assert!(y.process_protocol_message_at(&prop, T0).is_err());
        assert!(y.has_group());
        let (mut x, mut y) = pair();
        y.set_one_to_one();
        let rm = x.remove_members(&[1]).unwrap();
        assert!(y.process_protocol_message_at(&rm, T0).is_err());
        assert!(y.has_group());
        // The flag survives a restart.
        let r = OmcpMlsMember::restore(&y.export_state().unwrap()).unwrap();
        assert!(r.is_one_to_one());
    }

    #[test]
    fn mr52_notice_has_no_team_role_and_rejects_foreign_signers() {
        let (mut a, _b, mut c) = group3();
        let (notice, _commit) = a
            .remove_member_with_notice(c.own_leaf_index().unwrap(), "")
            .unwrap();
        // A notice whose signature is not by a group leaf (e.g. a team key)
        // is never accepted: the signer must be a group member.
        let n = c.accept_removal_notice(&notice).unwrap();
        let team = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        assert!(!n.verify(team.verifying_key().as_bytes()));
        assert!(matches!(
            n.remover_role,
            RemoverRole::GroupAdmin | RemoverRole::Member
        ));
        assert!(n.reason_text.len() <= MAX_REASON_TEXT);
    }
}

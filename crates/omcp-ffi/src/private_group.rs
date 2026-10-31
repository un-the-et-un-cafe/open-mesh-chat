//! Private groups (PROTOCOL §20.1, §23.10; REQUIREMENTS TM4–TM9): end-to-end
//! encrypted MLS groups (OpenMLS), up to [`MAX_PRIVATE_GROUP_MEMBERS`]
//! members, where only the founder invites. Until v0.1.24 this code was
//! (wrongly) used for Communities; open communities now live in
//! `open_community.rs`. The FFI keeps the old `community_*` names as legacy
//! aliases so the current Android/iOS UI still builds until the §5U UI pack
//! lands; new code calls the `private_group_*` names.
//!
//! The MLS group ID is 32 random bytes; the group name and roster travel only
//! inside pairwise E2EE messages, never in the GroupContext or authenticated
//! data (§23.10.1(3)).
//!
//! Control traffic travels as pairwise E2EE app messages between contacts
//! (`application/omcp-community`, no chat row): `Invite`, `Join`
//! (KeyPackage), `Welcome`, `Commit`, `Group` (MLS application message,
//! forwarded unchanged to the sender's contacts in the roster) and `Rekey`.
//!
//! Hard rule (§23.10.5, MR17): KeyPackages and Welcomes carrying a team
//! identity are refused. A valid `GROUP_TAKEDOWN` erases the group (MLS
//! state, roster, invites) and its `group_ref` stays tombstoned.

use std::collections::{BTreeMap, HashMap, HashSet};

use omcp_core::burn::AppContent;
use omcp_core::live::community::{community_stream_key, CommunityLive, Rekey};
use omcp_core::live::host::HostSession;
use omcp_core::live::LiveInvite;
use omcp_core::mls::group::{MlsMemberState, OmcpMlsMember};
use serde::{Deserialize, Serialize};

use crate::{invalid, now_ms, parse_id, FfiError, NodeState, OmcpNode};
use omcp_core::group_tags as gt;

pub const COMMUNITY_MIME: &str = "application/omcp-community";
const STATE_KEY: &[u8] = b"communities-v1";
const MAX_NAME: usize = 60;
/// Private groups hold up to 32 members.
pub const MAX_PRIVATE_GROUP_MEMBERS: usize = omcp_core::mls::MAX_SMALL_GROUP_MEMBERS;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CMsg {
    Invite {
        id: [u8; 32],
        name: String,
    },
    Join {
        id: [u8; 32],
        kp: Vec<u8>,
    },
    Welcome {
        id: [u8; 32],
        name: String,
        founder: [u8; 32],
        welcome: Vec<u8>,
        roster: Vec<([u8; 32], String)>,
    },
    Commit {
        id: [u8; 32],
        commit: Vec<u8>,
        roster: Vec<([u8; 32], String)>,
    },
    Group {
        id: [u8; 32],
        ct: Vec<u8>,
    },
    Rekey {
        rekey: Vec<u8>,
    },
}

/// §20.7 private-invite glue: a founder-side `Join` built from an
/// `InviteReply`'s KeyPackage.
pub(crate) fn cmsg_join(id: [u8; 32], kp: Vec<u8>) -> Vec<u8> {
    CMsg::Join { id, kp }.encode()
}

/// `(group id, founder)` of an encoded `Welcome`, for the invite path.
pub(crate) fn cmsg_welcome_ids(b: &[u8]) -> Option<([u8; 32], [u8; 32], String)> {
    match CMsg::decode(b)? {
        CMsg::Welcome {
            id, founder, name, ..
        } => Some((id, founder, name)),
        _ => None,
    }
}

/// Is this an encoded `Welcome` for group `id`?
pub(crate) fn cmsg_is_welcome(b: &[u8]) -> bool {
    matches!(CMsg::decode(b), Some(CMsg::Welcome { .. }))
}

impl CMsg {
    fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        ciborium::into_writer(self, &mut b).expect("cbor to vec");
        b
    }
    fn decode(b: &[u8]) -> Option<Self> {
        if b.len() > 256 * 1024 {
            return None;
        }
        ciborium::from_reader(b).ok()
    }
}

pub(crate) struct Community {
    pub name: String,
    pub founder: [u8; 32],
    pub roster: BTreeMap<[u8; 32], String>,
    pub member: OmcpMlsMember,
    /// Received application messages with their MLS signed bytes (newest
    /// last), for reports. Erased with the group.
    pub signed: std::collections::VecDeque<KeptSigned>,
    /// Text messages, oldest first (≤ [`MAX_KEPT_SIGNED`]).
    pub msgs: std::collections::VecDeque<GroupText>,
    /// Key rotations (member left or removed), oldest first (group-security).
    pub key_events: Vec<KeyEvent>,
}

/// One group key rotation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct KeyEvent {
    /// 1 = left, 2 = removed by the founder.
    pub kind: u8,
    pub name: String,
    pub at_ms: u64,
    /// Members holding the new key.
    pub members: u32,
}
const KEYEV_KEY: &[u8] = b"group-keyev-v1";
const MAX_KEY_EVENTS: usize = 100;

/// Group security screen data.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GroupSecurity {
    /// 60 hex chars of the group key fingerprint (shown as 3×5 groups of 4).
    pub fingerprint_hex: String,
    pub events: Vec<GroupKeyEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GroupKeyEvent {
    /// "left" or "removed".
    pub kind: String,
    pub name: String,
    pub at_ms: u64,
    pub members: u32,
}

impl Community {
    fn log_rotation(&mut self, kind: u8, name: String) {
        self.key_events.push(KeyEvent {
            kind,
            name,
            at_ms: now_ms(),
            members: self.roster.len() as u32,
        });
        if self.key_events.len() > MAX_KEY_EVENTS {
            self.key_events.remove(0);
        }
    }
}

/// One private-group text message.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct GroupText {
    /// Same id as the kept signed bytes (first 16 bytes of the ciphertext hash).
    pub id: [u8; 16],
    pub sender: [u8; 32],
    pub body: String,
    pub ms: u64,
}

/// Prefix of a text message inside the MLS application data (anything else
/// is a live announcement).
const GROUP_TEXT_MAGIC: &[u8] = b"OMCP-gtext-v1\x00";
/// Text message limit (bytes, UTF-8).
pub const MAX_GROUP_TEXT: usize = 4096;
const MSGS_KEY: &[u8] = b"group-msgs-v1";

/// A private-group text message for the UI.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GroupMessage {
    pub message_id_hex: String,
    pub sender_hex: String,
    pub sender_name: String,
    pub body: String,
    pub at_ms: u64,
    pub mine: bool,
    /// Received with its MLS signed bytes, so it can be reported.
    pub reportable: bool,
}

/// (message id, signed bytes, body).
pub(crate) type KeptSigned = ([u8; 16], omcp_core::mls::SignedAppMessage, Vec<u8>);
/// Signed messages kept per group (= the evidence file maximum).
const MAX_KEPT_SIGNED: usize = 500;
const SIGNED_KEY: &[u8] = b"group-signed-v1";

/// (id, name, founder, roster, MLS state)
type SavedCommunity = (
    [u8; 32],
    String,
    [u8; 32],
    Vec<([u8; 32], String)>,
    MlsMemberState,
);

#[derive(Serialize, Deserialize, Default)]
struct Saved {
    list: Vec<SavedCommunity>,
    invites: Vec<([u8; 32], [u8; 32], String)>,
    joining: Vec<([u8; 32], String, MlsMemberState)>,
    invited: Vec<([u8; 32], [u8; 32])>,
}

#[derive(Default)]
pub(crate) struct CommunityState {
    pub list: HashMap<[u8; 32], Community>,
    /// Invites received: community → (from contact, name).
    pub invites: HashMap<[u8; 32], ([u8; 32], String)>,
    /// Accepted, waiting for the Welcome: community → (name, fresh member).
    pub(crate) joining: HashMap<[u8; 32], (String, OmcpMlsMember)>,
    /// Founder: (community, contact) invites we sent.
    pub(crate) invited: HashSet<([u8; 32], [u8; 32])>,
    /// Pairwise messages to send once the node lock is released.
    pub outbox: Vec<([u8; 32], Vec<u8>)>,
    /// Gossip dedup (BLAKE2b of forwarded group / rekey messages).
    seen: HashSet<[u8; 32]>,
    /// Test hook: group-tag envelopes this node published.
    #[cfg(test)]
    pub test_log: Vec<omcp_core::EnvelopeV1>,
}

fn h32(parts: &[&[u8]]) -> [u8; 32] {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(32).expect("32");
    for p in parts {
        h.update(p);
    }
    let mut o = [0u8; 32];
    h.finalize_variable(&mut o).expect("32");
    o
}

fn mls(_: omcp_core::error::OmcpError) -> FfiError {
    invalid("community mls")
}

/// A community the user belongs to.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommunityInfo {
    pub community_id_hex: String,
    pub name: String,
    pub members: u32,
    pub founder_is_me: bool,
    /// Team takedown reason code (§23.9), `None` if not removed.
    pub removed_reason: Option<u8>,
    /// Active lives in this community (ours included).
    pub live_count: u32,
    /// After a backup restore: "Checking…" until one full digest exchange
    /// and its team actions are applied (MR30). Nothing is shown or relayed.
    pub checking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommunityInviteInfo {
    pub community_id_hex: String,
    pub name: String,
    pub from_contact_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommunityMember {
    pub peer_id_hex: String,
    pub name: String,
    pub is_contact: bool,
    pub is_me: bool,
}

impl NodeState {
    pub(crate) fn load_communities(&mut self) -> Result<(), FfiError> {
        let s: Saved = self.db.get_meta(STATE_KEY)?.unwrap_or_default();
        let c = &mut self.mesh.community;
        for (id, name, founder, roster, m) in s.list {
            if let Ok(member) = OmcpMlsMember::restore(&m) {
                c.list.insert(
                    id,
                    Community {
                        name,
                        founder,
                        roster: roster.into_iter().collect(),
                        member,
                        signed: Default::default(),
                        msgs: Default::default(),
                        key_events: Default::default(),
                    },
                );
            }
        }
        for (id, from, name) in s.invites {
            c.invites.insert(id, (from, name));
        }
        for (id, name, m) in s.joining {
            if let Ok(member) = OmcpMlsMember::restore(&m) {
                c.joining.insert(id, (name, member));
            }
        }
        c.invited = s.invited.into_iter().collect();
        let kept: Vec<([u8; 32], Vec<KeptSigned>)> =
            self.db.get_meta(SIGNED_KEY)?.unwrap_or_default();
        for (id, v) in kept {
            if let Some(k) = self.mesh.community.list.get_mut(&id) {
                k.signed = v.into_iter().collect();
            }
        }
        let msgs: Vec<([u8; 32], Vec<GroupText>)> = self.db.get_meta(MSGS_KEY)?.unwrap_or_default();
        for (id, v) in msgs {
            if let Some(k) = self.mesh.community.list.get_mut(&id) {
                k.msgs = v.into_iter().collect();
            }
        }
        Ok(())
    }

    pub(crate) fn save_communities(&self) -> Result<(), FfiError> {
        let c = &self.mesh.community;
        let mut s = Saved::default();
        for (id, k) in &c.list {
            s.list.push((
                *id,
                k.name.clone(),
                k.founder,
                k.roster.iter().map(|(a, b)| (*a, b.clone())).collect(),
                k.member.export_state().map_err(mls)?,
            ));
        }
        for (id, (from, name)) in &c.invites {
            s.invites.push((*id, *from, name.clone()));
        }
        for (id, (name, m)) in &c.joining {
            s.joining
                .push((*id, name.clone(), m.export_state().map_err(mls)?));
        }
        s.invited = c.invited.iter().copied().collect();
        self.db.put_meta(STATE_KEY, &s)?;
        let kept: Vec<([u8; 32], Vec<KeptSigned>)> = c
            .list
            .iter()
            .filter(|(_, k)| !k.signed.is_empty())
            .map(|(id, k)| (*id, k.signed.iter().cloned().collect()))
            .collect();
        self.db.put_meta(SIGNED_KEY, &kept)?;
        let msgs: Vec<([u8; 32], Vec<GroupText>)> = c
            .list
            .iter()
            .filter(|(_, k)| !k.msgs.is_empty())
            .map(|(id, k)| (*id, k.msgs.iter().cloned().collect()))
            .collect();
        self.db.put_meta(MSGS_KEY, &msgs)?;
        let ev: Vec<([u8; 32], Vec<KeyEvent>)> = c
            .list
            .iter()
            .filter(|(_, k)| !k.key_events.is_empty())
            .map(|(id, k)| (*id, k.key_events.clone()))
            .collect();
        self.db.put_meta(KEYEV_KEY, &ev)?;
        Ok(())
    }

    /// Whether this group has a team tombstone (taken down, or restored:
    /// a re-formed group needs a new random ID, MR25).
    pub(crate) fn removed(&self, id: &[u8; 32]) -> bool {
        self.mesh
            .community
            .list
            .get(id)
            .and_then(|c| c.member.moderation_group_ref().ok())
            .map(|r| self.mesh.moderation.is_group_tombstoned(&r))
            .unwrap_or(false)
    }

    /// Full erasure of taken-down private groups (§23.10.2(3)): MLS state,
    /// roster and pending invites. 1:1 chats are never touched.
    pub(crate) fn enforce_group_moderation(&mut self) -> Result<(), FfiError> {
        let gone: Vec<[u8; 32]> = self
            .mesh
            .community
            .list
            .keys()
            .filter(|id| self.removed(id))
            .copied()
            .collect();
        if gone.is_empty() {
            return Ok(());
        }
        for id in &gone {
            if let Some(mut c) = self.mesh.community.list.remove(id) {
                c.member.erase_group();
            }
            self.mesh.community.invites.remove(id);
            self.mesh.community.joining.remove(id);
            self.mesh.community.invited.retain(|(g, _)| g != id);
        }
        self.save_communities()
    }

    /// Our contacts in the roster, minus `skip`.
    fn roster_contacts(&self, id: &[u8; 32], skip: &[[u8; 32]]) -> Vec<[u8; 32]> {
        self.mesh
            .community
            .list
            .get(id)
            .map(|c| {
                c.roster
                    .keys()
                    .filter(|p| self.contacts.contains_key(*p) && !skip.contains(p))
                    .copied()
                    .collect()
            })
            .unwrap_or_default()
    }

    fn post(&mut self, to: [u8; 32], m: &CMsg) {
        self.mesh.community.outbox.push((to, m.encode()));
    }

    /// Tag secret of group `id` for the current tag epoch.
    fn group_secret(&self, id: &[u8; 32]) -> Option<[u8; 32]> {
        let e = omcp_core::sync::tag::tag_epoch(now_ms());
        self.mesh
            .community
            .list
            .get(id)?
            .member
            .tag_exporter(e)
            .ok()
    }

    /// Publish a group frame on the group's MLS tag (§22.7), sealed with
    /// `secret` (taken before a commit for commits). Reaches members who
    /// are not our contacts; relays see an opaque envelope only.
    fn publish_group_with(&mut self, secret: [u8; 32], m: &CMsg) -> Result<(), FfiError> {
        let now = now_ms();
        let e = omcp_core::sync::tag::tag_epoch(now);
        let tag = gt::group_tag(&secret, e).map_err(mls)?;
        let ttl = self.mesh.engine.new_ttl();
        let env = gt::seal(
            &secret,
            &tag,
            &m.encode(),
            crate::mesh::rand16(),
            now,
            now + omcp_core::sync::budget::DEFAULT_EXPIRY_MS,
            ttl,
        )
        .map_err(mls)?;
        #[cfg(test)]
        self.mesh.community.test_log.push(env.clone());
        let ev = self.mesh.engine.submit_own(env, now)?;
        self.route(ev)?;
        Ok(())
    }

    fn publish_group(&mut self, id: &[u8; 32], m: &CMsg) -> Result<(), FfiError> {
        match self.group_secret(id) {
            Some(s) => self.publish_group_with(s, m),
            None => Ok(()),
        }
    }

    /// A group-tag envelope for group `id` (tag epoch `e`).
    pub(crate) fn on_group_envelope(
        &mut self,
        id: [u8; 32],
        e: u64,
        env: &omcp_core::EnvelopeV1,
    ) -> Result<(), FfiError> {
        let Some(secret) = self
            .mesh
            .community
            .list
            .get(&id)
            .and_then(|c| c.member.tag_exporter(e).ok())
        else {
            return Ok(());
        };
        let Ok(frame) = gt::open(&secret, env) else {
            return Ok(());
        };
        match CMsg::decode(&frame) {
            Some(CMsg::Group { id: gid, ct }) if gid == id => self.on_group([0; 32], id, ct),
            Some(CMsg::Rekey { rekey }) => self.on_rekey([0; 32], rekey),
            // Commits are MLS-authenticated (only members can produce one);
            // the founder is the only committer. The roster is checked against
            // the MLS member list in the Commit handler.
            Some(CMsg::Commit { id: gid, .. }) if gid == id => {
                let founder = self.mesh.community.list[&id].founder;
                self.on_community_msg(founder, &frame)
            }
            _ => Ok(()),
        }
    }

    /// (tag → (group, tag epoch)) for every private group, epochs e-1 and e.
    pub(crate) fn group_wants(
        &self,
        now: u64,
    ) -> Vec<(omcp_core::sync::tag::MailboxTag, [u8; 32], u64)> {
        let e = omcp_core::sync::tag::tag_epoch(now);
        let mut out = Vec::new();
        for (id, c) in &self.mesh.community.list {
            if self.removed(id) {
                continue;
            }
            for t in [e.saturating_sub(1), e] {
                if let Ok(ts) = c.member.tag_exporter(t) {
                    if let Ok(tag) = gt::group_tag(&ts, t) {
                        out.push((tag, *id, t));
                    }
                }
            }
        }
        out
    }

    /// Fingerprint input: (group, MLS epoch) of every private group.
    pub(crate) fn group_epochs(&self) -> Vec<([u8; 32], u64)> {
        let mut v: Vec<_> = self
            .mesh
            .community
            .list
            .iter()
            .map(|(id, c)| (*id, c.member.epoch().unwrap_or(0)))
            .collect();
        v.sort();
        v
    }

    /// Seal a live start/end into the community group and send it to every
    /// member we know directly.
    pub(crate) fn announce_live(
        &mut self,
        id: &[u8; 32],
        a: &CommunityLive,
    ) -> Result<(), FfiError> {
        let body = a.encode().map_err(|_| invalid("community live"))?;
        let ct = {
            let c = self
                .mesh
                .community
                .list
                .get_mut(id)
                .ok_or_else(|| invalid("unknown community"))?;
            c.member.encrypt_app_message(&body).map_err(mls)?
        };
        self.save_communities()?;
        self.mesh.community.seen.insert(h32(&[&ct]));
        let msg = CMsg::Group { id: *id, ct };
        for p in self.roster_contacts(id, &[]) {
            self.post(p, &msg);
        }
        self.publish_group(id, &msg)
    }

    /// Host: rotate after removing `removed`; signed rekey to everyone else.
    pub(crate) fn community_rekey(
        &mut self,
        id: &[u8; 32],
        excluded: Vec<[u8; 32]>,
    ) -> Result<(), FfiError> {
        let r = {
            let h = self
                .mesh
                .live
                .host
                .as_mut()
                .ok_or_else(|| invalid("not live"))?;
            h.rotate().map_err(|_| invalid("rotate"))?;
            let sk = h.signer().ok_or_else(|| invalid("unsigned live"))?;
            Rekey::sign(*id, h.key(), excluded.clone(), sk).map_err(|_| invalid("rekey"))?
        };
        let bytes = r.encode().map_err(|_| invalid("rekey"))?;
        self.mesh.community.seen.insert(h32(&[&bytes]));
        let msg = CMsg::Rekey { rekey: bytes };
        for p in self.roster_contacts(id, &excluded) {
            self.post(p, &msg);
        }
        self.publish_group(id, &msg)
    }

    /// A community message from contact `peer` (pairwise E2EE).
    pub(crate) fn on_community_msg(
        &mut self,
        peer: [u8; 32],
        bytes: &[u8],
    ) -> Result<(), FfiError> {
        let Some(m) = CMsg::decode(bytes) else {
            return Ok(());
        };
        match m {
            CMsg::Invite { id, name } => {
                if self.mesh.community.list.contains_key(&id) || self.removed(&id) {
                    return Ok(());
                }
                let name: String = name.chars().take(MAX_NAME).collect();
                self.mesh.community.invites.insert(id, (peer, name));
                self.save_communities()?;
            }
            CMsg::Join { id, kp } => {
                // Founder: only contacts we invited, only while not removed.
                if !self.mesh.community.invited.contains(&(id, peer)) || self.removed(&id) {
                    return Ok(());
                }
                let name = self
                    .contacts
                    .get(&peer)
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let team = self.mesh.team.clone();
                let old_secret = self.group_secret(&id);
                let (commit, welcome, roster, others, cname, founder) = {
                    let Some(c) = self.mesh.community.list.get_mut(&id) else {
                        return Ok(());
                    };
                    if c.roster.len() >= MAX_PRIVATE_GROUP_MEMBERS {
                        return Ok(());
                    }
                    let kp = c.member.parse_key_package(&kp).map_err(mls)?;
                    let (commit, welcome) = c
                        .member
                        .add_members_excluding_team(&team, &[kp])
                        .map_err(mls)?;
                    let others: Vec<[u8; 32]> = c.roster.keys().copied().collect();
                    c.roster.insert(peer, name);
                    let roster: Vec<_> = c.roster.iter().map(|(a, b)| (*a, b.clone())).collect();
                    (commit, welcome, roster, others, c.name.clone(), c.founder)
                };
                self.mesh.community.invited.remove(&(id, peer));
                self.save_communities()?;
                for o in others {
                    if self.contacts.contains_key(&o) {
                        self.post(
                            o,
                            &CMsg::Commit {
                                id,
                                commit: commit.clone(),
                                roster: roster.clone(),
                            },
                        );
                    }
                }
                if let Some(sec) = old_secret {
                    self.publish_group_with(
                        sec,
                        &CMsg::Commit {
                            id,
                            commit: commit.clone(),
                            roster: roster.clone(),
                        },
                    )?;
                }
                self.post(
                    peer,
                    &CMsg::Welcome {
                        id,
                        name: cname,
                        founder,
                        welcome,
                        roster,
                    },
                );
            }
            CMsg::Welcome {
                id,
                name,
                founder,
                welcome,
                roster,
            } => {
                if founder != peer || self.removed(&id) {
                    return Ok(());
                }
                let Some((_, mut member)) = self.mesh.community.joining.remove(&id) else {
                    return Ok(());
                };
                member
                    .join_from_welcome_excluding_team(&self.mesh.team, &welcome)
                    .map_err(mls)?;
                let tomb = member
                    .moderation_group_ref()
                    .map(|r| self.mesh.moderation.is_group_tombstoned(&r))
                    .unwrap_or(true);
                if tomb {
                    member.erase_group();
                    return Ok(()); // tombstoned group_ref: refuse the Welcome
                }
                self.mesh.community.list.insert(
                    id,
                    Community {
                        name: name.chars().take(MAX_NAME).collect(),
                        founder,
                        roster: roster.into_iter().collect(),
                        member,
                        signed: Default::default(),
                        msgs: Default::default(),
                        key_events: Default::default(),
                    },
                );
                self.save_communities()?;
            }
            CMsg::Commit { id, commit, roster } => {
                let Some(c) = self.mesh.community.list.get_mut(&id) else {
                    return Ok(());
                };
                if c.founder != peer {
                    return Ok(());
                }
                if c.member.process_protocol_message(&commit).is_err() {
                    return Ok(()); // stale, replayed or not for this epoch
                }
                let ids: HashSet<Vec<u8>> = c
                    .member
                    .member_identities()
                    .map_err(mls)?
                    .into_iter()
                    .map(|(_, cred)| cred)
                    .collect();
                let new: BTreeMap<[u8; 32], String> = roster
                    .into_iter()
                    .filter(|(k, _)| ids.contains(k.as_slice()))
                    .collect();
                let gone: Vec<String> = c
                    .roster
                    .iter()
                    .filter(|(k, _)| !new.contains_key(*k))
                    .map(|(_, n)| n.clone())
                    .collect();
                c.roster = new;
                for n in gone {
                    c.log_rotation(2, n);
                }
                self.save_communities()?;
            }
            CMsg::Group { id, ct } => self.on_group(peer, id, ct)?,
            CMsg::Rekey { rekey } => self.on_rekey(peer, rekey)?,
        }
        Ok(())
    }

    fn on_group(&mut self, peer: [u8; 32], id: [u8; 32], ct: Vec<u8>) -> Result<(), FfiError> {
        let h = h32(&[&ct]);
        if self.mesh.restore_hold.is_some() {
            return Ok(()); // MR30: not shown or relayed until checked
        }
        if !self.mesh.community.seen.insert(h) || self.removed(&id) {
            return Ok(()); // duplicate, or community taken down: not applied, not forwarded
        }
        let (plain, sender) = {
            let Some(c) = self.mesh.community.list.get_mut(&id) else {
                return Ok(());
            };
            let Ok((plain, _aad, leaf, signed)) = c.member.decrypt_app_message_full(&ct) else {
                return Ok(());
            };
            // §23.10.1(2): keep the signed bytes with every application
            // message so a member can report it later.
            if let Some(sg) = signed {
                c.signed
                    .push_back((h32(&[&ct])[..16].try_into().expect("16"), sg, plain.clone()));
                while c.signed.len() > MAX_KEPT_SIGNED {
                    c.signed.pop_front();
                }
            }
            let ids = c.member.member_identities().map_err(mls)?;
            let Some(sender) = ids
                .into_iter()
                .find(|(l, _)| *l == leaf)
                .and_then(|(_, cred)| <[u8; 32]>::try_from(cred.as_slice()).ok())
            else {
                return Ok(());
            };
            (plain, sender)
        };
        if let Some(text) = plain.strip_prefix(GROUP_TEXT_MAGIC) {
            if let Ok(body) = std::str::from_utf8(text) {
                if body.len() <= MAX_GROUP_TEXT {
                    let c = self.mesh.community.list.get_mut(&id).expect("checked");
                    c.msgs.push_back(GroupText {
                        id: h[..16].try_into().expect("16"),
                        sender,
                        body: body.to_string(),
                        ms: now_ms(),
                    });
                    while c.msgs.len() > MAX_KEPT_SIGNED {
                        c.msgs.pop_front();
                    }
                }
            }
            self.save_communities()?;
            let msg = CMsg::Group { id, ct };
            for p in self.roster_contacts(&id, &[peer, sender]) {
                self.post(p, &msg);
            }
            return Ok(());
        }
        self.save_communities()?;
        let Ok(a) = CommunityLive::decode(&plain) else {
            return Ok(());
        };
        if a.community_id != id || !a.host_matches(&sender) {
            return Ok(());
        }
        // §20.3: a Live announcement naming an open community is ignored.
        if self.mesh.open.get(&a.community_id).is_some()
            && !omcp_core::community::accept_community_live_announcement(&a.community_id)
        {
            return Ok(());
        }
        if !self.mesh.moderation.live_allowed(&a.stream_id, Some(&id))
            || self
                .mesh
                .moderation
                .live_host_blocked(&id, &sender, now_ms())
        {
            return Ok(());
        }
        // Gossip on, unchanged, to members we know directly.
        let msg = CMsg::Group { id, ct };
        for p in self.roster_contacts(&id, &[peer, sender]) {
            self.post(p, &msg);
        }
        if !a.live {
            self.live_ended_remote(&a.stream_id);
            return Ok(());
        }
        if self.mesh.live.known.contains_key(&a.stream_id) {
            return Ok(());
        }
        let key = {
            let c = &self.mesh.community.list[&id];
            let ex = c.member.live_exporter(&id, &a.stream_id).map_err(mls)?;
            community_stream_key(&ex, &a.nonce, &id, a.stream_id)
        };
        let inv = LiveInvite {
            host: sender,
            title: a.title.clone(),
            started_ms: a.started_ms,
            video: a.video,
            max_hops: a.max_hops,
            key,
            live: true,
            host_key: a.host_key,
            community_id: Some(id),
        };
        self.mesh.live.known.insert(a.stream_id, (sender, inv));
        self.mesh.live.started.push(crate::live::LiveStarted {
            stream_id_hex: hex::encode(a.stream_id),
            community_id_hex: hex::encode(id),
        });
        Ok(())
    }

    fn on_rekey(&mut self, peer: [u8; 32], bytes: Vec<u8>) -> Result<(), FfiError> {
        if !self.mesh.community.seen.insert(h32(&[&bytes])) {
            return Ok(());
        }
        let Ok(r) = Rekey::decode(&bytes) else {
            return Ok(());
        };
        let Some((host, inv)) = self.mesh.live.known.get(&r.stream_id).cloned() else {
            return Ok(());
        };
        if inv.community_id != Some(r.community_id)
            || !r.verify(&inv.host_key)
            || r.key_epoch <= inv.key.key_epoch
        {
            return Ok(());
        }
        let me = self
            .mesh
            .community
            .list
            .get(&r.community_id)
            .map(|c| c.member.identity.clone())
            .unwrap_or_default();
        let excluded_me = r.excluded.iter().any(|e| e.as_slice() == me.as_slice());
        let mut skip = r.excluded.clone();
        skip.extend([peer, host]);
        let msg = CMsg::Rekey {
            rekey: bytes.clone(),
        };
        for p in self.roster_contacts(&r.community_id, &skip) {
            self.post(p, &msg);
        }
        if excluded_me {
            return Ok(());
        }
        let mut inv = inv;
        inv.key = r.key();
        if let Some(l) = self.mesh.live.listening.as_mut() {
            l.update(inv.clone());
        }
        self.mesh.live.known.insert(r.stream_id, (host, inv));
        Ok(())
    }

    fn community_info(&self, id: &[u8; 32], c: &Community, me: &[u8; 32]) -> CommunityInfo {
        let lv = &self.mesh.live;
        let mine = lv
            .host
            .as_ref()
            .filter(|h| h.community_id == Some(*id))
            .is_some() as u32;
        let others = lv
            .known
            .values()
            .filter(|(_, i)| i.community_id == Some(*id))
            .count() as u32;
        CommunityInfo {
            community_id_hex: hex::encode(id),
            name: c.name.clone(),
            members: c.roster.len() as u32,
            founder_is_me: &c.founder == me,
            removed_reason: self.mesh.moderation.removal(id).map(|r| r.reason.code()),
            live_count: mine + others,
            checking: self.mesh.restore_hold.is_some(),
        }
    }
}

impl OmcpNode {
    /// Send queued community messages (outside the node lock).
    pub(crate) fn drain_community_outbox(&self) {
        let out = std::mem::take(&mut self.inner.lock().mesh.community.outbox);
        for (to, bytes) in out {
            let _ = self.queue_app(
                &hex::encode(to),
                AppContent::Media {
                    mime: COMMUNITY_MIME.into(),
                    bytes,
                },
                None,
                None,
                true,
            );
        }
    }

    fn me(&self) -> [u8; 32] {
        self.inner.peer_id
    }

    pub(crate) fn signing_key(&self) -> Result<ed25519_dalek::SigningKey, FfiError> {
        omcp_core::device::identity_signing_key(&self.inner.seed).map_err(|_| invalid("identity"))
    }
}

fn parse_cid(h: &str) -> Result<[u8; 32], FfiError> {
    parse_id(h)
}

#[uniffi::export]
impl OmcpNode {
    /// Create a community (you are the founder and its only member).
    pub fn community_create(&self, name: String) -> Result<CommunityInfo, FfiError> {
        let name: String = name.trim().chars().take(MAX_NAME).collect();
        if name.is_empty() {
            return Err(invalid("community name"));
        }
        let me = self.me();
        let mut id = [0u8; 32];
        getrandom::getrandom(&mut id).map_err(|_| invalid("rng"))?;
        let mut member = OmcpMlsMember::generate(me.to_vec()).map_err(mls)?;
        member.create_group().map_err(mls)?;
        let mut st = self.inner.lock();
        let mut roster = BTreeMap::new();
        roster.insert(me, self.inner.name.clone());
        st.mesh.community.list.insert(
            id,
            Community {
                name,
                founder: me,
                roster,
                member,
                signed: Default::default(),
                msgs: Default::default(),
                key_events: Default::default(),
            },
        );
        st.save_communities()?;
        let c = &st.mesh.community.list[&id];
        Ok(st.community_info(&id, c, &me))
    }

    /// Founder: invite a contact (they accept, then get the MLS Welcome).
    pub fn community_invite(
        &self,
        community_id_hex: String,
        contact_id: String,
    ) -> Result<(), FfiError> {
        let id = parse_cid(&community_id_hex)?;
        let to = parse_id(&contact_id)?;
        {
            let mut st = self.inner.lock();
            if st.removed(&id) {
                return Err(invalid("community removed"));
            }
            let c = st
                .mesh
                .community
                .list
                .get(&id)
                .ok_or_else(|| invalid("unknown community"))?;
            if c.founder != self.me() {
                return Err(invalid("only the founder invites"));
            }
            if c.roster.len() >= MAX_PRIVATE_GROUP_MEMBERS {
                return Err(invalid("private groups hold up to 32 members"));
            }
            let msg = CMsg::Invite {
                id,
                name: c.name.clone(),
            };
            st.mesh.community.invited.insert((id, to));
            st.save_communities()?;
            st.post(to, &msg);
        }
        self.drain_community_outbox();
        Ok(())
    }

    pub fn community_invites(&self) -> Vec<CommunityInviteInfo> {
        let st = self.inner.lock();
        st.mesh
            .community
            .invites
            .iter()
            .filter(|(id, _)| !st.removed(id))
            .map(|(id, (from, name))| CommunityInviteInfo {
                community_id_hex: hex::encode(id),
                name: name.clone(),
                from_contact_id: hex::encode(from),
            })
            .collect()
    }

    /// Accept an invite: send our KeyPackage to the founder.
    pub fn community_accept(&self, community_id_hex: String) -> Result<(), FfiError> {
        let id = parse_cid(&community_id_hex)?;
        {
            let mut st = self.inner.lock();
            if st.removed(&id) {
                return Err(invalid("community removed"));
            }
            let (from, name) = st
                .mesh
                .community
                .invites
                .remove(&id)
                .ok_or_else(|| invalid("no invite"))?;
            let member = OmcpMlsMember::generate(self.me().to_vec()).map_err(mls)?;
            let kp = member.key_package_bytes().map_err(mls)?;
            st.mesh.community.joining.insert(id, (name, member));
            st.save_communities()?;
            st.post(from, &CMsg::Join { id, kp });
        }
        self.drain_community_outbox();
        Ok(())
    }

    pub fn community_decline(&self, community_id_hex: String) -> Result<(), FfiError> {
        let id = parse_cid(&community_id_hex)?;
        let mut st = self.inner.lock();
        st.mesh.community.invites.remove(&id);
        st.save_communities()
    }

    /// Communities we belong to (removed ones too, flagged, so the UI can say why).
    pub fn communities(&self) -> Vec<CommunityInfo> {
        let me = self.me();
        let st = self.inner.lock();
        let mut v: Vec<_> = st
            .mesh
            .community
            .list
            .iter()
            .map(|(id, c)| st.community_info(id, c, &me))
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn community_members(&self, community_id_hex: String) -> Vec<CommunityMember> {
        let Ok(id) = parse_cid(&community_id_hex) else {
            return Vec::new();
        };
        let me = self.me();
        let st = self.inner.lock();
        st.mesh
            .community
            .list
            .get(&id)
            .map(|c| {
                c.roster
                    .iter()
                    .map(|(p, n)| CommunityMember {
                        peer_id_hex: hex::encode(p),
                        name: n.clone(),
                        is_contact: st.contacts.contains_key(p),
                        is_me: p == &me,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Go live in a community: key from the community MLS exporter + random
    /// nonce; every member is told (lock-screen notification, no content).
    pub fn live_start_community(
        &self,
        community_id_hex: String,
        title: String,
        video: bool,
    ) -> Result<String, FfiError> {
        let id = parse_cid(&community_id_hex)?;
        let sk = self.signing_key()?;
        let me = self.me();
        let sid = {
            let mut st = self.inner.lock();
            if st.removed(&id) {
                return Err(invalid("community removed"));
            }
            // §20.3: no Live in open communities (deferred).
            if st.mesh.open.get(&id).is_some() || !st.mesh.community.list.contains_key(&id) {
                return Err(invalid("Live is only in 1:1 chats and private groups"));
            }
            // MR29: 24 h after a Live takedown the host can't restart here.
            if st.mesh.moderation.live_host_blocked(&id, &me, now_ms()) {
                return Err(invalid("live blocked for 24 h after a takedown"));
            }
            let mut sid = [0u8; 16];
            let mut nonce = [0u8; 32];
            getrandom::getrandom(&mut sid).map_err(|_| invalid("rng"))?;
            getrandom::getrandom(&mut nonce).map_err(|_| invalid("rng"))?;
            let key = {
                let c = st
                    .mesh
                    .community
                    .list
                    .get(&id)
                    .ok_or_else(|| invalid("unknown community"))?;
                let ex = c.member.live_exporter(&id, &sid).map_err(mls)?;
                community_stream_key(&ex, &nonce, &id, sid)
            };
            let mut h = HostSession::with_key(me, &title, video, now_ms(), key)
                .map_err(|_| invalid("live"))?;
            h.set_signer(sk);
            h.community_id = Some(id);
            let a = CommunityLive {
                community_id: id,
                stream_id: sid,
                nonce,
                title: h.title.clone(),
                started_ms: h.started_ms,
                video,
                max_hops: h.max_hops,
                live: true,
                host_key: h.host_key(),
            };
            st.stop_hosting_quietly();
            st.mesh.live.host = Some(h);
            st.mesh.live.community_nonce = Some(nonce);
            st.mesh.live.removed_viewers.clear();
            st.announce_live(&id, &a)?;
            sid
        };
        self.drain_community_outbox();
        Ok(hex::encode(sid))
    }

    /// Active lives in one community (ours first, then newest).
    pub fn live_community_streams(
        &self,
        community_id_hex: String,
    ) -> Vec<crate::live::LiveStreamInfo> {
        let want = community_id_hex.trim().to_lowercase();
        self.live_streams()
            .into_iter()
            .filter(|s| s.community_id_hex == want)
            .collect()
    }
}

/// New names for private groups. The `community_*` names above are legacy
/// aliases kept for the current app UI.
#[uniffi::export]
impl OmcpNode {
    pub fn private_group_create(&self, name: String) -> Result<CommunityInfo, FfiError> {
        self.community_create(name)
    }
    pub fn private_group_invite(
        &self,
        group_id_hex: String,
        contact_id: String,
    ) -> Result<(), FfiError> {
        self.community_invite(group_id_hex, contact_id)
    }
    pub fn private_group_invites(&self) -> Vec<CommunityInviteInfo> {
        self.community_invites()
    }
    pub fn private_group_accept(&self, group_id_hex: String) -> Result<(), FfiError> {
        self.community_accept(group_id_hex)
    }
    pub fn private_group_decline(&self, group_id_hex: String) -> Result<(), FfiError> {
        self.community_decline(group_id_hex)
    }
    pub fn private_groups(&self) -> Vec<CommunityInfo> {
        self.communities()
    }
    pub fn private_group_members(&self, group_id_hex: String) -> Vec<CommunityMember> {
        self.community_members(group_id_hex)
    }
    /// Private groups and 1:1 chats are end-to-end encrypted (MR27 marker).
    pub fn private_group_is_e2ee(&self) -> bool {
        true
    }
    /// `group_ref` (hex) of a private group, the only name the team ever sees.
    pub fn private_group_ref(&self, group_id_hex: String) -> Option<String> {
        let id = parse_cid(&group_id_hex).ok()?;
        let st = self.inner.lock();
        let c = st.mesh.community.list.get(&id)?;
        c.member.moderation_group_ref().ok().map(hex::encode)
    }

    /// Send a text message to a private group (E2EE, MLS). ≤ 4 KB.
    pub fn private_group_send(
        &self,
        group_id_hex: String,
        text: String,
    ) -> Result<String, FfiError> {
        let id = parse_cid(&group_id_hex)?;
        if text.trim().is_empty() || text.len() > MAX_GROUP_TEXT {
            return Err(invalid("message must be 1..4096 bytes"));
        }
        let me = self.me();
        let mid = {
            let mut st = self.inner.lock();
            if st.removed(&id) {
                return Err(invalid("group removed"));
            }
            if st.mesh.restore_hold.is_some() {
                return Err(invalid("checking after restore"));
            }
            let mut body = GROUP_TEXT_MAGIC.to_vec();
            body.extend_from_slice(text.as_bytes());
            let ct = {
                let c = st
                    .mesh
                    .community
                    .list
                    .get_mut(&id)
                    .ok_or_else(|| invalid("unknown group"))?;
                c.member.encrypt_app_message(&body).map_err(mls)?
            };
            let h = h32(&[&ct]);
            st.mesh.community.seen.insert(h);
            let mid: [u8; 16] = h[..16].try_into().expect("16");
            {
                let c = st.mesh.community.list.get_mut(&id).expect("checked");
                c.msgs.push_back(GroupText {
                    id: mid,
                    sender: me,
                    body: text,
                    ms: now_ms(),
                });
                while c.msgs.len() > MAX_KEPT_SIGNED {
                    c.msgs.pop_front();
                }
            }
            st.save_communities()?;
            let msg = CMsg::Group { id, ct };
            for p in st.roster_contacts(&id, &[]) {
                st.post(p, &msg);
            }
            st.publish_group(&id, &msg)?;
            mid
        };
        self.drain_community_outbox();
        Ok(hex::encode(mid))
    }

    /// Text messages of a private group, oldest first. Empty while the
    /// post-restore hold is on (MR30).
    pub fn private_group_messages(&self, group_id_hex: String) -> Vec<GroupMessage> {
        let Ok(id) = parse_cid(&group_id_hex) else {
            return Vec::new();
        };
        let me = self.me();
        let st = self.inner.lock();
        if st.mesh.restore_hold.is_some() || st.removed(&id) {
            return Vec::new();
        }
        let Some(c) = st.mesh.community.list.get(&id) else {
            return Vec::new();
        };
        c.msgs
            .iter()
            .map(|m| GroupMessage {
                message_id_hex: hex::encode(m.id),
                sender_hex: hex::encode(m.sender),
                sender_name: c.roster.get(&m.sender).cloned().unwrap_or_default(),
                body: m.body.clone(),
                at_ms: m.ms,
                mine: m.sender == me,
                reportable: m.sender != me && c.signed.iter().any(|(k, _, _)| *k == m.id),
            })
            .collect()
    }

    /// Anonymous report of picked private-group messages: an
    /// `.omcp-evidence` file built from their kept MLS signed bytes.
    pub fn private_group_report(
        &self,
        group_id_hex: String,
        message_ids_hex: Vec<String>,
        reason_code: u8,
        note: Option<String>,
    ) -> Result<Vec<u8>, FfiError> {
        let id = parse_cid(&group_id_hex)?;
        let want: Vec<[u8; 16]> = message_ids_hex
            .iter()
            .filter_map(|h| hex::decode(h.trim()).ok()?.try_into().ok())
            .collect();
        let picked: Vec<crate::moderation::EvidenceMessage> = {
            let st = self.inner.lock();
            let c = st
                .mesh
                .community
                .list
                .get(&id)
                .ok_or_else(|| invalid("unknown group"))?;
            c.signed
                .iter()
                .filter(|(k, _, _)| want.contains(k))
                .filter_map(|(_, sg, body)| {
                    let r = omcp_core::moderation::evidence::ReportedMessage::from_signed(
                        sg,
                        // The signed application data, prefix included (step e).
                        body.clone(),
                    )
                    .ok()?;
                    Some(crate::moderation::EvidenceMessage {
                        sender_id_hex: hex::encode(r.sender_id),
                        framed_content: r.framed_content,
                        group_context: r.group_context,
                        signature: r.signature,
                        leaf_node: r.leaf_node,
                        wire_format: r.wire_format,
                        body: r.body,
                    })
                })
                .collect()
        };
        if picked.is_empty() {
            return Err(invalid("no reportable message picked"));
        }
        self.report_export_evidence(group_id_hex, picked, reason_code, note)
    }

    /// MR29: whether we're blocked from going live in this group/community
    /// (24 h after a Live takedown of ours there).
    pub fn live_start_blocked(&self, community_id_hex: String) -> bool {
        let Ok(id) = parse_cid(&community_id_hex) else {
            return false;
        };
        let me = self.me();
        self.inner
            .lock()
            .mesh
            .moderation
            .live_host_blocked(&id, &me, now_ms())
    }

    /// Founder removes a member: MLS remove commit (new group key for
    /// everyone else), logged as a key change (group-remove).
    pub fn private_group_remove_member(
        &self,
        group_id_hex: String,
        peer_id_hex: String,
    ) -> Result<(), FfiError> {
        let id = parse_cid(&group_id_hex)?;
        let who = parse_cid(&peer_id_hex)?;
        let me = self.me();
        {
            let mut st = self.inner.lock();
            let old_secret = st.group_secret(&id);
            let (commit, roster) = {
                let c = st
                    .mesh
                    .community
                    .list
                    .get_mut(&id)
                    .ok_or_else(|| invalid("unknown group"))?;
                if c.founder != me || who == me {
                    return Err(invalid("only the founder removes members"));
                }
                let leaf = c
                    .member
                    .member_identities()
                    .map_err(mls)?
                    .into_iter()
                    .find(|(_, cred)| cred.as_slice() == who)
                    .map(|(l, _)| l)
                    .ok_or_else(|| invalid("not a member"))?;
                let commit = c.member.remove_members(&[leaf]).map_err(mls)?;
                let name = c.roster.remove(&who).unwrap_or_default();
                c.log_rotation(2, name);
                let roster: Vec<_> = c.roster.iter().map(|(a, b)| (*a, b.clone())).collect();
                (commit, roster)
            };
            st.save_communities()?;
            for p in st.roster_contacts(&id, &[me]) {
                st.post(
                    p,
                    &CMsg::Commit {
                        id,
                        commit: commit.clone(),
                        roster: roster.clone(),
                    },
                );
            }
            if let Some(sec) = old_secret {
                st.publish_group_with(sec, &CMsg::Commit { id, commit, roster })?;
            }
        }
        self.drain_community_outbox();
        Ok(())
    }

    /// Key fingerprint and key-change log (group-security).
    pub fn private_group_security(&self, group_id_hex: String) -> Result<GroupSecurity, FfiError> {
        let id = parse_cid(&group_id_hex)?;
        let st = self.inner.lock();
        let c = st
            .mesh
            .community
            .list
            .get(&id)
            .ok_or_else(|| invalid("unknown group"))?;
        let fp = c.member.key_fingerprint().map_err(mls)?;
        Ok(GroupSecurity {
            fingerprint_hex: hex::encode(fp)[..60].to_string(),
            events: c
                .key_events
                .iter()
                .map(|e| GroupKeyEvent {
                    kind: if e.kind == 1 { "left" } else { "removed" }.into(),
                    name: e.name.clone(),
                    at_ms: e.at_ms,
                    members: e.members,
                })
                .collect(),
        })
    }

    /// MR29: when we may go live here again (ms), if blocked now.
    pub fn live_blocked_until_ms(&self, community_id_hex: String) -> Option<u64> {
        let id = parse_cid(&community_id_hex).ok()?;
        let me = self.me();
        let now = now_ms();
        let st = self.inner.lock();
        st.mesh
            .moderation
            .live_blocked_at(&id, &me)
            .map(|at| at + omcp_core::moderation::LIVE_RESTART_BLOCK_MS)
            .filter(|until| *until > now || st.mesh.moderation.live_host_blocked(&id, &me, now))
    }
}

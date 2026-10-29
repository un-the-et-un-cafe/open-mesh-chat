//! Communities (PROTOCOL §20.1, minimal v0.2 cut) and Live inside a community.
//!
//! A community is an MLS group (OpenMLS, up to 32 members in this build).
//! Control traffic travels as pairwise E2EE app messages between contacts
//! (`application/omcp-community`, no chat row):
//!
//! - `Invite` (founder → contact), `Join` (KeyPackage back), `Welcome`
//!   (founder → new member) and `Commit` (founder → existing members). Only
//!   the founder adds members, so the founder is a contact of everyone.
//! - `Group`: an MLS application message of the community group (live
//!   start/end). MLS authenticates the sender; any member forwards it unchanged
//!   to its own contacts in the roster, so members who are not the host's
//!   contacts still hear about the live. Duplicates are dropped by hash.
//! - `Rekey`: a host-signed new stream key after "Remove viewer", forwarded the
//!   same way but never to the excluded PeerIDs.
//!
//! Team moderation (§23.9): a taken-down community is hidden, its lives can't
//! be started or joined, and its announcements are neither applied nor
//! forwarded (client side of `relay_allows`).

use std::collections::{BTreeMap, HashMap, HashSet};

use omcp_core::burn::AppContent;
use omcp_core::live::community::{community_stream_key, CommunityLive, Rekey};
use omcp_core::live::host::HostSession;
use omcp_core::live::LiveInvite;
use omcp_core::mls::group::{MlsMemberState, OmcpMlsMember};
use serde::{Deserialize, Serialize};

use crate::{invalid, now_ms, parse_id, FfiError, NodeState, OmcpNode};

pub const COMMUNITY_MIME: &str = "application/omcp-community";
const STATE_KEY: &[u8] = b"communities-v1";
const MAX_NAME: usize = 60;

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
}

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
    joining: HashMap<[u8; 32], (String, OmcpMlsMember)>,
    /// Founder: (community, contact) invites we sent.
    invited: HashSet<([u8; 32], [u8; 32])>,
    /// Pairwise messages to send once the node lock is released.
    pub outbox: Vec<([u8; 32], Vec<u8>)>,
    /// Gossip dedup (BLAKE2b of forwarded group / rekey messages).
    seen: HashSet<[u8; 32]>,
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
        Ok(())
    }

    fn removed(&self, id: &[u8; 32]) -> bool {
        self.mesh.moderation.is_removed(id)
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
        Ok(())
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
        Ok(())
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
                let (commit, welcome, roster, others, cname, founder) = {
                    let Some(c) = self.mesh.community.list.get_mut(&id) else {
                        return Ok(());
                    };
                    let kp = c.member.parse_key_package(&kp).map_err(mls)?;
                    let (commit, welcome) = c.member.add_members(&[kp]).map_err(mls)?;
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
                member.join_from_welcome(&welcome).map_err(mls)?;
                self.mesh.community.list.insert(
                    id,
                    Community {
                        name: name.chars().take(MAX_NAME).collect(),
                        founder,
                        roster: roster.into_iter().collect(),
                        member,
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
                c.member.process_protocol_message(&commit).map_err(mls)?;
                c.roster = roster.into_iter().collect();
                self.save_communities()?;
            }
            CMsg::Group { id, ct } => self.on_group(peer, id, ct)?,
            CMsg::Rekey { rekey } => self.on_rekey(peer, rekey)?,
        }
        Ok(())
    }

    fn on_group(&mut self, peer: [u8; 32], id: [u8; 32], ct: Vec<u8>) -> Result<(), FfiError> {
        let h = h32(&[&ct]);
        if !self.mesh.community.seen.insert(h) || self.removed(&id) {
            return Ok(()); // duplicate, or community taken down: not applied, not forwarded
        }
        let (plain, sender) = {
            let Some(c) = self.mesh.community.list.get_mut(&id) else {
                return Ok(());
            };
            let Ok((plain, _aad, leaf)) = c.member.decrypt_app_message_with_aad(&ct) else {
                return Ok(());
            };
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
        self.save_communities()?;
        let Ok(a) = CommunityLive::decode(&plain) else {
            return Ok(());
        };
        if a.community_id != id || !a.host_matches(&sender) {
            return Ok(());
        }
        if !self.mesh.moderation.live_allowed(&a.stream_id, Some(&id)) {
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

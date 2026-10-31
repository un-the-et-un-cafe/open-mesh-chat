//! Private invites from an open community into a private group
//! (PROTOCOL §20.7; TM13–TM17; MR33–MR39).
//!
//! The founder of a private group invites a member of an open community they
//! share, using the member's published `invite_pk` (profile field `0x10`).
//! Offer, reply and Welcome travel sealed in envelopes built like 1:1 chat
//! envelopes and are never written to community history, rosters, the
//! moderation log or anything the team sees. Delivery is mesh-only (v1.0,
//! MR39): there is no gateway path for unknown tags.

use std::collections::BTreeMap;

use omcp_core::community::ControlKind;
use omcp_core::invite::{
    self as inv, InviteKey, InviteKeyring, InviteOffer, InvitePk, InviteReply, InviteWelcome,
    ReceiverLimits, SenderLimits,
};
use omcp_core::mls::group::{MlsMemberState, OmcpMlsMember};
use omcp_core::EnvelopeV1;
use serde::{Deserialize, Serialize};

use crate::private_group::{
    cmsg_is_welcome, cmsg_join, cmsg_welcome_ids, MAX_PRIVATE_GROUP_MEMBERS,
};
use crate::{invalid, now_ms, parse_id, FfiError, NodeState, OmcpNode};

const STATE_KEY: &[u8] = b"private-invites-v1";

/// Shown to the inviter on every send (§20.7(8), TM17, MR39).
pub const INVITE_MESH_ONLY_NOTICE: &str =
    "Invites travel over the mesh only. It may not reach someone who is only reachable over the internet.";
pub const STATUS_PENDING: &str = "Not delivered yet";

fn ce(_: omcp_core::error::OmcpError) -> FfiError {
    invalid("private invite")
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SentOffer {
    group: [u8; 32],
    community: [u8; 32],
    invitee: [u8; 32],
    invitee_key_id: [u8; 8],
    reply_key: Vec<u8>,
    expires_s: u64,
    /// 0 pending, 1 accepted, 2 declined, 3 expired
    status: u8,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RecvOffer {
    community: [u8; 32],
    offer: Vec<u8>,
    /// Accepted: waiting for the Welcome (MLS member state).
    joining: Option<MlsMemberState>,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    keyring: Vec<u8>,
    sender: SenderLimits,
    receiver: ReceiverLimits,
    sent: BTreeMap<[u8; 16], SentOffer>,
    recv: BTreeMap<[u8; 16], RecvOffer>,
}

#[derive(Default)]
pub(crate) struct InviteState {
    pub keyring: InviteKeyring,
    pub sender: SenderLimits,
    pub receiver: ReceiverLimits,
    pub sent: BTreeMap<[u8; 16], SentOffer>,
    pub recv: BTreeMap<[u8; 16], RecvOffer>,
    /// Test hook: every invite envelope this node originated.
    #[cfg(test)]
    pub log: Vec<EnvelopeV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InviteCandidate {
    pub member_key_hex: String,
    pub contact_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InviteSendResult {
    pub offer_id_hex: String,
    /// Mesh-only notice the app MUST show when sending (MR39).
    pub notice: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SentInviteInfo {
    pub offer_id_hex: String,
    pub group_id_hex: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InviteRequest {
    pub offer_id_hex: String,
    pub community_id_hex: String,
    pub community_name: String,
    pub inviter_key_hex: String,
    pub inviter_name: Option<String>,
    pub invitation_text: String,
    pub group_name: String,
    pub accepted: bool,
}

fn status_str(s: u8) -> &'static str {
    match s {
        1 => "Accepted",
        2 => "Declined",
        3 => "Expired",
        _ => STATUS_PENDING,
    }
}

fn oid(h: &str) -> Result<[u8; 16], FfiError> {
    let b = hex::decode(h).map_err(|_| invalid("offer id"))?;
    b.try_into().map_err(|_| invalid("offer id"))
}

impl NodeState {
    pub(crate) fn load_invites(&mut self) -> Result<(), FfiError> {
        let s: Saved = self.db.get_meta(STATE_KEY)?.unwrap_or_default();
        let st = &mut self.mesh.invites;
        st.keyring = InviteKeyring::from_bytes(&s.keyring).unwrap_or_default();
        st.sender = s.sender;
        st.receiver = s.receiver;
        st.sent = s.sent;
        st.recv = s.recv;
        let now = now_ms() / 1000;
        st.keyring.rotate(now);
        self.invite_tick(now);
        self.refresh_invite_keys();
        Ok(())
    }

    fn save_invites(&self) -> Result<(), FfiError> {
        let st = &self.mesh.invites;
        let s = Saved {
            keyring: st.keyring.to_bytes(),
            sender: st.sender.clone(),
            receiver: st.receiver.clone(),
            sent: st.sent.clone(),
            recv: st.recv.clone(),
        };
        self.db.put_meta(STATE_KEY, &s)?;
        Ok(())
    }

    fn invite_tick(&mut self, now_s: u64) {
        let st = &mut self.mesh.invites;
        st.sender.tick(now_s);
        for o in st.sent.values_mut() {
            if o.status == 0 && now_s >= o.expires_s {
                o.status = 3;
            }
        }
        st.recv.retain(|_, r| {
            InviteOffer::decode(&r.offer).is_ok_and(|o| (o.expires as u64) > now_s)
                || r.joining.is_some()
        });
    }

    /// Keys the relay engine tries on unknown tags (≤ 16).
    fn invite_keys(&self) -> (Vec<InviteKey>, Vec<([u8; 16], InviteKey)>) {
        let st = &self.mesh.invites;
        let mine: Vec<InviteKey> = st
            .keyring
            .candidate_keys()
            .into_iter()
            .map(|(_, k)| k.clone())
            .collect();
        let replies: Vec<([u8; 16], InviteKey)> = st
            .sent
            .iter()
            .filter(|(_, o)| o.status == 0)
            .filter_map(|(id, o)| InviteKey::from_bytes(&o.reply_key).ok().map(|k| (*id, k)))
            .collect();
        (mine, replies)
    }

    fn refresh_invite_keys(&mut self) {
        let (mut k, r) = self.invite_keys();
        k.extend(r.into_iter().map(|(_, k)| k));
        self.mesh.engine.set_invite_keys(k);
    }

    fn send_invite_env(&mut self, env: EnvelopeV1) -> Result<(), FfiError> {
        #[cfg(test)]
        self.mesh.invites.log.push(env.clone());
        let ev = self.mesh.engine.submit_own(env, now_ms())?;
        self.route(ev)?;
        Ok(())
    }

    fn seal_to(
        &mut self,
        to: &InvitePk,
        info: &[u8],
        msg: &[u8],
        bucket: usize,
    ) -> Result<EnvelopeV1, FfiError> {
        let ttl = self.mesh.engine.new_ttl();
        let mut e = [0u8; 8];
        let _ = getrandom::getrandom(&mut e);
        inv::build_envelope(
            to,
            info,
            msg,
            bucket,
            crate::mesh::rand16(),
            now_ms(),
            ttl,
            u64::from(e[0] % 8) + 1,
        )
        .map_err(ce)
    }

    /// A delivered envelope: is it a private invite for us? Never stored in
    /// community history and never shown to the team (§20.7(7)).
    pub(crate) fn on_invite_envelope(&mut self, env: &EnvelopeV1) -> Result<bool, FfiError> {
        let (mine, replies) = self.invite_keys();
        if mine.is_empty() && replies.is_empty() {
            return Ok(false);
        }
        let now = now_ms() / 1000;
        let refs: Vec<&InviteKey> = mine.iter().collect();
        if let Some((i, pt)) = inv::try_open_envelope(env, &refs, |k| inv::invite_info(&k.key_id)) {
            let kid = mine[i].key_id;
            let community = self
                .mesh
                .invites
                .keyring
                .candidate_keys()
                .into_iter()
                .find(|(_, k)| k.key_id == kid)
                .map(|(c, _)| *c);
            match (pt.first().copied(), community) {
                (Some(inv::TYPE_OFFER), Some(c)) => self.on_offer(c, &pt, now)?,
                (Some(inv::TYPE_WELCOME), _) => self.on_welcome(&pt)?,
                _ => {}
            }
            return Ok(true);
        }
        let rrefs: Vec<&InviteKey> = replies.iter().map(|(_, k)| k).collect();
        if let Some((i, pt)) = inv::try_open_envelope(env, &rrefs, |_| inv::INFO_REPLY.to_vec()) {
            self.on_reply(replies[i].0, &pt, now)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn on_offer(&mut self, community: [u8; 32], pt: &[u8], now: u64) -> Result<(), FfiError> {
        if !self.mesh.invites.keyring.is_enabled(&community) {
            return Ok(()); // setting off: dropped unread
        }
        let Ok(o) = InviteOffer::decode(pt) else {
            return Ok(());
        };
        if (o.expires as u64) <= now || self.mesh.invites.recv.contains_key(&o.offer_id) {
            return Ok(());
        }
        // The inviter must be a member of that community.
        let member = self
            .mesh
            .open
            .get(&community)
            .is_some_and(|c| c.is_member(&o.inviter_pseudonym_pk));
        if !member
            || !self
                .mesh
                .invites
                .receiver
                .admit(&o.inviter_pseudonym_pk, now)
        {
            return Ok(()); // dropped silently, no reply
        }
        self.mesh.invites.recv.insert(
            o.offer_id,
            RecvOffer {
                community,
                offer: pt.to_vec(),
                joining: None,
            },
        );
        self.save_invites()
    }

    fn on_reply(&mut self, offer_id: [u8; 16], pt: &[u8], now: u64) -> Result<(), FfiError> {
        let Ok(r) = InviteReply::decode(pt) else {
            return Ok(());
        };
        let Some(o) = self.mesh.invites.sent.get(&offer_id).cloned() else {
            return Ok(());
        };
        if r.offer_id != offer_id || o.status != 0 {
            return Ok(());
        }
        if r.decision == inv::DECISION_DECLINE {
            self.mesh.invites.sent.get_mut(&offer_id).unwrap().status = 2;
            self.mesh
                .invites
                .sender
                .record_closed(o.invitee_key_id, true, now);
            self.save_invites()?;
            self.refresh_invite_keys();
            return Ok(());
        }
        let Some(wpk) = r.welcome_pk.clone() else {
            return Ok(());
        };
        // The KeyPackage identity is the invitee's peer id.
        let peer: [u8; 32] = {
            let Some(c) = self.mesh.community.list.get(&o.group) else {
                return Ok(());
            };
            match c.member.key_package_identity(&r.key_package) {
                Ok(id) if id.len() == 32 => id.try_into().unwrap(),
                _ => return Ok(()),
            }
        };
        if peer != o.invitee {
            return Ok(());
        }
        self.mesh.community.invited.insert((o.group, peer));
        self.on_community_msg(peer, &cmsg_join(o.group, r.key_package))?;
        // Pull the Welcome back out of the pairwise outbox: it goes sealed.
        let out = std::mem::take(&mut self.mesh.community.outbox);
        let mut welcome = None;
        for (to, b) in out {
            if to == peer && cmsg_is_welcome(&b) {
                welcome = Some(b);
            } else {
                self.mesh.community.outbox.push((to, b));
            }
        }
        self.mesh.community.invited.remove(&(o.group, peer));
        let Some(w) = welcome else {
            return Ok(()); // full (32) or refused (e.g. a team identity)
        };
        let msg = InviteWelcome {
            offer_id,
            welcome: w,
        }
        .encode();
        let env = self.seal_to(&wpk, &inv::invite_info(&wpk.key_id), &msg, 0)?;
        self.mesh.invites.sent.get_mut(&offer_id).unwrap().status = 1;
        self.mesh
            .invites
            .sender
            .record_closed(o.invitee_key_id, false, now);
        self.save_invites()?;
        self.refresh_invite_keys();
        self.send_invite_env(env)
    }

    fn on_welcome(&mut self, pt: &[u8]) -> Result<(), FfiError> {
        let Ok(w) = InviteWelcome::decode(pt) else {
            return Ok(());
        };
        let Some(r) = self.mesh.invites.recv.get(&w.offer_id).cloned() else {
            return Ok(());
        };
        let Some(state) = r.joining else {
            return Ok(());
        };
        let Some((id, founder, name)) = cmsg_welcome_ids(&w.welcome) else {
            return Ok(());
        };
        let Ok(member) = OmcpMlsMember::restore(&state) else {
            return Ok(());
        };
        self.mesh.community.joining.insert(id, (name, member));
        self.on_community_msg(founder, &w.welcome)?;
        self.mesh.invites.recv.remove(&w.offer_id);
        self.save_invites()
    }
}

impl OmcpNode {
    /// Rotate invite keys (30 days) and republish the profile of every
    /// community whose published `invite_pk` changed (§20.7(1)).
    pub(crate) fn invite_maintain(&self, now_s: u64) -> Result<Vec<[u8; 32]>, FfiError> {
        let changed: Vec<([u8; 32], Vec<u8>)> = {
            let mut st = self.inner.lock();
            let comms: Vec<[u8; 32]> = st.mesh.open.communities.keys().copied().collect();
            let ring = &mut st.mesh.invites.keyring;
            let before: Vec<([u8; 32], Option<[u8; 8]>)> = comms
                .into_iter()
                .map(|c| (c, ring.published(&c).map(|p| p.key_id)))
                .collect();
            ring.rotate(now_s);
            let mut out = Vec::new();
            for (c, kid) in before {
                if let Some(p) = ring.published(&c) {
                    if Some(p.key_id) != kid {
                        out.push((c, p.encode()));
                    }
                }
            }
            st.invite_tick(now_s);
            if !out.is_empty() {
                st.save_invites()?;
                st.refresh_invite_keys();
            }
            out
        };
        let mut ids = Vec::new();
        for (c, pk) in changed {
            self.control(
                c,
                ControlKind::Profile {
                    invite_pk: Some(pk),
                },
            )?;
            ids.push(c);
        }
        Ok(ids)
    }
}

#[uniffi::export]
impl OmcpNode {
    /// "Invites from people who aren't contacts" for this community (TM15).
    /// Off by default on a fresh install and in every newly joined community.
    pub fn open_community_invites_enabled(&self, community_id_hex: String) -> bool {
        parse_id(&community_id_hex)
            .map(|id| self.inner.lock().mesh.invites.keyring.is_enabled(&id))
            .unwrap_or(false)
    }

    /// Turn the setting on (publishes `invite_pk` in our profile) or off
    /// (removes it; the private key is erased after the 7-day overlap).
    pub fn open_community_set_invites_enabled(
        &self,
        community_id_hex: String,
        on: bool,
    ) -> Result<(), FfiError> {
        let id = parse_id(&community_id_hex)?;
        let pk = {
            let mut st = self.inner.lock();
            if st.mesh.open.get(&id).is_none() {
                return Err(invalid("unknown community"));
            }
            st.mesh.invites.keyring.set_enabled(id, on, now_ms() / 1000);
            st.save_invites()?;
            st.refresh_invite_keys();
            st.mesh.invites.keyring.published(&id).map(|p| p.encode())
        };
        self.control(
            id,
            ControlKind::Profile {
                invite_pk: if on { pk } else { None },
            },
        )
    }

    /// Community members who accept invites (have `invite_pk`), minus us.
    pub fn private_invite_candidates(&self, community_id_hex: String) -> Vec<InviteCandidate> {
        let Ok(id) = parse_id(&community_id_hex) else {
            return vec![];
        };
        let Ok((_, me)) = self.key_and_me() else {
            return vec![];
        };
        let st = self.inner.lock();
        let Some(c) = st.mesh.open.get(&id) else {
            return vec![];
        };
        c.invite_pks
            .keys()
            .filter(|k| **k != me && c.is_member(k))
            .map(|k| InviteCandidate {
                member_key_hex: hex::encode(k),
                contact_name: st
                    .contacts
                    .get(&omcp_core::device::peer_id_from_identity(k))
                    .map(|c| c.name.clone()),
            })
            .collect()
    }

    /// Group founder: invite a community member into a private group.
    pub fn private_invite_send(
        &self,
        group_id_hex: String,
        community_id_hex: String,
        member_key_hex: String,
        text: String,
    ) -> Result<InviteSendResult, FfiError> {
        let gid = parse_id(&group_id_hex)?;
        let cid = parse_id(&community_id_hex)?;
        let mk = parse_id(&member_key_hex)?;
        let (k, me) = self.key_and_me()?;
        let now = now_ms() / 1000;
        let mut st = self.inner.lock();
        st.invite_tick(now);
        let invitee = omcp_core::device::peer_id_from_identity(&mk);
        let gname = {
            if st.removed(&gid) {
                return Err(invalid("group removed"));
            }
            let g = st
                .mesh
                .community
                .list
                .get(&gid)
                .ok_or_else(|| invalid("unknown group"))?;
            if g.founder != self.inner.peer_id {
                return Err(invalid("only the founder invites"));
            }
            if g.roster.len() >= MAX_PRIVATE_GROUP_MEMBERS {
                return Err(invalid("private groups hold up to 32 members"));
            }
            if g.roster.contains_key(&invitee) {
                return Err(invalid("already a member"));
            }
            g.name.clone()
        };
        let pk = {
            let c = st
                .mesh
                .open
                .get(&cid)
                .ok_or_else(|| invalid("unknown community"))?;
            if !c.is_member(&me) || !c.is_member(&mk) {
                return Err(invalid("not a community member"));
            }
            let b = c
                .invite_pks
                .get(&mk)
                .ok_or_else(|| invalid("member does not accept invites"))?;
            InvitePk::decode(b).map_err(ce)?
        };
        if (pk.not_after as u64) <= now {
            return Err(invalid("invite key expired"));
        }
        st.mesh
            .invites
            .sender
            .check(&pk.key_id, now)
            .map_err(|_| invalid("invite limit reached"))?;
        let text: String = text.chars().take(280).collect();
        let text = if text.len() > inv::MAX_INVITATION_TEXT {
            String::new()
        } else {
            text
        };
        let gname: String = gname.chars().take(32).collect();
        let reply = InviteKey::generate(now, inv::OFFER_TTL_S);
        let offer =
            InviteOffer::new_signed(&k, reply.public(), &text, &gname, None, now).map_err(ce)?;
        let env = st.seal_to(&pk, &inv::invite_info(&pk.key_id), &offer.encode(), 0)?;
        st.mesh
            .invites
            .sender
            .record_sent(pk.key_id, offer.expires as u64, now);
        st.mesh.invites.sent.insert(
            offer.offer_id,
            SentOffer {
                group: gid,
                community: cid,
                invitee,
                invitee_key_id: pk.key_id,
                reply_key: reply.to_bytes(),
                expires_s: offer.expires as u64,
                status: 0,
            },
        );
        st.save_invites()?;
        st.refresh_invite_keys();
        st.send_invite_env(env)?;
        Ok(InviteSendResult {
            offer_id_hex: hex::encode(offer.offer_id),
            notice: INVITE_MESH_ONLY_NOTICE.into(),
            status: STATUS_PENDING.into(),
        })
    }

    pub fn private_invites_sent(&self) -> Vec<SentInviteInfo> {
        let mut st = self.inner.lock();
        st.invite_tick(now_ms() / 1000);
        st.mesh
            .invites
            .sent
            .iter()
            .map(|(id, o)| SentInviteInfo {
                offer_id_hex: hex::encode(id),
                group_id_hex: hex::encode(o.group),
                status: status_str(o.status).into(),
            })
            .collect()
    }

    /// Requests: "<name> from <community> invites you to a private group" (TM14).
    pub fn private_invite_requests(&self) -> Vec<InviteRequest> {
        let mut st = self.inner.lock();
        st.invite_tick(now_ms() / 1000);
        st.mesh
            .invites
            .recv
            .iter()
            .filter_map(|(id, r)| {
                let o = InviteOffer::decode(&r.offer).ok()?;
                Some(InviteRequest {
                    offer_id_hex: hex::encode(id),
                    community_id_hex: hex::encode(r.community),
                    community_name: st
                        .mesh
                        .open
                        .get(&r.community)
                        .map(|c| c.name.clone())
                        .unwrap_or_default(),
                    inviter_key_hex: hex::encode(o.inviter_pseudonym_pk),
                    inviter_name: st
                        .contacts
                        .get(&omcp_core::device::peer_id_from_identity(
                            &o.inviter_pseudonym_pk,
                        ))
                        .map(|c| c.name.clone()),
                    invitation_text: o.invitation_text,
                    group_name: o.group_name,
                    accepted: r.joining.is_some(),
                })
            })
            .collect()
    }

    /// Accept (sends a fresh KeyPackage) or decline (same padded size).
    pub fn private_invite_answer(
        &self,
        offer_id_hex: String,
        accept: bool,
    ) -> Result<(), FfiError> {
        let id = oid(&offer_id_hex)?;
        let mut st = self.inner.lock();
        let r = st
            .mesh
            .invites
            .recv
            .get(&id)
            .cloned()
            .ok_or_else(|| invalid("no invite"))?;
        if r.joining.is_some() {
            return Err(invalid("already answered"));
        }
        let o = InviteOffer::decode(&r.offer).map_err(ce)?;
        let member = OmcpMlsMember::generate(self.inner.peer_id.to_vec()).map_err(ce)?;
        let kp = member.key_package_bytes().map_err(ce)?;
        let wpk = st.mesh.invites.keyring.published(&r.community);
        let accept_msg = InviteReply {
            offer_id: id,
            decision: inv::DECISION_ACCEPT,
            key_package: kp,
            welcome_pk: wpk.clone(),
        }
        .encode();
        let bucket = InviteReply::bucket(accept_msg.len());
        let msg = if accept && wpk.is_some() {
            accept_msg
        } else {
            InviteReply {
                offer_id: id,
                decision: inv::DECISION_DECLINE,
                key_package: vec![],
                welcome_pk: None,
            }
            .encode()
        };
        let env = st.seal_to(&o.reply_pk, inv::INFO_REPLY, &msg, bucket)?;
        if accept && wpk.is_some() {
            let s = member.export_state().map_err(ce)?;
            st.mesh.invites.recv.get_mut(&id).unwrap().joining = Some(s);
        } else {
            st.mesh.invites.recv.remove(&id);
        }
        st.save_invites()?;
        st.send_invite_env(env)
    }

    /// Ignore silently: nothing is sent (community history unchanged, MR36).
    pub fn private_invite_ignore(&self, offer_id_hex: String) -> Result<(), FfiError> {
        let id = oid(&offer_id_hex)?;
        let mut st = self.inner.lock();
        st.mesh.invites.recv.remove(&id);
        st.save_invites()
    }

    /// Block the inviter's pseudonym (their later offers are dropped silently).
    pub fn private_invite_block(&self, offer_id_hex: String) -> Result<(), FfiError> {
        let id = oid(&offer_id_hex)?;
        let mut st = self.inner.lock();
        let r = st
            .mesh
            .invites
            .recv
            .remove(&id)
            .ok_or_else(|| invalid("no invite"))?;
        let o = InviteOffer::decode(&r.offer).map_err(ce)?;
        st.mesh.invites.receiver.block(o.inviter_pseudonym_pk);
        st.save_invites()
    }

    /// Report evidence: only the signed `InviteOffer` (MR38). Nothing is sent
    /// automatically; the app hands these bytes to the report flow.
    pub fn private_invite_report(&self, offer_id_hex: String) -> Result<Vec<u8>, FfiError> {
        let id = oid(&offer_id_hex)?;
        let st = self.inner.lock();
        let r = st
            .mesh
            .invites
            .recv
            .get(&id)
            .ok_or_else(|| invalid("no invite"))?;
        let o = InviteOffer::decode(&r.offer).map_err(ce)?;
        Ok(inv::offer_report(&o))
    }

    pub fn private_invite_mesh_only_notice(&self) -> String {
        INVITE_MESH_ONLY_NOTICE.into()
    }
}

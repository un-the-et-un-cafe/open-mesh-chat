//! Multi-hop store-and-forward mesh (PROTOCOL §8.3, §8.4, §22) on top of the
//! per-link BLE sessions — **v0.2 beta, experimental**.
//!
//! The relay logic lives in [`omcp_core::sync::relay::RelayEngine`]; this
//! module is the glue: end-to-end sealing for envelopes that travel through
//! other phones, the encrypted carry store, storage reporting, and per-link
//! frame queues.
//!
//! # Relay (apps/REQUIREMENTS.md RL1–RL4)
//!
//! The relay is always on: there is no setting to turn it off (RL1). Its
//! storage cap is automatic, 500 MB, lowered when the apps report less than
//! 10 % free storage through [`OmcpNode::mesh_set_free_storage`] (RL2).
//! Envelopes are deleted on a delivery ack or at outer expiry, max 7 d (RL3).
//! The apps show only [`MeshStats::storage_used_bytes`] ("Storage used for
//! relaying: X MB", RL4).
//!
//! # Link PDU
//!
//! `0x03 ‖ frame` — one link-AEAD fragment (same codec and `LinkSession` as
//! `0x02`) of a *mesh frame* (`HELLO`/`DIGEST_REQ` beacons, want-digests,
//! opaque envelopes; see the relay module). Mesh frames therefore only ever
//! travel inside an authenticated, encrypted link to a contact.
//!
//! # End-to-end keys: 2-member MLS (§22.7(5), §22.8(1))
//!
//! Every 1:1 chat is a 2-member MLS group, created the first time the two
//! contacts have a direct link (link PDU `0x04` `MLS_SETUP`, §21.9, see
//! [`PDU_MLS_SETUP`]):
//! the contact with the higher id sends a KeyPackage, the lower id creates
//! the group, adds it and answers with the Welcome, the joiner confirms
//! with `JOINED`. Existing contacts migrate the same way on their next
//! direct link. Until both sides have the group, messages wait in the local
//! queue (direct only) and are not mesh-relayed.
//!
//! ```text
//! tag_secret = MLS-Exporter("OMCP-tag", tag_epoch_be, 32)
//! chat tag   = mailbox_tag(tag_secret, epoch)             // TLV 0x54
//! sub-tag    = subtag(tag_secret, recipient_leaf, epoch)  // TLV 0x56
//! ciphertext = MLS application message (PrivateMessage), MLS AAD =
//!              "OMCP-mesh-v1" ‖ version ‖ suite ‖ envelope_id ‖ created_ms ‖ flags ‖ header_aad
//! ```
//!
//! Envelopes are bound to the MLS epoch, not to the device key (§22.8(5)):
//! rotating the device key does not touch the group. Relays see only tags,
//! padded sizes (MLS padding) and `outer_expiry` (TLV `0x58`).
//!
//! # Delivery acks (§22.8(3))
//!
//! The recipient answers every mesh message with `DELIVERY_ACK`:
//! `msg_hash` = BLAKE2b-256(ciphertext)[0..16] in the clear for relays, plus an
//! envelope to the sender's sub-tag whose MLS application message is
//! `0x45 ‖ msg_hash`. The sender only trusts the MLS-authenticated copy; the
//! message then moves Queued → Sent (handed to a relay) → Delivered.

use std::collections::{BTreeSet, HashMap, VecDeque};

use omcp_core::ble::fragment::{fragment_and_seal, random_frag_id};
use omcp_core::ble::message::CONTENT_TYPE_INNER_V1;
use omcp_core::burn::{self as core_burn, BurnHeader};
use omcp_core::framing::flags;
use omcp_core::mls::group::{MlsMemberState, OmcpMlsMember};
use omcp_core::store::{self as st, CarryRec};
use omcp_core::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use omcp_core::sync::budget::{RelayClass, DEFAULT_EXPIRY_MS, MAX_EXPIRY_MS, RELAY_CAP_BYTES};
use omcp_core::sync::relay::{
    effective_expiry, msg_hash, put_expires_in_aad, CarryItem, RelayConfig, RelayEngine,
    RelayEvent, FRAME_DELIVERY_ACK, MSG_HASH_LEN,
};
use omcp_core::sync::tag::{
    mailbox_tag, put_subtags_in_aad, put_tag_in_aad, subtag, tag_epoch, MailboxTag,
};
use omcp_core::EnvelopeV1;

use crate::{
    invalid, now_ms, random_id16, Contact, Established, FfiError, LinkOutput, MeshLink,
    MessageStatus, NodeInner, NodeState, OmcpNode, Queued, LINK_FRAGMENT_PAYLOAD,
};

/// Link PDU `0x03` `MESH_FRAME` (§21.9).
pub(crate) const PDU_MESH: u8 = 0x03;
/// Link PDU `0x04` `MLS_SETUP` (§21.9 registry, §22.8(1)): MLS group setup
/// for a 1:1 chat. Wire: `0x04 ‖ link-AEAD fragment` of the setup frame
/// `type ‖ body`, with `type` one of [`SETUP_KEY_PACKAGE`],
/// [`SETUP_WELCOME`], [`SETUP_JOINED`].
pub(crate) const PDU_MLS_SETUP: u8 = 0x04;
/// `MLS_SETUP` frame `KEY_PACKAGE` (joiner → creator: MLS KeyPackage).
pub(crate) const SETUP_KEY_PACKAGE: u8 = 0x01;
/// `MLS_SETUP` frame `WELCOME` (creator → joiner: MLS Welcome).
pub(crate) const SETUP_WELCOME: u8 = 0x02;
/// `MLS_SETUP` frame `JOINED` (joiner → creator: empty body).
pub(crate) const SETUP_JOINED: u8 = 0x03;
/// Mesh frames sealed onto one link per call (keeps BLE queues short).
pub(crate) const MESH_FRAMES_PER_FLUSH: usize = 32;
/// Simultaneous mesh links the apps should keep (central + peripheral roles).
pub const MAX_MESH_LINKS: u32 = 4;
const MLS_KEY: &[u8] = b"mesh-mls:";
const AAD_PREFIX: &[u8] = b"OMCP-mesh-v1";

/// Relay counters. Settings shows only `storage_used_bytes` (RL4); the
/// rest is for diagnostics and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MeshStats {
    /// Envelopes carried for others.
    pub carried: u32,
    /// Bytes carried for others.
    pub carried_bytes: u64,
    pub own_pending: u32,
    pub neighbours: u32,
    /// RL4 "Storage used for relaying": the whole carry store (own chats
    /// included), i.e. what the automatic cap limits.
    pub storage_used_bytes: u64,
    /// RL2 automatic cap currently in force (500 MB unless storage is low).
    pub storage_cap_bytes: u64,
}

/// A message status change caused by the mesh (handed to a relay, acked).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MessageStatusUpdate {
    pub id_hex: String,
    pub contact_id: String,
    pub status: MessageStatus,
}

/// RL2 automatic relay storage cap (500 MB) before any low-storage
/// reduction. Informational: it is not a setting.
#[uniffi::export]
pub fn mesh_relay_cap_bytes() -> u64 {
    RELAY_CAP_BYTES
}

/// Recommended simultaneous mesh links per device.
#[uniffi::export]
pub fn mesh_max_links() -> u32 {
    MAX_MESH_LINKS
}

/// One contact's 1:1 MLS group state.
pub(crate) struct ContactMls {
    member: OmcpMlsMember,
    /// Both sides have the group (creator: `JOINED` received).
    ready: bool,
}

/// A mesh envelope for us: `(wanted tag, envelope, acked msg_hash if an ack)`.
pub(crate) type MeshDelivery = (MailboxTag, EnvelopeV1, Option<[u8; MSG_HASH_LEN]>);
type PendingCopy = ([u8; 16], u64, zeroize::Zeroizing<Vec<u8>>, bool);

/// One frame waiting for a link: link PDU kind, plain frame, and our own
/// message id it hands over (status → Sent once it is actually sealed).
pub(crate) struct OutFrame {
    pub(crate) pdu: u8,
    pub(crate) frame: Vec<u8>,
    handed: Option<[u8; 16]>,
}

impl OutFrame {
    pub(crate) fn new(pdu: u8, frame: Vec<u8>) -> Self {
        Self {
            pdu,
            frame,
            handed: None,
        }
    }
}

/// Persisted as `(ready, member state)` (sealed meta row).
type ContactMlsRec = (bool, MlsMemberState);

pub(crate) struct MeshState {
    pub engine: RelayEngine,
    /// Wanted sub-tag → contact.
    pub tags: HashMap<MailboxTag, [u8; 32]>,
    /// (epoch, MLS fingerprint) the tags were computed for.
    pub tags_for: Option<(u64, [u8; 32])>,
    /// Frames waiting to be sealed onto each link.
    pub out: HashMap<u64, VecDeque<OutFrame>>,
    /// 1:1 MLS groups by contact.
    pub mls: HashMap<[u8; 32], ContactMls>,
    /// Status changes for the apps (`mesh_take_status_updates`).
    pub status: Vec<MessageStatusUpdate>,
    /// Hopla News: interests, fetch times, story cache.
    pub news: crate::news::NewsState,
    /// Team moderation (§23.9): shipped policy and applied actions.
    pub team: omcp_core::moderation::TeamPolicy,
    pub moderation: omcp_core::moderation::ModerationState,
    /// Live audio (never persisted).
    pub live: crate::live::LiveState,
}

impl MeshState {
    pub(crate) fn new(now: u64) -> Self {
        let seed = u64::from_le_bytes(rand16()[..8].try_into().expect("8"));
        Self {
            engine: RelayEngine::new(RelayConfig::default(), now, seed),
            tags: HashMap::new(),
            tags_for: None,
            out: HashMap::new(),
            mls: HashMap::new(),
            status: Vec::new(),
            news: Default::default(),
            team: crate::moderation::shipped_policy(),
            moderation: Default::default(),
            live: Default::default(),
        }
    }

    /// Links (other than `except`) with frames waiting.
    pub(crate) fn pending_links(&self, except: Option<u64>) -> Vec<u64> {
        let mut v: Vec<u64> = self
            .out
            .iter()
            .filter(|(l, q)| Some(**l) != except && !q.is_empty())
            .map(|(l, _)| *l)
            .collect();
        v.sort_unstable();
        v
    }

    /// MLS group with this contact is up on both sides.
    pub(crate) fn mls_ready(&self, peer: &[u8; 32]) -> bool {
        self.mls.get(peer).is_some_and(|m| m.ready)
    }
}

fn rand16() -> [u8; 16] {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    b
}

/// Our id as this contact knows us (legacy contacts know the legacy id).
fn my_id(inner: &NodeInner, id: &[u8; 32], c: &Contact) -> [u8; 32] {
    if c.legacy(id) {
        inner.legacy_id
    } else {
        inner.peer_id
    }
}

fn aad(env: &EnvelopeV1) -> Vec<u8> {
    let mut a = AAD_PREFIX.to_vec();
    a.extend_from_slice(&env.version.to_be_bytes());
    a.extend_from_slice(&env.suite_id.to_be_bytes());
    a.extend_from_slice(&env.envelope_id);
    a.extend_from_slice(&env.created_ms.to_be_bytes());
    a.extend_from_slice(&env.flags.to_be_bytes());
    a.extend_from_slice(&env.header_aad);
    a
}

fn mls_err(_: omcp_core::error::OmcpError) -> FfiError {
    invalid("mls")
}

/// The other member's leaf in a 2-member group.
fn other_leaf(m: &OmcpMlsMember) -> Result<u32, FfiError> {
    Ok(1 - m.own_leaf_index().map_err(mls_err)?.min(1))
}

/// Seal `plain` for contact `peer` as an opaque mesh envelope (MLS
/// application message), addressed to the other leaf's sub-tag.
#[allow(clippy::too_many_arguments)]
fn seal_for(
    st: &mut NodeState,
    peer: &[u8; 32],
    envelope_id: [u8; 16],
    created_ms: u64,
    expires_at: u64,
    ttl_hops: u8,
    plain: &[u8],
) -> Result<EnvelopeV1, FfiError> {
    let m = st
        .mesh
        .mls
        .get_mut(peer)
        .filter(|m| m.ready)
        .ok_or_else(|| invalid("no MLS group"))?;
    let epoch = tag_epoch(created_ms);
    let ts = m.member.tag_exporter(epoch).map_err(mls_err)?;
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &mailbox_tag(&ts, epoch)?);
    put_subtags_in_aad(
        &mut header_aad,
        &[subtag(&ts, other_leaf(&m.member)?, epoch)?],
    )?;
    put_expires_in_aad(&mut header_aad, expires_at);
    let mut env = EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id,
        flags: flags::RELAY_OK | flags::STORE_OK,
        ttl_hops,
        created_ms,
        header_aad,
        ciphertext: Vec::new(),
    };
    env.ciphertext = m
        .member
        .encrypt_app_message_with_aad(plain, &aad(&env))
        .map_err(mls_err)?;
    st.save_mls(peer)?;
    Ok(env)
}

/// Open a mesh envelope from contact `peer` (sender = the other leaf).
fn open_from(
    st: &mut NodeState,
    peer: &[u8; 32],
    env: &EnvelopeV1,
) -> Result<zeroize::Zeroizing<Vec<u8>>, FfiError> {
    let m = st
        .mesh
        .mls
        .get_mut(peer)
        .ok_or_else(|| invalid("no MLS group"))?;
    let (plain, got_aad, leaf) = m
        .member
        .decrypt_app_message_with_aad(&env.ciphertext)
        .map_err(mls_err)?;
    let plain = zeroize::Zeroizing::new(plain);
    st.save_mls(peer)?;
    let m = &st.mesh.mls[peer];
    if got_aad != aad(env) || leaf != other_leaf(&m.member)? {
        return Err(invalid("mesh envelope binding"));
    }
    Ok(plain)
}

impl NodeState {
    /// Load settings, MLS groups, carry store and dedup set (node start).
    pub(crate) fn load_mesh(&mut self, now: u64) -> Result<(), FfiError> {
        // Rows from the removed relay switch / cap setting ("mesh-settings")
        // are ignored: the relay is always on with an automatic cap (RL1, RL2).
        let mut mesh = MeshState::new(now);
        for id in self.contacts.keys() {
            let Some(r) = self.db.get_meta::<ContactMlsRec>(&mls_key(id))? else {
                continue;
            };
            if let Ok(member) = OmcpMlsMember::restore(&r.1) {
                mesh.mls.insert(*id, ContactMls { member, ready: r.0 });
            }
        }
        for r in self.db.carry_items()? {
            match EnvelopeV1::decode(&r.envelope) {
                Ok(envelope) if r.expires_at > now => mesh.engine.restore(
                    CarryItem {
                        envelope,
                        expires_at: r.expires_at,
                        hop_count: r.hop_count,
                        own: r.own,
                        class: carry_class(&r),
                        ack_of: r.ack_of,
                    },
                    now,
                ),
                _ => self.db.delete_carry(&r.id)?,
            }
        }
        for s in self.db.mesh_seen(now)? {
            mesh.engine.mark_seen(s.id, s.until_ms);
        }
        self.mesh = mesh;
        self.load_moderation()?;
        self.load_news(now)
    }

    pub(crate) fn save_mls(&self, peer: &[u8; 32]) -> Result<(), FfiError> {
        if let Some(m) = self.mesh.mls.get(peer) {
            let rec: ContactMlsRec = (m.ready, m.member.export_state().map_err(mls_err)?);
            self.db.put_meta(&mls_key(peer), &rec)?;
        }
        Ok(())
    }

    /// RL2: free / total device storage from the app; under 10 % free the
    /// automatic cap is lowered and the carry store evicts in RL2 order.
    pub(crate) fn mesh_set_free_storage(&mut self, free: u64, total: u64) -> Result<(), FfiError> {
        let ev = self.mesh.engine.set_free_storage(free, total);
        self.route(ev)?;
        Ok(())
    }

    pub(crate) fn mesh_stats(&self) -> MeshStats {
        let s = self.mesh.engine.stats();
        MeshStats {
            carried: s.carried,
            carried_bytes: s.carried_bytes,
            own_pending: s.own_pending,
            neighbours: s.neighbours,
            storage_used_bytes: s.storage_used_bytes,
            storage_cap_bytes: s.storage_cap_bytes,
        }
    }

    pub(crate) fn set_status(&mut self, id: &[u8; 16], status: u8) -> Result<(), FfiError> {
        let Some(m) = self.db.message(id)? else {
            return Ok(());
        };
        // Never go backwards (Delivered > Sent > Queued).
        let rank = |s: u8| match s {
            st::STATUS_QUEUED => 0,
            st::STATUS_SENT => 1,
            st::STATUS_DELIVERED => 2,
            _ => 3,
        };
        if !m.mine || rank(status) <= rank(m.status) {
            return Ok(());
        }
        self.update_message(id, |m| m.status = status)?;
        self.mesh.status.push(MessageStatusUpdate {
            id_hex: hex::encode(id),
            contact_id: hex::encode(m.peer),
            status: if status == st::STATUS_DELIVERED {
                MessageStatus::Delivered
            } else {
                MessageStatus::Sent
            },
        });
        Ok(())
    }

    /// Apply engine events: queue frames, persist carry changes, status
    /// changes. Returns deliveries for the caller to open.
    pub(crate) fn route(&mut self, ev: Vec<RelayEvent>) -> Result<Vec<MeshDelivery>, FfiError> {
        self.mesh.news.on_events(&ev, now_ms());
        let mut got = Vec::new();
        let mut last_link = None;
        for e in ev {
            match e {
                RelayEvent::Send { link, frame } => {
                    last_link = Some(link);
                    // Chat frames go ahead of queued live audio (LV2).
                    let q = self.mesh.out.entry(link).or_default();
                    let at = q
                        .iter()
                        .position(|f| {
                            f.pdu == PDU_MESH && omcp_core::live::audio::is_live_audio(&f.frame)
                        })
                        .unwrap_or(q.len());
                    q.insert(at, OutFrame::new(PDU_MESH, frame))
                }
                RelayEvent::Stored(it) => self.db.put_carry(&CarryRec {
                    id: it.envelope.envelope_id,
                    envelope: it.envelope.encode()?,
                    expires_at: it.expires_at,
                    hop_count: it.hop_count,
                    own: it.own,
                    ack_of: it.ack_of,
                    class: Some(it.class.to_u8()),
                })?,
                RelayEvent::Removed(id) => self.db.delete_carry(&id)?,
                RelayEvent::Deliver {
                    tag,
                    envelope,
                    ack_of,
                    ..
                } => got.push((tag, envelope, ack_of)),
                // Emitted right after the Send carrying it.
                RelayEvent::Handed(id) => {
                    if let Some(f) = last_link
                        .and_then(|l| self.mesh.out.get_mut(&l))
                        .and_then(|q| q.back_mut())
                    {
                        f.handed = Some(id);
                    }
                }
            }
        }
        Ok(got)
    }

    /// Mesh copy already superseded (delivered directly or burned).
    pub(crate) fn mesh_cancel_own(&mut self, id: &[u8; 16]) -> Result<(), FfiError> {
        let ev = self.mesh.engine.cancel_own(id);
        self.route(ev)?;
        Ok(())
    }

    /// Received over a direct link: ignore later mesh copies, across restarts.
    pub(crate) fn mesh_note_direct(&mut self, id: [u8; 16]) -> Result<(), FfiError> {
        let until = now_ms().saturating_add(MAX_EXPIRY_MS);
        self.mesh.engine.mark_seen(id, until);
        self.db.put_mesh_seen(&id, until)?;
        Ok(())
    }

    pub(crate) fn mesh_seen(&self, id: &[u8; 16]) -> bool {
        self.mesh.engine.seen(id)
    }

    fn queue_setup(&mut self, link: u64, kind: u8, body: &[u8]) {
        let mut f = Vec::with_capacity(1 + body.len());
        f.push(kind);
        f.extend_from_slice(body);
        self.mesh.out.entry(link).or_default().push_back(OutFrame {
            pdu: PDU_MLS_SETUP,
            frame: f,
            handed: None,
        });
    }
}

fn mls_key(peer: &[u8; 32]) -> Vec<u8> {
    let mut k = MLS_KEY.to_vec();
    k.extend_from_slice(peer);
    k
}

/// RL2 class of a persisted carry row (rows from before RL2 have none).
pub(crate) fn carry_class(r: &CarryRec) -> RelayClass {
    match r.class.and_then(RelayClass::from_u8) {
        _ if r.own => RelayClass::OwnChat,
        Some(c) if c != RelayClass::OwnChat => c,
        _ => RelayClass::Contact,
    }
}

/// Fingerprint of the ready MLS groups (tags must be recomputed when a group
/// comes up or changes epoch).
fn mls_fp(st: &NodeState) -> [u8; 32] {
    let mut v: Vec<([u8; 32], u64)> = st
        .mesh
        .mls
        .iter()
        .filter(|(id, m)| m.ready && st.contacts.contains_key(*id))
        .map(|(id, m)| (*id, m.member.epoch().unwrap_or(0)))
        .collect();
    v.sort();
    let mut h = blake3::Hasher::new();
    for (id, e) in v {
        h.update(&id);
        h.update(&e.to_be_bytes());
    }
    // Subscribed news channels are wanted too.
    for t in st.mesh.news.tags() {
        h.update(b"news");
        h.update(&t);
    }
    *h.finalize().as_bytes()
}

/// Recompute our wanted sub-tags (current and previous tag epoch) if the
/// epoch or the set of MLS groups changed.
pub(crate) fn refresh_wants(st: &mut NodeState, now: u64) -> Result<(), FfiError> {
    let epoch = tag_epoch(now);
    let fp = mls_fp(st);
    if st.mesh.tags_for == Some((epoch, fp)) {
        return Ok(());
    }
    let mut tags = HashMap::new();
    for (id, m) in &st.mesh.mls {
        if !m.ready || !st.contacts.contains_key(id) {
            continue;
        }
        let Ok(leaf) = m.member.own_leaf_index() else {
            continue;
        };
        for e in [epoch.saturating_sub(1), epoch] {
            let ts = m.member.tag_exporter(e).map_err(mls_err)?;
            tags.insert(subtag(&ts, leaf, e)?, *id);
        }
    }
    let mut wants: BTreeSet<MailboxTag> = tags.keys().copied().collect();
    // Hopla News: the phone subscribes only to its channels' public tags.
    wants.extend(st.mesh.news.tags());
    // Team moderation actions (§23.9) are wanted by every compliant client.
    wants.insert(omcp_core::moderation::team_mod_tag());
    st.mesh.tags = tags;
    st.mesh.tags_for = Some((epoch, fp));
    let ev = st.mesh.engine.set_wants(wants, now);
    st.route(ev)?;
    Ok(())
}

/// Hand a freshly queued message to the mesh as an opaque envelope. Without
/// an MLS group it stays in the local queue only (§22.8(1)).
pub(crate) fn submit_own(
    _inner: &NodeInner,
    st: &mut NodeState,
    peer: &[u8; 32],
    q: &Queued,
    burn: Option<&BurnHeader>,
) -> Result<(), FfiError> {
    if !st.contacts.contains_key(peer) || !st.mesh.mls_ready(peer) {
        return Ok(());
    }
    let expires_at = match burn {
        Some(h) => core_burn::expires_at_ms(q.created_ms, h),
        None => q.created_ms.saturating_add(DEFAULT_EXPIRY_MS),
    };
    let ttl = st.mesh.engine.new_ttl();
    let env = seal_for(
        st,
        peer,
        q.envelope_id,
        q.created_ms,
        expires_at,
        ttl,
        &q.inner,
    )?;
    refresh_wants(st, now_ms())?;
    let ev = st.mesh.engine.submit_own(env, now_ms())?;
    st.route(ev)?;
    Ok(())
}

/// The MLS group just came up: hand everything still queued for `peer` to
/// the mesh.
fn submit_pending(inner: &NodeInner, st: &mut NodeState, peer: &[u8; 32]) -> Result<(), FfiError> {
    refresh_wants(st, now_ms())?;
    let pending: Vec<PendingCopy> = st
        .queues
        .get(peer)
        .map(|q| {
            q.iter()
                .map(|q| (q.envelope_id, q.created_ms, q.inner.clone(), q.control))
                .collect()
        })
        .unwrap_or_default();
    for (id, created_ms, plain, control) in pending {
        if st.mesh.engine.seen(&id) {
            continue;
        }
        let burn = st
            .db()
            .message(&id)?
            .and_then(|m| m.burn)
            .and_then(|b| BurnHeader::decode(&b).ok());
        let q = Queued {
            envelope_id: id,
            created_ms,
            inner: plain,
            control,
        };
        submit_own(inner, st, peer, &q, burn.as_ref())?;
    }
    Ok(())
}

/// Recipient side: send the end-to-end `DELIVERY_ACK` for `env`.
fn send_ack(st: &mut NodeState, peer: &[u8; 32], env: &EnvelopeV1) -> Result<(), FfiError> {
    let h = msg_hash(&env.ciphertext);
    let now = now_ms();
    let expires_at = effective_expiry(env, now)?.max(now.saturating_add(60_000));
    let mut body = vec![FRAME_DELIVERY_ACK];
    body.extend_from_slice(&h);
    let ttl = st.mesh.engine.new_ttl();
    let ack = seal_for(st, peer, random_id16()?, now, expires_at, ttl, &body)?;
    let ev = st.mesh.engine.submit_ack(ack, h, now)?;
    st.route(ev)?;
    Ok(())
}

/// Periodic work (beacons, expiry, epoch rotation). Returns links with
/// frames waiting.
pub(crate) fn tick(st: &mut NodeState, now: u64) -> Result<Vec<u64>, FfiError> {
    refresh_wants(st, now)?;
    // Own copies of messages that were burned meanwhile are withdrawn.
    let stale: Vec<[u8; 16]> = st
        .mesh
        .engine
        .item_ids()
        .into_iter()
        .filter(|id| st.mesh.engine.item(id).is_some_and(|i| i.own))
        .filter(|id| st.burn_meta.contains_key(id) && !st.burn_out.is_live(id))
        .collect();
    for id in stale {
        st.mesh_cancel_own(&id)?;
    }
    let ev = st.mesh.engine.tick(now);
    st.route(ev)?;
    Ok(st.mesh.pending_links(None))
}

impl MeshLink {
    /// Link established: start mesh sync, and MLS setup if this contact has
    /// no 1:1 group yet.
    pub(crate) fn mesh_link_up(&self, peer: &[u8; 32]) -> Result<(), FfiError> {
        let now = now_ms();
        let mut node = self.node.lock();
        if !node.mesh.mls_ready(peer) {
            self.start_mls_setup(&mut node, peer)?;
        }
        refresh_wants(&mut node, now)?;
        // Links are only made to contacts today; the flag only ranks what
        // the neighbour hands us (RL2), everything is carried (RL1).
        let is_contact = node.contacts.contains_key(peer);
        let ev = node.mesh.engine.link_up(self.handle, peer, is_contact, now);
        node.route(ev)?;
        Ok(())
    }

    /// The higher id is the joiner: it sends its KeyPackage (or repeats
    /// `JOINED` if it already joined).
    fn start_mls_setup(&self, node: &mut NodeState, peer: &[u8; 32]) -> Result<(), FfiError> {
        let Some(c) = node.contacts.get(peer) else {
            return Ok(());
        };
        let me = my_id(&self.node, peer, c);
        if me < *peer {
            return Ok(()); // creator: waits for the KeyPackage
        }
        let joined = node
            .mesh
            .mls
            .get(peer)
            .is_some_and(|m| m.member.has_group());
        if joined {
            node.queue_setup(self.handle, SETUP_JOINED, &[]);
            return Ok(());
        }
        if !node.mesh.mls.contains_key(peer) {
            let member = OmcpMlsMember::generate(me.to_vec()).map_err(mls_err)?;
            node.mesh.mls.insert(
                *peer,
                ContactMls {
                    member,
                    ready: false,
                },
            );
            node.save_mls(peer)?;
        }
        let kp = node.mesh.mls[peer]
            .member
            .key_package_bytes()
            .map_err(mls_err)?;
        node.queue_setup(self.handle, SETUP_KEY_PACKAGE, &kp);
        Ok(())
    }

    /// One reassembled MLS setup frame (PDU `0x04`) from this link's contact.
    pub(crate) fn mls_setup_frame(&self, peer: &[u8; 32], frame: &[u8]) -> Result<(), FfiError> {
        let (&kind, body) = frame.split_first().ok_or_else(|| invalid("empty setup"))?;
        let mut node = self.node.lock();
        let Some(c) = node.contacts.get(peer) else {
            return Ok(());
        };
        let me = my_id(&self.node, peer, c);
        let creator = me < *peer;
        let mut became_ready = false;
        match (kind, creator) {
            (SETUP_KEY_PACKAGE, true) => {
                // New group (also after the joiner lost its state).
                let mut member = OmcpMlsMember::generate(me.to_vec()).map_err(mls_err)?;
                let kp = member.parse_key_package(body).map_err(mls_err)?;
                member.create_group().map_err(mls_err)?;
                let (_commit, welcome) = member.add_members(&[kp]).map_err(mls_err)?;
                node.mesh.mls.insert(
                    *peer,
                    ContactMls {
                        member,
                        ready: false,
                    },
                );
                node.save_mls(peer)?;
                node.queue_setup(self.handle, SETUP_WELCOME, &welcome);
            }
            (SETUP_WELCOME, false) => {
                let Some(m) = node.mesh.mls.get_mut(peer) else {
                    return Ok(());
                };
                if m.member.has_group() {
                    // Creator restarted the group: start over with a fresh member.
                    let mut fresh = OmcpMlsMember::generate(me.to_vec()).map_err(mls_err)?;
                    std::mem::swap(&mut m.member, &mut fresh);
                    m.ready = false;
                    let kp = m.member.key_package_bytes().map_err(mls_err)?;
                    node.save_mls(peer)?;
                    node.queue_setup(self.handle, SETUP_KEY_PACKAGE, &kp);
                    return Ok(());
                }
                m.member.join_from_welcome(body).map_err(mls_err)?;
                m.ready = true;
                became_ready = true;
                node.save_mls(peer)?;
                node.queue_setup(self.handle, SETUP_JOINED, &[]);
            }
            (SETUP_JOINED, true) => {
                if let Some(m) = node.mesh.mls.get_mut(peer) {
                    if m.member.has_group() && !m.ready {
                        m.ready = true;
                        became_ready = true;
                        node.save_mls(peer)?;
                    }
                }
            }
            _ => return Err(invalid("unexpected MLS setup frame")),
        }
        if became_ready {
            node.mesh.tags_for = None;
            submit_pending(&self.node, &mut node, peer)?;
        }
        Ok(())
    }

    /// One reassembled mesh frame from this link.
    pub(crate) fn mesh_frame(&self, frame: &[u8], out: &mut LinkOutput) -> Result<(), FfiError> {
        let now = now_ms();
        let mut node = self.node.lock();
        if node.on_live_frame(self.handle, frame, now) {
            return Ok(()); // live audio: forwarded / played, never stored
        }
        refresh_wants(&mut node, now)?;
        let ev = node.mesh.engine.on_frame(self.handle, frame, now)?;
        let from = node.links_up.get(&self.handle).copied();
        let mut news_via = false;
        for e in &ev {
            if let RelayEvent::Stored(it) = e {
                if it.class == RelayClass::News {
                    if let Some(p) = from {
                        news_via |= node.mesh.news.relayed_by(it.envelope.envelope_id, p);
                    }
                }
            }
        }
        for (tag, env, ack_of) in node.route(ev)? {
            if node.on_team_mod_envelope(&env)? {
                continue; // public team action: verified and applied, never a chat
            }
            if omcp_core::news::is_news_envelope(&env) {
                // Public news (already checked by the engine): no ack, no chat.
                node.mesh.news.note(&env, now);
                if let Some(p) = from {
                    news_via |= node.mesh.news.relayed_by(env.envelope_id, p);
                }
                continue;
            }
            let Some(peer) = node.mesh.tags.get(&tag).copied() else {
                continue;
            };
            if !node.contacts.contains_key(&peer) {
                continue;
            }
            let Ok(plain) = open_from(&mut node, &peer, &env) else {
                continue; // not for us after all (tag collision), replayed or forged
            };
            if let Some(h) = ack_of {
                // Only the MLS-authenticated ack counts.
                let mut want = vec![FRAME_DELIVERY_ACK];
                want.extend_from_slice(&h);
                if plain.as_slice() != want.as_slice() {
                    continue;
                }
                if let Some(id) = node.mesh.engine.own_by_hash(&h) {
                    node.mesh_cancel_own(&id)?;
                    node.set_status(&id, st::STATUS_DELIVERED)?;
                }
                continue;
            }
            // Receiving a valid application message proves the joiner joined.
            if let Some(m) = node.mesh.mls.get_mut(&peer) {
                if !m.ready {
                    m.ready = true;
                    node.save_mls(&peer)?;
                    node.mesh.tags_for = None;
                    submit_pending(&self.node, &mut node, &peer)?;
                }
            }
            // A failed ack only costs a redundant copy; never block delivery.
            let _ = send_ack(&mut node, &peer, &env);
            if node.db().message(&env.envelope_id)?.is_some()
                || node
                    .seen
                    .check_and_insert(env.envelope_id, std::time::Instant::now())
            {
                continue;
            }
            let until = env.created_ms.saturating_add(MAX_EXPIRY_MS);
            node.db().put_mesh_seen(&env.envelope_id, until)?;
            Self::deliver(&mut node, peer, &env, CONTENT_TYPE_INNER_V1, &plain, out)?;
        }
        if news_via {
            node.save_news()?;
        }
        Ok(())
    }

    /// Seal waiting mesh / setup frames for this link into `out`.
    pub(crate) fn drain_mesh(
        &self,
        up: &mut Established,
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let frames: Vec<OutFrame> = {
            let mut node = self.node.lock();
            let Some(q) = node.mesh.out.get_mut(&self.handle) else {
                return Ok(());
            };
            let n = q.len().min(MESH_FRAMES_PER_FLUSH);
            let v = q.drain(..n).collect();
            if !q.is_empty() {
                out.more_pending = true;
            }
            v
        };
        let mut handed = Vec::new();
        for f in frames {
            for frag in fragment_and_seal(
                &mut up.link,
                &f.frame,
                random_frag_id()?,
                LINK_FRAGMENT_PAYLOAD,
            )? {
                let mut p = Vec::with_capacity(1 + frag.len());
                p.push(f.pdu);
                p.extend_from_slice(&frag);
                out.outgoing.push(p);
            }
            handed.extend(f.handed);
        }
        if !handed.is_empty() {
            let mut node = self.node.lock();
            for id in handed {
                node.set_status(&id, st::STATUS_SENT)?;
            }
        }
        Ok(())
    }

    pub(crate) fn mesh_link_down(&self) {
        let mut node = self.node.lock();
        if let Some(q) = node.mesh.out.remove(&self.handle) {
            let lost: Vec<Vec<u8>> = q
                .into_iter()
                .filter(|f| f.pdu == PDU_MESH)
                .map(|f| f.frame)
                .collect();
            node.mesh.engine.unsent(self.handle, &lost);
        }
        node.mesh.engine.link_down(self.handle, now_ms());
    }

    pub(crate) fn relay_links(&self, out: &mut LinkOutput) {
        out.relay_links = self.node.lock().mesh.pending_links(Some(self.handle));
    }
}

#[uniffi::export]
impl OmcpNode {
    /// Periodic mesh work: call every few seconds while the radio is on (and
    /// after `queue_*`). Returns link handles with frames waiting; call
    /// `flush()` on each.
    pub fn mesh_tick(&self, now_ms: u64) -> Result<Vec<u64>, FfiError> {
        let mut st = self.inner.lock();
        tick(&mut st, now_ms)
    }

    /// Link handles with mesh frames waiting (e.g. after queueing a message).
    pub fn mesh_pending_links(&self) -> Vec<u64> {
        self.inner.lock().mesh.pending_links(None)
    }

    /// 6-byte `HELLO` beacon (§22.7(12)) for BLE advertisement service data.
    pub fn mesh_beacon(&self) -> Vec<u8> {
        let mut st = self.inner.lock();
        let _ = refresh_wants(&mut st, now_ms());
        st.mesh.engine.beacon().to_vec()
    }

    /// RL2: report free and total device storage (bytes), e.g. on every
    /// mesh tick. Under 10 % free the automatic 500 MB cap is lowered in
    /// proportion and the oldest, lowest-priority envelopes are dropped.
    /// There is no relay on/off or cap setting (RL1, RL4).
    pub fn mesh_set_free_storage(&self, free_bytes: u64, total_bytes: u64) -> Result<(), FfiError> {
        self.inner
            .lock()
            .mesh_set_free_storage(free_bytes, total_bytes)
    }

    pub fn mesh_stats(&self) -> MeshStats {
        self.inner.lock().mesh_stats()
    }

    /// Message status changes caused by the mesh since the last call
    /// (Queued → Sent when handed to a relay, → Delivered on an ack).
    pub fn mesh_take_status_updates(&self) -> Vec<MessageStatusUpdate> {
        std::mem::take(&mut self.inner.lock().mesh.status)
    }

    /// Whether the 1:1 MLS group with this contact is up (mesh relaying is
    /// only used once it is, §22.8(1)).
    pub fn mesh_ready(&self, contact_id: String) -> Result<bool, FfiError> {
        let id = crate::parse_id(&contact_id)?;
        Ok(self.inner.lock().mesh.mls_ready(&id))
    }
}

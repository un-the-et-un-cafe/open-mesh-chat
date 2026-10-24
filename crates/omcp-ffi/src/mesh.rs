//! Multi-hop store-and-forward mesh (PROTOCOL §8.3, §8.4, §22) on top of the
//! per-link BLE sessions — **v0.2 beta, experimental**.
//!
//! The relay logic lives in [`omcp_core::sync::relay::RelayEngine`]; this
//! module is the glue: end-to-end sealing for envelopes that travel through
//! other phones, the encrypted carry store, settings, and per-link frame
//! queues.
//!
//! # Link PDU
//!
//! `0x03 ‖ frame` — one link-AEAD fragment (same codec and `LinkSession` as
//! `0x02`) of a *mesh frame* (`HELLO`/`DIGEST_REQ` beacons, want-digests,
//! opaque envelopes; see the relay module). Mesh frames therefore only ever
//! travel inside an authenticated, encrypted link to a contact.
//!
//! # End-to-end sealing (beta, spec gap)
//!
//! §22.7(5) makes every chat a 2-member MLS group and derives tags from the
//! MLS exporter. The apps do not run MLS for 1:1 chats yet, so the beta uses a
//! **pairwise secret** both contacts can compute without being in range:
//!
//! ```text
//! pair       = HKDF-SHA-384(salt = "OMCP-mesh-pair-v1",
//!                           ikm  = X25519(our device key, their device key),
//!                           info = min(id_a, id_b) ‖ max(id_a, id_b))
//! tag_secret = HKDF-SHA-384(ikm = pair, info = "OMCP-tag" ‖ epoch_be)   // exporter stand-in
//! chat tag   = mailbox_tag(tag_secret, epoch)            // TLV 0x54
//! sub-tag    = subtag(tag_secret, leaf, epoch)           // TLV 0x56, leaf 0 = lower id
//! msg_key    = HKDF-SHA-384(ikm = pair, info = "OMCP-mesh-msg-v1" ‖ sender_leaf ‖ envelope_id)
//! ciphertext = seal_padded(msg_key, nonce = 0, aad, [Secret frame: InnerV1])
//! aad        = "OMCP-mesh-v1" ‖ version ‖ suite ‖ envelope_id ‖ created_ms ‖ flags ‖ header_aad
//! ```
//!
//! The key is unique per `envelope_id` (random) and sender, so the fixed
//! nonce is never reused. Relays see only tags, sizes (bucket-padded, §22.3)
//! and expiry. No forward secrecy / PQ for mesh hops (Noise still protects
//! each hop): this is the gap MLS closes.

use std::collections::{BTreeSet, HashMap, VecDeque};

use hkdf::Hkdf;
use omcp_core::ble::fragment::{fragment_and_seal, random_frag_id};
use omcp_core::ble::message::CONTENT_TYPE_INNER_V1;
use omcp_core::burn::{self as core_burn, BurnHeader};
use omcp_core::framing::flags;
use omcp_core::store::CarryRec;
use omcp_core::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use omcp_core::sync::budget::{RelayBudget, MAX_EXPIRY_MS, MB};
use omcp_core::sync::compress::{open_padded, seal_padded, Frame, FrameClass, SealOptions};
use omcp_core::sync::relay::{put_expires_in_aad, CarryItem, RelayConfig, RelayEngine, RelayEvent};
use omcp_core::sync::tag::{
    mailbox_tag, put_subtags_in_aad, put_tag_in_aad, subtag, tag_epoch, MailboxTag,
};
use omcp_core::EnvelopeV1;
use sha2::Sha384;
use zeroize::Zeroizing;

use crate::{
    invalid, now_ms, Contact, Established, FfiError, LinkOutput, MeshLink, NodeInner, NodeState,
    OmcpNode, Queued, LINK_FRAGMENT_PAYLOAD,
};

/// Link PDU kind for mesh frames.
pub(crate) const PDU_MESH: u8 = 0x03;
/// Mesh frames sealed onto one link per call (keeps BLE queues short).
pub(crate) const MESH_FRAMES_PER_FLUSH: usize = 32;
/// Simultaneous mesh links the apps should keep (central + peripheral roles).
pub const MAX_MESH_LINKS: u32 = 4;
const SETTINGS_KEY: &[u8] = b"mesh-settings";
const PAIR_SALT: &[u8] = b"OMCP-mesh-pair-v1";
const MSG_INFO: &[u8] = b"OMCP-mesh-msg-v1";
const AAD_PREFIX: &[u8] = b"OMCP-mesh-v1";

/// Relay settings shown in Settings → Mesh relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MeshSettings {
    /// Carry and forward other people's encrypted messages.
    pub relay_enabled: bool,
    /// Carry-store cap in bytes (§22.4 relay storage budget).
    pub carry_cap_bytes: u64,
}

/// Relay counters for Settings / diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MeshStats {
    pub carried: u32,
    pub carried_bytes: u64,
    pub own_pending: u32,
    pub neighbours: u32,
}

/// §22.4 default relay storage budget (battery / cellular): 50 MB.
#[uniffi::export]
pub fn mesh_default_carry_cap_bytes() -> u64 {
    RelayConfig::default().budget.storage_bytes
}

/// Upper bound accepted for the carry cap (§22.4 generous budget: 500 MB).
#[uniffi::export]
pub fn mesh_max_carry_cap_bytes() -> u64 {
    500 * MB
}

/// Recommended simultaneous mesh links per device.
#[uniffi::export]
pub fn mesh_max_links() -> u32 {
    MAX_MESH_LINKS
}

pub(crate) struct MeshState {
    pub engine: RelayEngine,
    /// Wanted sub-tag → contact.
    pub tags: HashMap<MailboxTag, [u8; 32]>,
    /// (epoch, contacts fingerprint) the tags were computed for.
    pub tags_for: Option<(u64, [u8; 32])>,
    /// Plain mesh frames waiting to be sealed onto each link.
    pub out: HashMap<u64, VecDeque<Vec<u8>>>,
}

impl MeshState {
    pub(crate) fn new(now: u64) -> Self {
        let seed = u64::from_le_bytes(rand16()[..8].try_into().expect("8"));
        Self {
            engine: RelayEngine::new(RelayConfig::default(), now, seed),
            tags: HashMap::new(),
            tags_for: None,
            out: HashMap::new(),
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
}

fn rand16() -> [u8; 16] {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    b
}

fn hkdf32(
    salt: Option<&[u8]>,
    ikm: &[u8],
    info: &[&[u8]],
) -> Result<Zeroizing<[u8; 32]>, FfiError> {
    let hk = Hkdf::<Sha384>::new(salt, ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    let info: Vec<u8> = info.concat();
    hk.expand(&info, &mut out[..])
        .map_err(|_| invalid("mesh hkdf"))?;
    Ok(out)
}

/// Our id as this contact knows us (legacy contacts know the legacy id).
fn my_id(inner: &NodeInner, id: &[u8; 32], c: &Contact) -> [u8; 32] {
    if c.legacy(id) {
        inner.legacy_id
    } else {
        inner.peer_id
    }
}

struct Pair {
    secret: Zeroizing<[u8; 32]>,
    my_leaf: u8,
}

fn pair(inner: &NodeInner, id: &[u8; 32], c: &Contact) -> Result<Pair, FfiError> {
    let me = my_id(inner, id, c);
    let ss = inner.identity()?.dh_agree(&c.dh)?;
    let (lo, hi) = if me < *id { (me, *id) } else { (*id, me) };
    Ok(Pair {
        secret: hkdf32(Some(PAIR_SALT), &ss[..], &[&lo, &hi])?,
        my_leaf: u8::from(me >= *id),
    })
}

fn tag_secret(p: &Pair, epoch: u64) -> Result<Zeroizing<[u8; 32]>, FfiError> {
    hkdf32(None, &p.secret[..], &[b"OMCP-tag", &epoch.to_be_bytes()])
}

fn msg_key(p: &Pair, sender_leaf: u8, id: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>, FfiError> {
    hkdf32(None, &p.secret[..], &[MSG_INFO, &[sender_leaf], id])
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

fn no_compress(burn: bool) -> SealOptions {
    SealOptions {
        compress: false,
        compress_quoted: false,
        burn,
        nearby: false,
    }
}

/// Seal `inner` (InnerV1) for contact `id` as an opaque mesh envelope.
#[allow(clippy::too_many_arguments)]
pub(crate) fn seal_for(
    inner: &NodeInner,
    id: &[u8; 32],
    c: &Contact,
    envelope_id: [u8; 16],
    created_ms: u64,
    expires_at: u64,
    ttl_hops: u8,
    plain: &[u8],
    burn: bool,
) -> Result<EnvelopeV1, FfiError> {
    let p = pair(inner, id, c)?;
    let epoch = tag_epoch(created_ms);
    let ts = tag_secret(&p, epoch)?;
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &mailbox_tag(&ts[..], epoch)?);
    put_subtags_in_aad(
        &mut header_aad,
        &[subtag(&ts[..], u32::from(1 - p.my_leaf), epoch)?],
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
    let key = msg_key(&p, p.my_leaf, &envelope_id)?;
    env.ciphertext = seal_padded(
        &key,
        &[0u8; 12],
        &aad(&env),
        &[Frame {
            class: FrameClass::Secret,
            data: plain.to_vec(),
        }],
        no_compress(burn),
    )?;
    Ok(env)
}

/// Open a mesh envelope from contact `id` (sender = the other leaf).
pub(crate) fn open_from(
    inner: &NodeInner,
    id: &[u8; 32],
    c: &Contact,
    env: &EnvelopeV1,
) -> Result<Zeroizing<Vec<u8>>, FfiError> {
    let p = pair(inner, id, c)?;
    let key = msg_key(&p, 1 - p.my_leaf, &env.envelope_id)?;
    let frames = open_padded(&key, &[0u8; 12], &aad(env), &env.ciphertext)?;
    let f = frames
        .into_iter()
        .next()
        .ok_or_else(|| invalid("empty mesh payload"))?;
    Ok(Zeroizing::new(f.data))
}

impl NodeState {
    /// Load settings, carry store and dedup set (node start).
    pub(crate) fn load_mesh(&mut self, now: u64) -> Result<(), FfiError> {
        let mut mesh = MeshState::new(now);
        if let Some((enabled, cap)) = self.db.get_meta::<(bool, u64)>(SETTINGS_KEY)? {
            mesh.engine.set_config(config(enabled, cap));
        }
        for r in self.db.carry_items()? {
            match EnvelopeV1::decode(&r.envelope) {
                Ok(envelope) if r.expires_at > now => mesh.engine.restore(
                    CarryItem {
                        envelope,
                        expires_at: r.expires_at,
                        hop_count: r.hop_count,
                        own: r.own,
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
        Ok(())
    }

    pub(crate) fn mesh_settings(&self) -> MeshSettings {
        let c = self.mesh.engine.config();
        MeshSettings {
            relay_enabled: c.enabled,
            carry_cap_bytes: c.budget.storage_bytes,
        }
    }

    pub(crate) fn set_mesh_settings(&mut self, s: MeshSettings) -> Result<(), FfiError> {
        if s.carry_cap_bytes > mesh_max_carry_cap_bytes() {
            return Err(invalid("carry cap above maximum"));
        }
        self.db
            .put_meta(SETTINGS_KEY, &(s.relay_enabled, s.carry_cap_bytes))?;
        let ev = self
            .mesh
            .engine
            .set_config(config(s.relay_enabled, s.carry_cap_bytes));
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
        }
    }

    /// Apply engine events: queue frames, persist carry changes. Returns
    /// deliveries for the caller to open.
    pub(crate) fn route(
        &mut self,
        ev: Vec<RelayEvent>,
    ) -> Result<Vec<(MailboxTag, EnvelopeV1)>, FfiError> {
        let mut got = Vec::new();
        for e in ev {
            match e {
                RelayEvent::Send { link, frame } => {
                    self.mesh.out.entry(link).or_default().push_back(frame)
                }
                RelayEvent::Stored(it) => self.db.put_carry(&CarryRec {
                    id: it.envelope.envelope_id,
                    envelope: it.envelope.encode()?,
                    expires_at: it.expires_at,
                    hop_count: it.hop_count,
                    own: it.own,
                })?,
                RelayEvent::Removed(id) => self.db.delete_carry(&id)?,
                RelayEvent::Deliver { tag, envelope, .. } => got.push((tag, envelope)),
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
}

fn config(enabled: bool, cap: u64) -> RelayConfig {
    let d = RelayConfig::default();
    RelayConfig {
        enabled,
        budget: RelayBudget {
            storage_bytes: cap,
            daily_forward_bytes: d.budget.daily_forward_bytes,
        },
    }
}

/// Fingerprint of the contact set + device keys (tags must be recomputed
/// when it changes).
fn contacts_fp(st: &NodeState) -> [u8; 32] {
    let mut ids: Vec<(&[u8; 32], &[u8; 32])> =
        st.contacts.iter().map(|(id, c)| (id, &c.dh)).collect();
    ids.sort();
    let mut h = blake3::Hasher::new();
    for (id, dh) in ids {
        h.update(id);
        h.update(dh);
    }
    *h.finalize().as_bytes()
}

/// Recompute our wanted sub-tags (current and previous epoch) if the epoch
/// or the contact set changed.
pub(crate) fn refresh_wants(
    inner: &NodeInner,
    st: &mut NodeState,
    now: u64,
) -> Result<(), FfiError> {
    let epoch = tag_epoch(now);
    let fp = contacts_fp(st);
    if st.mesh.tags_for == Some((epoch, fp)) {
        return Ok(());
    }
    let mut tags = HashMap::new();
    for (id, c) in &st.contacts {
        let Ok(p) = pair(inner, id, c) else {
            continue;
        };
        for e in [epoch.saturating_sub(1), epoch] {
            let ts = tag_secret(&p, e)?;
            tags.insert(subtag(&ts[..], u32::from(p.my_leaf), e)?, *id);
        }
    }
    let wants: BTreeSet<MailboxTag> = tags.keys().copied().collect();
    st.mesh.tags = tags;
    st.mesh.tags_for = Some((epoch, fp));
    let ev = st.mesh.engine.set_wants(wants, now);
    st.route(ev)?;
    Ok(())
}

/// Hand a freshly queued message to the mesh as an opaque envelope.
pub(crate) fn submit_own(
    inner: &NodeInner,
    st: &mut NodeState,
    peer: &[u8; 32],
    q: &Queued,
    burn: Option<&BurnHeader>,
) -> Result<(), FfiError> {
    let Some(c) = st.contacts.get(peer) else {
        return Ok(());
    };
    let expires_at = match burn {
        Some(h) => core_burn::expires_at_ms(q.created_ms, h),
        None => q
            .created_ms
            .saturating_add(omcp_core::sync::budget::DEFAULT_EXPIRY_MS),
    };
    let ttl = st.mesh.engine.new_ttl();
    let env = seal_for(
        inner,
        peer,
        c,
        q.envelope_id,
        q.created_ms,
        expires_at,
        ttl,
        &q.inner,
        burn.is_some(),
    )?;
    refresh_wants(inner, st, now_ms())?;
    let ev = st.mesh.engine.submit_own(env, now_ms())?;
    st.route(ev)?;
    Ok(())
}

/// Periodic work (beacons, expiry, epoch rotation). Returns links with
/// frames waiting.
pub(crate) fn tick(inner: &NodeInner, st: &mut NodeState, now: u64) -> Result<Vec<u64>, FfiError> {
    refresh_wants(inner, st, now)?;
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
    /// Link established: start mesh sync on it.
    pub(crate) fn mesh_link_up(&self, peer: &[u8; 32]) -> Result<(), FfiError> {
        let now = now_ms();
        let mut node = self.node.lock();
        refresh_wants(&self.node, &mut node, now)?;
        let ev = node.mesh.engine.link_up(self.handle, peer, now);
        node.route(ev)?;
        Ok(())
    }

    /// One reassembled mesh frame from this link.
    pub(crate) fn mesh_frame(&self, frame: &[u8], out: &mut LinkOutput) -> Result<(), FfiError> {
        let now = now_ms();
        let mut node = self.node.lock();
        refresh_wants(&self.node, &mut node, now)?;
        let ev = node.mesh.engine.on_frame(self.handle, frame, now)?;
        for (tag, env) in node.route(ev)? {
            let Some(peer) = node.mesh.tags.get(&tag).copied() else {
                continue;
            };
            let Some(c) = node.contacts.get(&peer) else {
                continue;
            };
            let Ok(plain) = open_from(&self.node, &peer, c, &env) else {
                continue; // not for us after all (tag collision) or forged
            };
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
        Ok(())
    }

    /// Seal waiting mesh frames for this link into `out`.
    pub(crate) fn drain_mesh(
        &self,
        up: &mut Established,
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let frames: Vec<Vec<u8>> = {
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
        for f in frames {
            for frag in
                fragment_and_seal(&mut up.link, &f, random_frag_id()?, LINK_FRAGMENT_PAYLOAD)?
            {
                let mut p = Vec::with_capacity(1 + frag.len());
                p.push(PDU_MESH);
                p.extend_from_slice(&frag);
                out.outgoing.push(p);
            }
        }
        Ok(())
    }

    pub(crate) fn mesh_link_down(&self) {
        let mut node = self.node.lock();
        node.mesh.engine.link_down(self.handle, now_ms());
        node.mesh.out.remove(&self.handle);
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
        tick(&self.inner, &mut st, now_ms)
    }

    /// Link handles with mesh frames waiting (e.g. after queueing a message).
    pub fn mesh_pending_links(&self) -> Vec<u64> {
        self.inner.lock().mesh.pending_links(None)
    }

    /// 6-byte `HELLO` beacon (§22.7(12)) for BLE advertisement service data.
    pub fn mesh_beacon(&self) -> Vec<u8> {
        let mut st = self.inner.lock();
        let _ = refresh_wants(&self.inner, &mut st, now_ms());
        st.mesh.engine.beacon().to_vec()
    }

    pub fn mesh_settings(&self) -> MeshSettings {
        self.inner.lock().mesh_settings()
    }

    pub fn set_mesh_settings(&self, settings: MeshSettings) -> Result<(), FfiError> {
        self.inner.lock().set_mesh_settings(settings)
    }

    pub fn mesh_stats(&self) -> MeshStats {
        self.inner.lock().mesh_stats()
    }
}

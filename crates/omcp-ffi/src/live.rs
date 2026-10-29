//! Live v0.2 over the FFI: offline audio over the BLE mesh (REQUIREMENTS
//! "Live", LV1–LV4). See `omcp_core::live` for the protocol.
//!
//! - The host invites contacts: the stream key travels in an E2EE chat
//!   message (`application/omcp-live-invite`, no chat row).
//! - Audio packets are mesh frames `0x4C`, sent to every link; every phone
//!   forwards them up to the hop limit, members decrypt into a jitter buffer.
//! - Live packets are never stored and are dropped (not queued) when a link
//!   already has [`LIVE_QUEUE_CAP`] waiting; chat frames go ahead of them.

use std::collections::{BTreeSet, HashMap};

use omcp_core::burn::AppContent;
use omcp_core::live::audio::is_live_audio;
use omcp_core::live::host::{HostSession, ListenerSession};
use omcp_core::live::jitter::Playout;
use omcp_core::live::relay::{LiveRelay, LIVE_QUEUE_CAP};
use omcp_core::live::sframe::stream_base_key;
use omcp_core::live::signal::{RtcSignal, SignalKind, SIGNAL_MIME};
use omcp_core::live::video::{wire, Dest, Routed, VideoRouter, HOST};
use omcp_core::live::{LiveInvite, StreamKey, INVITE_MIME};

use crate::mesh::{OutFrame, PDU_MESH};
use crate::{invalid, now_ms, parse_id, FfiError, LinkOutput, NodeState, OmcpNode};

#[derive(Default)]
pub(crate) struct LiveState {
    pub(crate) relay: LiveRelay,
    pub(crate) host: Option<HostSession>,
    /// Streams we were invited to: stream id → (host contact, invite).
    pub(crate) known: HashMap<[u8; 16], ([u8; 32], LiveInvite)>,
    pub(crate) listening: Option<ListenerSession>,
    pub(crate) forwarded: u64,
    pub(crate) dropped_congested: u64,
    /// Host: online video tree + signal router.
    pub(crate) video: VideoRouter,
    /// Signals for our app (offer/answer/ICE/assign/bye).
    pub(crate) signals_in: Vec<LiveSignalIn>,
    /// Host: signals to relay to viewers (sent on the next poll, outside the lock).
    pub(crate) pending_fwd: Vec<Routed>,
    /// Community lives announced to us (lock-screen notification, no content).
    pub(crate) started: Vec<LiveStarted>,
    /// Streams that stopped here because of a team takedown.
    pub(crate) ended: Vec<LiveEnded>,
    /// Host of a community live: the announcement nonce.
    pub(crate) community_nonce: Option<[u8; 32]>,
    /// Host of a community live: members removed from this stream (cumulative).
    pub(crate) removed_viewers: Vec<[u8; 32]>,
    /// Public audio tags of taken-down streams: relays drop them.
    pub(crate) blocked_tags: BTreeSet<[u8; 8]>,
    pub(crate) dropped_moderation: u64,
}

/// A community member went live (poll `live_take_started`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LiveStarted {
    pub stream_id_hex: String,
    pub community_id_hex: String,
}

/// A stream ended here for a reason other than the host stopping.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LiveEnded {
    pub stream_id_hex: String,
    /// `taken_down` (team moderation) or `removed` (the host removed you).
    pub reason: String,
    /// We were the host.
    pub mine: bool,
}

/// WebRTC signal kinds (PROTOCOL §20.3, content type 0x50).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LiveSignalKind {
    Offer,
    Answer,
    Ice,
    /// Tear down; body `parked` = no tree slot (audio over the mesh).
    Bye,
    Join,
    /// Open a peer connection to `peer_hex` (our new child) and send an offer.
    Assign,
}

impl From<SignalKind> for LiveSignalKind {
    fn from(k: SignalKind) -> Self {
        match k {
            SignalKind::Offer => Self::Offer,
            SignalKind::Answer => Self::Answer,
            SignalKind::Ice => Self::Ice,
            SignalKind::Bye => Self::Bye,
            SignalKind::Join => Self::Join,
            SignalKind::Assign => Self::Assign,
        }
    }
}

impl From<LiveSignalKind> for SignalKind {
    fn from(k: LiveSignalKind) -> Self {
        match k {
            LiveSignalKind::Offer => Self::Offer,
            LiveSignalKind::Answer => Self::Answer,
            LiveSignalKind::Ice => Self::Ice,
            LiveSignalKind::Bye => Self::Bye,
            LiveSignalKind::Join => Self::Join,
            LiveSignalKind::Assign => Self::Assign,
        }
    }
}

/// One incoming signal. `peer_hex` is the tree peer (64 zeros = the host).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LiveSignalIn {
    pub peer_hex: String,
    pub kind: LiveSignalKind,
    pub body: String,
}

/// SFrame media key for the WebRTC frame cryptor (RFC 9605 base key + KID).
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct LiveMediaKey {
    pub base_key: Vec<u8>,
    pub kid: u64,
}

impl std::fmt::Debug for LiveMediaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveMediaKey")
            .field("kid", &self.kid)
            .finish_non_exhaustive()
    }
}

/// A stream shown in the Live list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LiveStreamInfo {
    pub stream_id_hex: String,
    pub title: String,
    /// Empty for our own stream.
    pub host_contact_id: String,
    pub mine: bool,
    pub listening: bool,
    /// The host offers video (needs internet on both ends).
    pub video: bool,
    pub started_ms: u64,
    /// Hex community id for a community live, empty for a contact-picked one.
    pub community_id_hex: String,
}

/// One 20 ms slot for the player.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LivePlayout {
    pub index: u64,
    /// Empty when the frame was lost: run FEC with `fec_next` or PLC.
    pub opus: Vec<u8>,
    pub lost: bool,
    pub fec_next: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LiveStats {
    pub played: u64,
    pub lost: u64,
    pub forwarded: u64,
    pub dropped_congested: u64,
    pub listeners: u32,
    /// Packets of taken-down streams this phone refused to relay.
    pub dropped_moderation: u64,
}

fn is_live_out(f: &OutFrame) -> bool {
    f.pdu == PDU_MESH && is_live_audio(&f.frame)
}

impl NodeState {
    /// Queue one live packet on every link except `except`; drop on congestion.
    fn live_fanout(&mut self, pkt: &[u8], except: Option<u64>) {
        let links: Vec<u64> = self
            .links_up
            .keys()
            .copied()
            .filter(|l| Some(*l) != except)
            .collect();
        for l in links {
            let q = self.mesh.out.entry(l).or_default();
            if q.iter().filter(|f| is_live_out(f)).count() >= LIVE_QUEUE_CAP {
                self.mesh.live.dropped_congested += 1;
                continue;
            }
            q.push_back(OutFrame::new(PDU_MESH, pkt.to_vec()));
            self.mesh.live.forwarded += 1;
        }
    }

    /// A live audio frame from link `from`. `true` if it was one.
    pub(crate) fn on_live_frame(&mut self, from: u64, frame: &[u8], now: u64) -> bool {
        if !is_live_audio(frame) {
            return false;
        }
        if omcp_core::live::audio::AudioHeader::parse(frame)
            .is_ok_and(|h| self.mesh.live.blocked_tags.contains(&h.tag))
        {
            // Team takedown: neither played nor relayed (§23.9).
            self.mesh.live.dropped_moderation += 1;
            return true;
        }
        let d = self.mesh.live.relay.on_packet(frame, now);
        if d.deliver {
            if let Some(l) = self.mesh.live.listening.as_mut() {
                l.on_packet(frame, now);
            }
        }
        if let Some(f) = d.forward {
            self.live_fanout(&f, Some(from));
        }
        true
    }

    /// Current stream key: ours when hosting, else the one we listen to.
    fn live_key(&self) -> Option<StreamKey> {
        let lv = &self.mesh.live;
        lv.host
            .as_ref()
            .map(|h| h.key().clone())
            .or_else(|| lv.listening.as_ref().map(|l| l.invite.key.clone()))
    }

    /// A sealed 0x50 signal from contact `peer` (E2EE chat, no chat row).
    /// Returns signals to send on to other viewers (host only).
    pub(crate) fn on_live_signal(&mut self, peer: [u8; 32], bytes: &[u8]) -> Vec<Routed> {
        let Some(key) = self.live_key() else {
            return Vec::new();
        };
        let Ok(s) = RtcSignal::open(&key, bytes) else {
            return Vec::new(); // wrong key/epoch or garbage: drop
        };
        if self.mesh.live.host.is_some() {
            // Host: the authenticated sender is `peer`; route through the tree.
            let routed = self.mesh.live.video.on_signal(peer, s.to, s.kind, s.body);
            let mut fwd = Vec::new();
            for r in routed {
                match r.dest {
                    Dest::Local => self.mesh.live.signals_in.push(LiveSignalIn {
                        peer_hex: hex::encode(r.peer),
                        kind: r.kind.into(),
                        body: r.body,
                    }),
                    Dest::Contact(_) => fwd.push(r),
                }
            }
            fwd
        } else {
            // Viewer: only our host relays signals to us.
            let host = self
                .mesh
                .live
                .listening
                .as_ref()
                .map(|l| l.invite.key.stream_id);
            let from_host = host
                .and_then(|id| self.mesh.live.known.get(&id).map(|(h, _)| *h))
                .is_some_and(|h| h == peer);
            if from_host {
                self.mesh.live.signals_in.push(LiveSignalIn {
                    peer_hex: hex::encode(s.from),
                    kind: s.kind.into(),
                    body: s.body,
                });
            }
            Vec::new()
        }
    }

    pub(crate) fn on_live_invite(
        &mut self,
        peer: [u8; 32],
        bytes: &[u8],
        _out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let Ok(mut inv) = LiveInvite::decode(bytes) else {
            return Ok(()); // malformed invite: ignore
        };
        inv.host = peer; // the authenticated sender is the host
        inv.community_id = None; // community lives only come through the group
        let id = inv.key.stream_id;
        if inv.live {
            // Frames must be signed by the host's identity key: it has to be
            // the sender's (PeerID = BLAKE2b(identity key), or the pinned key).
            let pinned = self
                .contacts
                .get(&peer)
                .and_then(|c| c.device.identity_sign.clone());
            let ok = omcp_core::device::peer_id_from_identity(&inv.host_key) == peer
                || pinned.as_deref() == Some(inv.host_key.as_slice());
            if !ok || !self.mesh.moderation.live_allowed(&id, None) {
                return Ok(());
            }
        }
        if !inv.live {
            self.mesh.live.known.remove(&id);
            if self
                .mesh
                .live
                .listening
                .as_ref()
                .is_some_and(|l| l.invite.key.stream_id == id)
            {
                self.mesh.live.listening = None;
            }
            return Ok(());
        }
        if let Some(l) = self.mesh.live.listening.as_mut() {
            l.update(inv.clone());
        }
        match self.mesh.live.known.get(&id) {
            Some((_, old)) if old.key.key_epoch > inv.key.key_epoch => {}
            _ => {
                self.mesh.live.known.insert(id, (peer, inv));
            }
        }
        Ok(())
    }
}

impl NodeState {
    pub(crate) fn stop_hosting_quietly(&mut self) {
        self.mesh.live.video = VideoRouter::new();
        self.mesh.live.signals_in.clear();
        self.mesh.live.community_nonce = None;
    }

    /// The host ended a stream we know.
    pub(crate) fn live_ended_remote(&mut self, id: &[u8; 16]) {
        self.mesh.live.known.remove(id);
        if self
            .mesh
            .live
            .listening
            .as_ref()
            .is_some_and(|l| l.invite.key.stream_id == *id)
        {
            self.mesh.live.listening = None;
            self.mesh.live.signals_in.clear();
        }
    }

    /// Apply team takedowns to lives (after any accepted moderation action):
    /// the host stops broadcasting, viewers stop playing, relays stop forwarding.
    pub(crate) fn enforce_live_moderation(&mut self) {
        let m = &self.mesh.moderation;
        let lv = &mut self.mesh.live;
        lv.blocked_tags = m.removed_live_tags();
        if let Some(h) = lv.host.as_ref() {
            if !m.live_allowed(&h.stream_id(), h.community_id.as_ref()) {
                lv.ended.push(LiveEnded {
                    stream_id_hex: hex::encode(h.stream_id()),
                    reason: "taken_down".into(),
                    mine: true,
                });
                lv.host = None;
                lv.video = VideoRouter::new();
                lv.signals_in.clear();
                lv.pending_fwd.clear();
                lv.community_nonce = None;
            }
        }
        if let Some(l) = lv.listening.as_ref() {
            let i = &l.invite;
            if !m.live_allowed(&i.key.stream_id, i.community_id.as_ref()) {
                lv.ended.push(LiveEnded {
                    stream_id_hex: hex::encode(i.key.stream_id),
                    reason: "taken_down".into(),
                    mine: false,
                });
                lv.listening = None;
                lv.signals_in.clear();
            }
        }
        lv.known
            .retain(|id, (_, i)| m.live_allowed(id, i.community_id.as_ref()));
    }
}

fn parse_stream(hex_id: &str) -> Result<[u8; 16], FfiError> {
    hex::decode(hex_id.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("stream id"))
}

impl OmcpNode {
    /// Seal and send routed signals (host → viewer).
    pub(crate) fn send_routed(&self, routed: Vec<Routed>) -> Result<(), FfiError> {
        let key = self.inner.lock().live_key();
        let Some(key) = key else { return Ok(()) };
        for r in routed {
            if let Dest::Contact(to) = r.dest {
                let sig = wire(key.stream_id, r.peer, to, r.kind, r.body);
                self.send_signal_to(&to, &key, &sig)?;
            }
        }
        Ok(())
    }

    fn send_signal_to(
        &self,
        contact: &[u8; 32],
        key: &StreamKey,
        sig: &RtcSignal,
    ) -> Result<(), FfiError> {
        let bytes = sig.seal(key).map_err(|_| invalid("rtc signal"))?;
        self.queue_app(
            &hex::encode(contact),
            AppContent::Media {
                mime: SIGNAL_MIME.into(),
                bytes,
            },
            None,
            None,
            true,
        )
        .map(|_| ())
    }

    fn send_invite(&self, contact: &[u8; 32], inv: &LiveInvite) -> Result<(), FfiError> {
        let bytes = inv.encode().map_err(|_| invalid("live invite"))?;
        self.queue_app(
            &hex::encode(contact),
            AppContent::Media {
                mime: INVITE_MIME.into(),
                bytes,
            },
            None,
            None,
            true,
        )
        .map(|_| ())
    }
}

#[uniffi::export]
impl OmcpNode {
    /// Go live (audio over the mesh; `video` if this phone has internet).
    /// Invites `contact_ids` over the E2EE chat. Returns the stream id.
    pub fn live_start(
        &self,
        title: String,
        video: bool,
        contact_ids: Vec<String>,
    ) -> Result<String, FfiError> {
        let ids: Vec<[u8; 32]> = contact_ids
            .iter()
            .map(|c| parse_id(c))
            .collect::<Result<_, _>>()?;
        let inv = {
            let mut st = self.inner.lock();
            for id in &ids {
                if !st.contacts.contains_key(id) {
                    return Err(FfiError::UnknownContact);
                }
            }
            let mut h = HostSession::start([0; 32], &title, video, now_ms())
                .map_err(|_| invalid("live start"))?;
            h.set_signer(self.signing_key()?);
            for id in &ids {
                h.add_listener(*id);
            }
            let inv = h.invite(true);
            st.stop_hosting_quietly();
            st.mesh.live.host = Some(h);
            inv
        };
        for id in &ids {
            self.send_invite(id, &inv)?;
        }
        Ok(hex::encode(inv.key.stream_id))
    }

    /// Invite one more contact to our stream (late joiners start from the next frame).
    pub fn live_invite(&self, contact_id: String) -> Result<(), FfiError> {
        let id = parse_id(&contact_id)?;
        let inv = {
            let mut st = self.inner.lock();
            let Some(h) = st.mesh.live.host.as_mut() else {
                return Err(invalid("not live"));
            };
            h.add_listener(id);
            h.invite(true)
        };
        self.send_invite(&id, &inv)
    }

    /// Remove a listener: the key rotates and the others get the new key (LV4).
    pub fn live_remove_listener(&self, contact_id: String) -> Result<(), FfiError> {
        let id = parse_id(&contact_id)?;
        let community = {
            let st = self.inner.lock();
            st.mesh.live.host.as_ref().and_then(|h| h.community_id)
        };
        if let Some(cid) = community {
            // Community live: everyone else gets a host-signed new key; the
            // removed member can't derive it from the community exporter.
            {
                let mut st = self.inner.lock();
                if !st.mesh.live.removed_viewers.contains(&id) {
                    st.mesh.live.removed_viewers.push(id);
                }
                let ex = st.mesh.live.removed_viewers.clone();
                st.community_rekey(&cid, ex)?;
                if let Some(h) = st.mesh.live.host.as_mut() {
                    h.remove_listener(&id).map_err(|_| invalid("rotate"))?;
                }
                let fwd = st.mesh.live.video.leave(&id);
                st.mesh.live.pending_fwd.extend(fwd);
            }
            self.drain_community_outbox();
            return Ok(());
        }
        let (inv, notify) = {
            let mut st = self.inner.lock();
            let Some(h) = st.mesh.live.host.as_mut() else {
                return Err(invalid("not live"));
            };
            let n = h.remove_listener(&id).map_err(|_| invalid("rotate"))?;
            let inv = h.invite(true);
            let fwd = st.mesh.live.video.leave(&id);
            st.mesh.live.pending_fwd.extend(fwd);
            (inv, n)
        };
        for c in notify {
            self.send_invite(&c, &inv)?;
        }
        Ok(())
    }

    /// End our stream and tell the listeners.
    pub fn live_stop(&self) -> Result<(), FfiError> {
        let h = {
            let mut st = self.inner.lock();
            st.mesh.live.video = VideoRouter::new();
            let h = st.mesh.live.host.take();
            let nonce = st.mesh.live.community_nonce.take();
            if let (Some(h), Some(nonce)) = (h.as_ref(), nonce) {
                if let Some(cid) = h.community_id {
                    let a = omcp_core::live::community::CommunityLive {
                        community_id: cid,
                        stream_id: h.stream_id(),
                        nonce,
                        title: h.title.clone(),
                        started_ms: h.started_ms,
                        video: h.video,
                        max_hops: h.max_hops,
                        live: false,
                        host_key: h.host_key(),
                    };
                    let _ = st.announce_live(&cid, &a);
                }
            }
            h
        };
        self.drain_community_outbox();
        if let Some(h) = h {
            let inv = h.invite(false);
            for c in h.listeners() {
                self.send_invite(&c, &inv)?;
            }
        }
        Ok(())
    }

    /// Host: hand encoded 20 ms Opus frames (12 kbit/s mono). Returns the
    /// links to `flush()`.
    pub fn live_push_audio(&self, frames: Vec<Vec<u8>>) -> Result<Vec<u64>, FfiError> {
        let mut st = self.inner.lock();
        let now = now_ms();
        st.enforce_live_moderation();
        let Some(h) = st.mesh.live.host.as_mut() else {
            return Err(invalid("not live"));
        };
        let pkts = h
            .push_frames(frames, now)
            .map_err(|_| invalid("live frame"))?;
        for p in pkts {
            // Mark our own packets as seen so echoes are not forwarded back.
            let _ = st.mesh.live.relay.on_packet(&p, now);
            st.live_fanout(&p, None);
        }
        Ok(st.mesh.pending_links(None))
    }

    /// Viewer with internet: ask the host for a slot in the video tree.
    /// The host answers with an offer (from our parent) or `Bye parked`.
    pub fn live_video_join(&self) -> Result<(), FfiError> {
        self.live_send_signal(hex::encode(HOST), LiveSignalKind::Join, String::new())
    }

    /// Send a WebRTC signal to tree peer `peer_hex` (64 zeros = host). Viewers
    /// always send via the host, which relays it sealed to the other viewer.
    pub fn live_send_signal(
        &self,
        peer_hex: String,
        kind: LiveSignalKind,
        body: String,
    ) -> Result<(), FfiError> {
        let to = parse_id(&peer_hex)?;
        let (key, dest, routed) = {
            let st = self.inner.lock();
            let key = st.live_key().ok_or_else(|| invalid("not live"))?;
            if st.mesh.live.host.is_some() {
                let r = st
                    .mesh
                    .live
                    .video
                    .from_host(to, kind.into(), body.clone())
                    .ok_or_else(|| invalid("not a direct viewer"))?;
                (key, to, Some(r))
            } else {
                let id = key.stream_id;
                let host = st
                    .mesh
                    .live
                    .known
                    .get(&id)
                    .map(|(h, _)| *h)
                    .ok_or_else(|| invalid("unknown stream"))?;
                (key, host, None)
            }
        };
        match routed {
            Some(r) => self.send_routed(vec![r]),
            None => {
                let sig = wire(key.stream_id, HOST, to, kind.into(), body);
                self.send_signal_to(&dest, &key, &sig)
            }
        }
    }

    /// Host: a direct viewer's link failed for good; re-parent its children.
    pub fn live_video_drop(&self, peer_hex: String) -> Result<(), FfiError> {
        let id = parse_id(&peer_hex)?;
        let routed = {
            let mut st = self.inner.lock();
            let mut out = Vec::new();
            for r in st.mesh.live.video.leave(&id) {
                match r.dest {
                    Dest::Local => st.mesh.live.signals_in.push(LiveSignalIn {
                        peer_hex: hex::encode(r.peer),
                        kind: r.kind.into(),
                        body: r.body,
                    }),
                    Dest::Contact(_) => out.push(r),
                }
            }
            out
        };
        self.send_routed(routed)
    }

    /// Signals waiting for the app (poll with the audio loop).
    pub fn live_take_signals(&self) -> Vec<LiveSignalIn> {
        let fwd = std::mem::take(&mut self.inner.lock().mesh.live.pending_fwd);
        if !fwd.is_empty() {
            let _ = self.send_routed(fwd);
        }
        std::mem::take(&mut self.inner.lock().mesh.live.signals_in)
    }

    /// SFrame media key for the current stream (frame cryptor). Changes when
    /// the key rotates (listener removed): re-read after `live_streams()`.
    pub fn live_media_key(&self) -> Option<LiveMediaKey> {
        let k = self.inner.lock().live_key()?;
        Some(LiveMediaKey {
            base_key: stream_base_key(&k).to_vec(),
            kid: u64::from(k.key_epoch),
        })
    }

    /// Video tree: viewers placed / direct children of the host.
    pub fn live_video_viewers(&self) -> u32 {
        self.inner.lock().mesh.live.video.tree.viewers() as u32
    }

    /// Streams we can join (invites) plus our own.
    pub fn live_streams(&self) -> Vec<LiveStreamInfo> {
        let st = self.inner.lock();
        let lv = &st.mesh.live;
        let listening = lv.listening.as_ref().map(|l| l.invite.key.stream_id);
        let mut v: Vec<LiveStreamInfo> = lv
            .host
            .iter()
            .map(|h| LiveStreamInfo {
                stream_id_hex: hex::encode(h.stream_id()),
                title: h.title.clone(),
                host_contact_id: String::new(),
                mine: true,
                listening: false,
                video: h.video,
                started_ms: h.started_ms,
                community_id_hex: h.community_id.map(hex::encode).unwrap_or_default(),
            })
            .collect();
        let mut others: Vec<_> = lv
            .known
            .iter()
            .map(|(id, (peer, inv))| LiveStreamInfo {
                stream_id_hex: hex::encode(id),
                title: inv.title.clone(),
                host_contact_id: hex::encode(peer),
                mine: false,
                listening: listening == Some(*id),
                video: inv.video,
                started_ms: inv.started_ms,
                community_id_hex: inv.community_id.map(hex::encode).unwrap_or_default(),
            })
            .collect();
        others.sort_by_key(|s| std::cmp::Reverse(s.started_ms));
        v.extend(others);
        v
    }

    /// Start listening to an invited stream (one at a time).
    pub fn live_join(&self, stream_id_hex: String, has_internet: bool) -> Result<(), FfiError> {
        let id = parse_stream(&stream_id_hex)?;
        let mut st = self.inner.lock();
        let inv = st
            .mesh
            .live
            .known
            .get(&id)
            .map(|(_, i)| i.clone())
            .ok_or_else(|| invalid("unknown stream"))?;
        if !st
            .mesh
            .moderation
            .live_allowed(&id, inv.community_id.as_ref())
        {
            return Err(invalid("live taken down"));
        }
        st.mesh.live.listening = Some(ListenerSession::new(inv, has_internet));
        Ok(())
    }

    pub fn live_leave(&self) {
        let _ = self.live_send_signal(hex::encode(HOST), LiveSignalKind::Bye, String::new());
        let mut st = self.inner.lock();
        st.mesh.live.listening = None;
        st.mesh.live.signals_in.clear();
    }

    /// Frames due for playout now (call every 20–100 ms while listening).
    pub fn live_take_audio(&self, now_ms: u64) -> Vec<LivePlayout> {
        let mut st = self.inner.lock();
        let Some(l) = st.mesh.live.listening.as_mut() else {
            return Vec::new();
        };
        l.jitter
            .pop_due(now_ms)
            .into_iter()
            .map(|p| match p {
                Playout::Frame { index, opus } => LivePlayout {
                    index,
                    opus,
                    lost: false,
                    fec_next: None,
                },
                Playout::Lost { index, next } => LivePlayout {
                    index,
                    opus: Vec::new(),
                    lost: true,
                    fec_next: next,
                },
            })
            .collect()
    }

    pub fn live_stats(&self) -> LiveStats {
        let st = self.inner.lock();
        let lv = &st.mesh.live;
        LiveStats {
            played: lv.listening.as_ref().map_or(0, |l| l.jitter.played),
            lost: lv.listening.as_ref().map_or(0, |l| l.jitter.lost),
            forwarded: lv.forwarded,
            dropped_congested: lv.dropped_congested,
            listeners: lv.host.as_ref().map_or(0, |h| h.listeners().len() as u32),
            dropped_moderation: lv.dropped_moderation,
        }
    }

    /// Community lives that started since the last call: show a lock-screen
    /// notification with the generic text (no title, no host name).
    pub fn live_take_started(&self) -> Vec<LiveStarted> {
        std::mem::take(&mut self.inner.lock().mesh.live.started)
    }

    /// Streams stopped here by a team takedown: stop the mic/camera, close
    /// every WebRTC connection (so relaying viewers stop forwarding video).
    pub fn live_take_ended(&self) -> Vec<LiveEnded> {
        let mut st = self.inner.lock();
        st.enforce_live_moderation();
        std::mem::take(&mut st.mesh.live.ended)
    }
}

/// Opus bitrate (bit/s) for the apps' encoders.
#[uniffi::export]
pub fn live_audio_bitrate() -> u32 {
    omcp_core::live::AUDIO_BITRATE
}

#[uniffi::export]
pub fn live_frame_ms() -> u64 {
    omcp_core::live::FRAME_MS
}

/// STUN servers for WebRTC video (public STUN only; no TURN).
#[uniffi::export]
pub fn live_stun_servers() -> Vec<String> {
    omcp_core::live::STUN_SERVERS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

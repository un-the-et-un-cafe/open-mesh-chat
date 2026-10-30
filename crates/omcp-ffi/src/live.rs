//! Live v0.2 over the FFI: offline audio over the BLE mesh (REQUIREMENTS
//! "Live", LV1–LV4). See `omcp_core::live` for the protocol.
//!
//! - The host invites contacts: the stream key travels in an E2EE chat
//!   message (`application/omcp-live-invite`, no chat row).
//! - Audio packets are mesh frames `0x4C`, sent to every link; every phone
//!   forwards them up to the hop limit, members decrypt into a jitter buffer.
//! - Live packets are never stored and are dropped (not queued) when a link
//!   already has [`LIVE_QUEUE_CAP`] waiting; chat frames go ahead of them.

use std::collections::HashMap;

use omcp_core::burn::AppContent;
use omcp_core::live::audio::is_live_audio;
use omcp_core::live::host::{HostSession, ListenerSession};
use omcp_core::live::jitter::Playout;
use omcp_core::live::relay::{LiveRelay, LIVE_QUEUE_CAP};
use omcp_core::live::{LiveInvite, INVITE_MIME};

use crate::mesh::{OutFrame, PDU_MESH};
use crate::{invalid, now_ms, parse_id, FfiError, LinkOutput, NodeState, OmcpNode};

#[derive(Default)]
pub(crate) struct LiveState {
    relay: LiveRelay,
    host: Option<HostSession>,
    /// Streams we were invited to: stream id → (host contact, invite).
    known: HashMap<[u8; 16], ([u8; 32], LiveInvite)>,
    listening: Option<ListenerSession>,
    forwarded: u64,
    dropped_congested: u64,
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
        let id = inv.key.stream_id;
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

fn parse_stream(hex_id: &str) -> Result<[u8; 16], FfiError> {
    hex::decode(hex_id.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| invalid("stream id"))
}

impl OmcpNode {
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
            for id in &ids {
                h.add_listener(*id);
            }
            let inv = h.invite(true);
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
        let (inv, notify) = {
            let mut st = self.inner.lock();
            let Some(h) = st.mesh.live.host.as_mut() else {
                return Err(invalid("not live"));
            };
            let n = h.remove_listener(&id).map_err(|_| invalid("rotate"))?;
            (h.invite(true), n)
        };
        for c in notify {
            self.send_invite(&c, &inv)?;
        }
        Ok(())
    }

    /// End our stream and tell the listeners.
    pub fn live_stop(&self) -> Result<(), FfiError> {
        let h = self.inner.lock().mesh.live.host.take();
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
        st.mesh.live.listening = Some(ListenerSession::new(inv, has_internet));
        Ok(())
    }

    pub fn live_leave(&self) {
        self.inner.lock().mesh.live.listening = None;
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
        }
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

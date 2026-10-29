//! Host side: listeners, invites and key rotation.

use std::collections::BTreeSet;

use super::audio::{seal_audio, FRAMES_PER_PACKET};
use super::{LiveInvite, StreamKey, DEFAULT_MAX_HOPS};
use crate::error::Result;

#[derive(Debug)]
pub struct HostSession {
    pub host: [u8; 32],
    pub title: String,
    pub started_ms: u64,
    pub video: bool,
    pub max_hops: u8,
    key: StreamKey,
    listeners: BTreeSet<[u8; 32]>,
    seq: u32,
    pending: Vec<Vec<u8>>,
}

impl HostSession {
    pub fn start(host: [u8; 32], title: &str, video: bool, now_ms: u64) -> Result<Self> {
        let mut id = [0u8; 16];
        getrandom::getrandom(&mut id).map_err(|_| crate::OmcpError::Sync("rng"))?;
        Ok(Self {
            host,
            title: title.chars().take(super::MAX_TITLE).collect(),
            started_ms: now_ms,
            video,
            max_hops: DEFAULT_MAX_HOPS,
            key: StreamKey::random(id, 0)?,
            listeners: BTreeSet::new(),
            seq: 0,
            pending: Vec::new(),
        })
    }

    pub fn key(&self) -> &StreamKey {
        &self.key
    }

    pub fn stream_id(&self) -> [u8; 16] {
        self.key.stream_id
    }

    pub fn invite(&self, live: bool) -> LiveInvite {
        LiveInvite {
            host: self.host,
            title: self.title.clone(),
            started_ms: self.started_ms,
            video: self.video,
            max_hops: self.max_hops,
            key: self.key.clone(),
            live,
        }
    }

    /// Add a listener; send them [`Self::invite`].
    pub fn add_listener(&mut self, id: [u8; 32]) -> bool {
        self.listeners.insert(id)
    }

    /// Remove a listener and rotate the key. Returns the listeners that need
    /// the new invite; the removed one cannot decrypt new frames (LV4).
    pub fn remove_listener(&mut self, id: &[u8; 32]) -> Result<Vec<[u8; 32]>> {
        if !self.listeners.remove(id) {
            return Ok(Vec::new());
        }
        self.key = StreamKey::random(self.key.stream_id, self.key.key_epoch + 1)?;
        Ok(self.listeners.iter().copied().collect())
    }

    pub fn listeners(&self) -> Vec<[u8; 32]> {
        self.listeners.iter().copied().collect()
    }

    /// Feed encoded 20 ms Opus frames; returns sealed packets every
    /// [`FRAMES_PER_PACKET`] frames.
    pub fn push_frames(&mut self, frames: Vec<Vec<u8>>, now_ms: u64) -> Result<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        for f in frames {
            self.pending.push(f);
            if self.pending.len() == FRAMES_PER_PACKET {
                let batch = std::mem::take(&mut self.pending);
                out.push(seal_audio(
                    &self.key,
                    self.seq,
                    self.max_hops,
                    now_ms,
                    &batch,
                )?);
                self.seq = self.seq.wrapping_add(1);
            }
        }
        Ok(out)
    }
}

/// Listener side: holds the invite key and a jitter buffer.
#[derive(Debug)]
pub struct ListenerSession {
    pub invite: LiveInvite,
    pub jitter: super::jitter::JitterBuffer,
    pub mode: super::mode::ModeSelector,
}

impl ListenerSession {
    pub fn new(invite: LiveInvite, my_internet: bool) -> Self {
        let video = invite.video;
        Self {
            invite,
            jitter: super::jitter::JitterBuffer::new(super::jitter::DEFAULT_TARGET_MS),
            mode: super::mode::ModeSelector::new(my_internet, video),
        }
    }

    /// A newer invite (key rotation) replaces the key; frames keep flowing.
    pub fn update(&mut self, invite: LiveInvite) {
        if invite.key.stream_id == self.invite.key.stream_id
            && invite.key.key_epoch >= self.invite.key.key_epoch
        {
            self.invite = invite;
        }
    }

    /// Decrypt one packet into the jitter buffer. `false` if it is not ours
    /// or does not open.
    pub fn on_packet(&mut self, pkt: &[u8], now_ms: u64) -> bool {
        match super::audio::open_audio(&self.invite.key, pkt) {
            Ok((h, frames)) => {
                let base = h.seq as u64 * FRAMES_PER_PACKET as u64;
                for (k, f) in frames.into_iter().enumerate() {
                    self.jitter.push(base + k as u64, f, now_ms);
                }
                true
            }
            Err(_) => false,
        }
    }
}

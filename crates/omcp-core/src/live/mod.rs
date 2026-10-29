//! Live v0.2 (REQUIREMENTS "Live (v0.2, draft)", PROTOCOL §20.3).
//!
//! Offline: audio only, Opus 12 kbit/s mono, 20 ms frames, spread over the BLE
//! mesh up to 3 hops ([`audio`], [`relay`], [`jitter`]). Online: peer-to-peer
//! WebRTC video in a viewer tree, host fan-out 6, viewer fan-out 4 ([`tree`]),
//! signalled through E2EE OMCP messages ([`signal`]). The mode follows
//! connectivity ([`mode`]).
//!
//! # Keys (LV4)
//!
//! The host draws a random 32-byte stream secret and sends it, inside a
//! [`LiveInvite`], to each listener over the existing end-to-end encrypted
//! chat (MLS). Every frame and signal is sealed with
//! `key = HKDF-SHA-256(salt = "OMCP-live-key-v1", ikm = secret, info = stream_id ‖ key_epoch)`
//! (XChaCha20-Poly1305). Relays that are not listeners only see the public
//! header (tag, sequence, hop count, time) and cannot decrypt. Removing a
//! listener rotates the secret and bumps `key_epoch` ([`host::HostSession`]).

pub mod audio;
pub mod host;
pub mod jitter;
pub mod mode;
pub mod relay;
pub mod signal;
pub mod tree;

#[cfg(test)]
mod sim;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};

use crate::error::{OmcpError, Result};

/// Opus bitrate for mesh audio (bit/s); fallback for weak links.
pub const AUDIO_BITRATE: u32 = 12_000;
pub const AUDIO_BITRATE_FALLBACK: u32 = 6_000;
/// Opus frame length.
pub const FRAME_MS: u64 = 20;
/// Default hop limit for mesh audio.
pub const DEFAULT_MAX_HOPS: u8 = 3;
/// Audio packets older than this are dropped everywhere (never stored by Relay).
pub const AUDIO_EXPIRY_MS: u64 = 5_000;
/// Host fan-out and viewer fan-out in the video tree.
pub const HOST_FANOUT: usize = 6;
pub const VIEWER_FANOUT: usize = 4;
/// Public STUN only reveals the phone's public address; no media passes
/// through it. TURN is off by default (REQUIREMENTS "Connectivity").
pub const STUN_SERVERS: &[&str] = &["stun:stun.l.google.com:19302"];

pub(crate) const NONCE_LEN: usize = 24;

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

pub(crate) fn blake2b(parts: &[&[u8]], out: &mut [u8]) {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(out.len()).expect("valid BLAKE2b output size");
    for p in parts {
        h.update(p);
    }
    h.finalize_variable(out).expect("output size matches");
}

/// Public 8-byte routing tag of a stream (dedup key for relays).
pub fn stream_tag(stream_id: &[u8; 16]) -> [u8; 8] {
    let mut o = [0u8; 8];
    blake2b(&[b"OMCP-live-tag-v1\x00", stream_id], &mut o);
    o
}

/// The stream key material a listener holds.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamKey {
    pub stream_id: [u8; 16],
    pub key_epoch: u32,
    secret: [u8; 32],
}

impl std::fmt::Debug for StreamKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamKey")
            .field("stream_id", &self.stream_id)
            .field("key_epoch", &self.key_epoch)
            .finish_non_exhaustive()
    }
}

impl StreamKey {
    pub fn new(stream_id: [u8; 16], key_epoch: u32, secret: [u8; 32]) -> Self {
        Self {
            stream_id,
            key_epoch,
            secret,
        }
    }

    pub fn random(stream_id: [u8; 16], key_epoch: u32) -> Result<Self> {
        let mut secret = [0u8; 32];
        getrandom::getrandom(&mut secret).map_err(|_| OmcpError::Sync("rng"))?;
        Ok(Self::new(stream_id, key_epoch, secret))
    }

    pub fn secret(&self) -> &[u8; 32] {
        &self.secret
    }

    pub fn tag(&self) -> [u8; 8] {
        stream_tag(&self.stream_id)
    }

    fn aead_key(&self) -> [u8; 32] {
        let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(b"OMCP-live-key-v1"), &self.secret);
        let mut info = self.stream_id.to_vec();
        info.extend_from_slice(&self.key_epoch.to_be_bytes());
        let mut k = [0u8; 32];
        hk.expand(&info, &mut k)
            .expect("32 bytes is a valid HKDF length");
        k
    }

    pub(crate) fn seal(&self, plain: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::getrandom(&mut nonce).map_err(|_| OmcpError::Sync("rng"))?;
        let c = XChaCha20Poly1305::new_from_slice(&self.aead_key())
            .map_err(|_| OmcpError::Sync("live key"))?;
        let ct = c
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
            .map_err(|_| OmcpError::Sync("live seal"))?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub(crate) fn open(&self, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
        if sealed.len() < NONCE_LEN + 16 {
            return err("live ciphertext");
        }
        let (n, ct) = sealed.split_at(NONCE_LEN);
        let c = XChaCha20Poly1305::new_from_slice(&self.aead_key())
            .map_err(|_| OmcpError::Sync("live key"))?;
        c.decrypt(XNonce::from_slice(n), Payload { msg: ct, aad })
            .map_err(|_| OmcpError::Sync("live open"))
    }
}

/// Stream announcement + key, sent to each listener over the E2EE chat.
/// Late joiners get one too and start from the next frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveInvite {
    pub host: [u8; 32],
    pub title: String,
    pub started_ms: u64,
    /// Host has internet and offers video.
    pub video: bool,
    pub max_hops: u8,
    pub key: StreamKey,
    /// `false` = the stream ended (sent to listeners on stop).
    pub live: bool,
}

/// MIME used to carry a [`LiveInvite`] inside an E2EE chat message.
pub const INVITE_MIME: &str = "application/omcp-live-invite";
pub const MAX_TITLE: usize = 120;

impl LiveInvite {
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.title.len() > MAX_TITLE {
            return err("live title too long");
        }
        let mut b = Vec::new();
        ciborium::into_writer(self, &mut b).map_err(|_| OmcpError::Sync("live invite cbor"))?;
        Ok(b)
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() > 1024 {
            return err("live invite too large");
        }
        let v: Self = ciborium::from_reader(b).map_err(|_| OmcpError::Sync("live invite cbor"))?;
        if v.title.len() > MAX_TITLE || v.max_hops == 0 || v.max_hops > 8 {
            return err("live invite fields");
        }
        Ok(v)
    }
}

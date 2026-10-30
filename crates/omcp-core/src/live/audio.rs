//! Mesh audio packet (link frame `0x4C`).
//!
//! ```text
//! 0x4C ‖ version 1 ‖ tag[8] ‖ key_epoch u32 ‖ seq u32 ‖ hop u8 ‖ max_hops u8
//!      ‖ sent_ms u64 ‖ nonce[24] ‖ XChaCha20-Poly1305(key, plain, aad)
//! plain = n u8 ‖ n × (len u8 ‖ opus frame)      // n consecutive 20 ms frames
//! aad   = header without the hop byte            // relays bump `hop`
//! ```
//!
//! Version 2 (host-signed, every stream since v0.2 community live) appends
//! `sig[64]` = Ed25519(host identity key, "OMCP-live-audio-sig-v1" ‖ aad ‖
//! nonce ‖ ciphertext). Members hold the stream key, so AEAD alone can't stop
//! a relaying member from injecting frames; the signature can. Cost: 64 B per
//! 100 ms packet (5.1 kbit/s on top of 12 kbit/s Opus), one verify per packet.
//!
//! Frame `k` of packet `seq` has index `seq × n + k` for the jitter buffer.
//! Packing 5 frames (100 ms) per packet keeps BLE at 10 packets/s; there is
//! no retransmission, Opus in-band FEC covers single losses.

use super::{err, StreamKey};
use crate::error::Result;

pub const FRAME_LIVE_AUDIO: u8 = 0x4C;
const VERSION: u8 = 1;
const VERSION_SIGNED: u8 = 2;
pub const SIG_LEN: usize = 64;
const SIG_DOMAIN: &[u8] = b"OMCP-live-audio-sig-v1\x00";
const HEADER_LEN: usize = 1 + 1 + 8 + 4 + 4 + 1 + 1 + 8;
const HOP_AT: usize = 18;
/// Frames per packet (5 × 20 ms).
pub const FRAMES_PER_PACKET: usize = 5;
pub const MAX_FRAME_BYTES: usize = 160;

/// Cleartext header, readable by any relay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioHeader {
    pub tag: [u8; 8],
    pub key_epoch: u32,
    pub seq: u32,
    pub hop: u8,
    pub max_hops: u8,
    pub sent_ms: u64,
    pub signed: bool,
}

impl AudioHeader {
    fn bytes(&self) -> Vec<u8> {
        let v = if self.signed { VERSION_SIGNED } else { VERSION };
        let mut b = vec![FRAME_LIVE_AUDIO, v];
        b.extend_from_slice(&self.tag);
        b.extend_from_slice(&self.key_epoch.to_be_bytes());
        b.extend_from_slice(&self.seq.to_be_bytes());
        b.push(self.hop);
        b.push(self.max_hops);
        b.extend_from_slice(&self.sent_ms.to_be_bytes());
        b
    }

    fn aad(&self) -> Vec<u8> {
        let mut b = self.bytes();
        b[HOP_AT] = 0;
        b
    }

    pub fn parse(pkt: &[u8]) -> Result<Self> {
        if pkt.len() < HEADER_LEN
            || pkt[0] != FRAME_LIVE_AUDIO
            || (pkt[1] != VERSION && pkt[1] != VERSION_SIGNED)
        {
            return err("not a live audio packet");
        }
        let signed = pkt[1] == VERSION_SIGNED;
        if signed && pkt.len() < HEADER_LEN + SIG_LEN {
            return err("live packet signature");
        }
        let h = Self {
            tag: pkt[2..10].try_into().expect("8"),
            key_epoch: u32::from_be_bytes(pkt[10..14].try_into().expect("4")),
            seq: u32::from_be_bytes(pkt[14..18].try_into().expect("4")),
            hop: pkt[18],
            max_hops: pkt[19],
            sent_ms: u64::from_be_bytes(pkt[20..28].try_into().expect("8")),
            signed,
        };
        if h.hop == 0 || h.max_hops == 0 || h.hop > h.max_hops {
            return err("live hop count");
        }
        Ok(h)
    }
}

pub fn is_live_audio(pkt: &[u8]) -> bool {
    pkt.first() == Some(&FRAME_LIVE_AUDIO)
}

fn sig_body(aad: &[u8], sealed: &[u8]) -> Vec<u8> {
    let mut b = SIG_DOMAIN.to_vec();
    b.extend_from_slice(aad);
    b.extend_from_slice(sealed);
    b
}

/// Host side: seal `frames` (each one Opus frame) as packet `seq`, unsigned
/// (version 1; kept for tests and old peers).
pub fn seal_audio(
    key: &StreamKey,
    seq: u32,
    max_hops: u8,
    sent_ms: u64,
    frames: &[Vec<u8>],
) -> Result<Vec<u8>> {
    seal_audio_signed(key, None, seq, max_hops, sent_ms, frames)
}

/// Host side: seal and, with `signer`, sign (version 2).
pub fn seal_audio_signed(
    key: &StreamKey,
    signer: Option<&ed25519_dalek::SigningKey>,
    seq: u32,
    max_hops: u8,
    sent_ms: u64,
    frames: &[Vec<u8>],
) -> Result<Vec<u8>> {
    if frames.is_empty() || frames.len() > 16 {
        return err("live frame count");
    }
    let h = AudioHeader {
        tag: key.tag(),
        key_epoch: key.key_epoch,
        seq,
        hop: 1,
        max_hops,
        sent_ms,
        signed: signer.is_some(),
    };
    let mut plain = vec![frames.len() as u8];
    for f in frames {
        if f.is_empty() || f.len() > MAX_FRAME_BYTES {
            return err("live frame size");
        }
        plain.push(f.len() as u8);
        plain.extend_from_slice(f);
    }
    let mut pkt = h.bytes();
    let sealed = key.seal(&plain, &h.aad())?;
    pkt.extend_from_slice(&sealed);
    if let Some(sk) = signer {
        use ed25519_dalek::Signer;
        pkt.extend_from_slice(&sk.sign(&sig_body(&h.aad(), &sealed)).to_bytes());
    }
    Ok(pkt)
}

/// Relay side: the same packet one hop further (`None` at the hop limit).
pub fn next_hop(pkt: &[u8]) -> Result<Option<Vec<u8>>> {
    let h = AudioHeader::parse(pkt)?;
    if h.hop >= h.max_hops {
        return Ok(None);
    }
    let mut p = pkt.to_vec();
    p[HOP_AT] = h.hop + 1;
    Ok(Some(p))
}

/// Listener side: decrypt. Fails for anyone without the stream key (LV4).
pub fn open_audio(key: &StreamKey, pkt: &[u8]) -> Result<(AudioHeader, Vec<Vec<u8>>)> {
    open_audio_auth(key, None, pkt)
}

/// Decrypt and, with `host_key`, require a valid host signature: frames
/// injected by a member who holds the stream key are rejected.
pub fn open_audio_auth(
    key: &StreamKey,
    host_key: Option<&[u8; 32]>,
    pkt: &[u8],
) -> Result<(AudioHeader, Vec<Vec<u8>>)> {
    let h = AudioHeader::parse(pkt)?;
    if h.tag != key.tag() || h.key_epoch != key.key_epoch {
        return err("live packet for another stream or key epoch");
    }
    let sealed = if h.signed {
        &pkt[HEADER_LEN..pkt.len() - SIG_LEN]
    } else {
        &pkt[HEADER_LEN..]
    };
    if let Some(hk) = host_key {
        if !h.signed {
            return err("live packet not signed by the host");
        }
        let sig: [u8; SIG_LEN] = pkt[pkt.len() - SIG_LEN..].try_into().expect("64");
        let vk = ed25519_dalek::VerifyingKey::from_bytes(hk)
            .map_err(|_| crate::OmcpError::Sync("live host key"))?;
        vk.verify_strict(
            &sig_body(&h.aad(), sealed),
            &ed25519_dalek::Signature::from_bytes(&sig),
        )
        .map_err(|_| crate::OmcpError::Sync("live packet signature"))?;
    }
    let plain = key.open(sealed, &h.aad())?;
    let n = *plain.first().ok_or(crate::OmcpError::Sync("live plain"))? as usize;
    let mut frames = Vec::with_capacity(n);
    let mut i = 1;
    for _ in 0..n {
        let l = *plain.get(i).ok_or(crate::OmcpError::Sync("live plain"))? as usize;
        let f = plain
            .get(i + 1..i + 1 + l)
            .ok_or(crate::OmcpError::Sync("live plain"))?;
        frames.push(f.to_vec());
        i += 1 + l;
    }
    if i != plain.len() {
        return err("live plain trailing bytes");
    }
    Ok((h, frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    #[test]
    fn signed_packets_survive_relays_and_injected_frames_are_rejected() {
        let key = StreamKey::random([5; 16], 0).unwrap();
        let host = SigningKey::from_bytes(&[1; 32]);
        let hk = host.verifying_key().to_bytes();
        let frames = vec![vec![7u8; 30]; 5];
        let p = seal_audio_signed(&key, Some(&host), 3, 3, 100, &frames).unwrap();
        assert_eq!(
            p.len(),
            seal_audio(&key, 3, 3, 100, &frames).unwrap().len() + SIG_LEN
        );
        // Relays bump the hop; the signature still holds.
        let p2 = next_hop(&p).unwrap().unwrap();
        assert_eq!(open_audio_auth(&key, Some(&hk), &p2).unwrap().1, frames);
        // A member holds the stream key: unsigned or self-signed injection fails.
        let inj = seal_audio(&key, 4, 3, 100, &frames).unwrap();
        assert!(open_audio_auth(&key, Some(&hk), &inj).is_err());
        let member = SigningKey::from_bytes(&[2; 32]);
        let inj2 = seal_audio_signed(&key, Some(&member), 4, 3, 100, &frames).unwrap();
        assert!(open_audio_auth(&key, Some(&hk), &inj2).is_err());
        // Tampering with the ciphertext or seq breaks the signature.
        let mut t = p.clone();
        t[16] ^= 1;
        assert!(open_audio_auth(&key, Some(&hk), &t).is_err());
    }
}

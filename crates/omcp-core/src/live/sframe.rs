//! SFrame (RFC 9605) end-to-end encryption of live video/audio frames.
//!
//! Cipher suite `AES_128_GCM_SHA256_128` (0x0004). The base key is derived
//! from the per-stream secret ([`StreamKey`]) and the KID is the key epoch,
//! so a listener removal (key rotation) also rotates the media key. Relaying
//! viewers never need the key to forward an SFrame ciphertext: the header is
//! public and the body is opaque.
//!
//! Wire: `header ‖ AEAD(key, nonce = salt ⊕ CTR, aad = header ‖ metadata, pt)`.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;

use super::{err, StreamKey};
use crate::error::{OmcpError, Result};

pub const CIPHER_SUITE: u16 = 0x0004;
const NK: usize = 16;
const NN: usize = 12;
const NT: usize = 16;

/// Encode an SFrame header (RFC 9605 §4.3).
pub fn encode_header(kid: u64, ctr: u64) -> Vec<u8> {
    fn min_be(v: u64) -> Vec<u8> {
        let b = v.to_be_bytes();
        let skip = b.iter().take_while(|x| **x == 0).count().min(7);
        b[skip..].to_vec()
    }
    let mut cfg = 0u8;
    let mut tail = Vec::new();
    if kid < 8 {
        cfg |= (kid as u8) << 4;
    } else {
        let k = min_be(kid);
        cfg |= 0x80 | (((k.len() - 1) as u8) << 4);
        tail.extend_from_slice(&k);
    }
    if ctr < 8 {
        cfg |= ctr as u8;
    } else {
        let c = min_be(ctr);
        cfg |= 0x08 | ((c.len() - 1) as u8);
        tail.extend_from_slice(&c);
    }
    let mut out = vec![cfg];
    out.extend_from_slice(&tail);
    out
}

/// Decode a header: `(kid, ctr, header_len)`.
pub fn decode_header(b: &[u8]) -> Result<(u64, u64, usize)> {
    let Some(&cfg) = b.first() else {
        return err("sframe header");
    };
    let mut pos = 1;
    let mut take = |n: usize| -> Result<u64> {
        if b.len() < pos + n {
            return err("sframe header");
        }
        let v = b[pos..pos + n]
            .iter()
            .fold(0u64, |a, x| (a << 8) | u64::from(*x));
        pos += n;
        Ok(v)
    };
    let kid = if cfg & 0x80 != 0 {
        take((((cfg >> 4) & 0x07) + 1) as usize)?
    } else {
        u64::from((cfg >> 4) & 0x07)
    };
    let ctr = if cfg & 0x08 != 0 {
        take(((cfg & 0x07) + 1) as usize)?
    } else {
        u64::from(cfg & 0x07)
    };
    Ok((kid, ctr, pos))
}

/// One SFrame key context (key + salt for one KID).
pub struct SframeKey {
    pub kid: u64,
    key: [u8; NK],
    salt: [u8; NN],
}

impl std::fmt::Debug for SframeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SframeKey")
            .field("kid", &self.kid)
            .finish_non_exhaustive()
    }
}

impl SframeKey {
    /// RFC 9605 §4.4.2 key derivation from `base_key`.
    pub fn derive(kid: u64, base_key: &[u8]) -> Self {
        let hk = Hkdf::<Sha256>::new(Some(&[]), base_key);
        let label = |what: &str| {
            let mut l = format!("SFrame 1.0 Secret {what} ").into_bytes();
            l.extend_from_slice(&kid.to_be_bytes());
            l.extend_from_slice(&CIPHER_SUITE.to_be_bytes());
            l
        };
        let mut key = [0u8; NK];
        let mut salt = [0u8; NN];
        hk.expand(&label("key"), &mut key).expect("valid length");
        hk.expand(&label("salt"), &mut salt).expect("valid length");
        Self { kid, key, salt }
    }

    /// The media key for a live stream: base key from the stream secret, KID = key epoch.
    pub fn for_stream(k: &StreamKey) -> Self {
        Self::derive(u64::from(k.key_epoch), &stream_base_key(k))
    }

    fn nonce(&self, ctr: u64) -> [u8; NN] {
        let mut n = self.salt;
        for (i, b) in ctr.to_be_bytes().iter().enumerate() {
            n[NN - 8 + i] ^= b;
        }
        n
    }

    pub fn encrypt(&self, ctr: u64, metadata: &[u8], pt: &[u8]) -> Result<Vec<u8>> {
        let header = encode_header(self.kid, ctr);
        let mut aad = header.clone();
        aad.extend_from_slice(metadata);
        let c = Aes128Gcm::new_from_slice(&self.key).map_err(|_| OmcpError::Sync("sframe key"))?;
        let ct = c
            .encrypt(
                Nonce::from_slice(&self.nonce(ctr)),
                Payload { msg: pt, aad: &aad },
            )
            .map_err(|_| OmcpError::Sync("sframe seal"))?;
        let mut out = header;
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn decrypt(&self, metadata: &[u8], frame: &[u8]) -> Result<Vec<u8>> {
        let (kid, ctr, hl) = decode_header(frame)?;
        if kid != self.kid {
            return err("sframe kid");
        }
        if frame.len() < hl + NT {
            return err("sframe short");
        }
        let mut aad = frame[..hl].to_vec();
        aad.extend_from_slice(metadata);
        let c = Aes128Gcm::new_from_slice(&self.key).map_err(|_| OmcpError::Sync("sframe key"))?;
        c.decrypt(
            Nonce::from_slice(&self.nonce(ctr)),
            Payload {
                msg: &frame[hl..],
                aad: &aad,
            },
        )
        .map_err(|_| OmcpError::Sync("sframe open"))
    }
}

/// 16-byte SFrame base key for a stream (HKDF-SHA-256, domain-separated from the
/// mesh audio key). Also handed to the WebRTC frame cryptor on the apps.
pub fn stream_base_key(k: &StreamKey) -> [u8; 16] {
    let hk = Hkdf::<Sha256>::new(Some(b"OMCP-live-sframe-v1"), k.secret());
    let mut info = k.stream_id.to_vec();
    info.extend_from_slice(&k.key_epoch.to_be_bytes());
    let mut out = [0u8; 16];
    hk.expand(&info, &mut out).expect("valid length");
    out
}

/// Sender state: monotonically increasing CTR per key.
#[derive(Debug)]
pub struct SframeSender {
    key: SframeKey,
    ctr: u64,
}

impl SframeSender {
    pub fn new(key: SframeKey) -> Self {
        Self { key, ctr: 0 }
    }
    pub fn protect(&mut self, metadata: &[u8], pt: &[u8]) -> Result<Vec<u8>> {
        let c = self.ctr;
        self.ctr = self
            .ctr
            .checked_add(1)
            .ok_or(OmcpError::Sync("sframe ctr"))?;
        self.key.encrypt(c, metadata, pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        let s: String = s.split_whitespace().collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn rfc9605_header_vectors() {
        let cases: &[(u64, u64, &str)] = &[
            (0, 0, "00"),
            (0, 1, "01"),
            (0, 0xff, "08ff"),
            (0, 0x100, "090100"),
            (0, 0xffff, "09ffff"),
            (0, 0x10000, "0a010000"),
            (0, 0xffffffff, "0bffffffff"),
            (0, 0x100000000, "0c0100000000"),
            (0, 0xffff_ffff_ffff_ffff, "0fffffffffffffffff"),
            (0x123, 0x4567, "9901234567"),
            (7, 7, "77"),
            (8, 0, "8008"),
        ];
        for (kid, ctr, hex) in cases {
            assert_eq!(encode_header(*kid, *ctr), h(hex), "kid {kid} ctr {ctr}");
            let b = h(hex);
            assert_eq!(decode_header(&b).unwrap(), (*kid, *ctr, b.len()));
        }
        assert!(decode_header(&[]).is_err());
        assert!(decode_header(&h("0b0100")).is_err());
    }

    #[test]
    fn rfc9605_c3_aes_128_gcm_sha256_128() {
        let k = SframeKey::derive(0x123, &h("000102030405060708090a0b0c0d0e0f"));
        assert_eq!(k.key.to_vec(), h("d34f547f4ca4f9a7447006fe7fcbf768"));
        assert_eq!(k.salt.to_vec(), h("75234edefe07819026751816"));
        assert_eq!(k.nonce(0x4567).to_vec(), h("75234edefe07819026755d71"));
        let md = h("4945544620534672616d65205747");
        let pt = h("64726166742d696574662d736672616d652d656e63");
        let ct = h(
            "9901234567b7412c2513a1b66dbb48841bbaf17f598751176ad847681a69c6d0b091c07018ce4adb34eb",
        );
        assert_eq!(k.encrypt(0x4567, &md, &pt).unwrap(), ct);
        assert_eq!(k.decrypt(&md, &ct).unwrap(), pt);
    }

    #[test]
    fn tamper_wrong_key_and_epoch_rejected() {
        let sk = StreamKey::new([3; 16], 2, [9; 32]);
        let mut tx = SframeSender::new(SframeKey::for_stream(&sk));
        let a = tx.protect(b"", b"frame-a").unwrap();
        let b = tx.protect(b"", b"frame-b").unwrap();
        assert_ne!(a[0], b[0]); // counter advanced
        let rx = SframeKey::for_stream(&sk);
        assert_eq!(rx.decrypt(b"", &b).unwrap(), b"frame-b");
        let mut bad = a.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(rx.decrypt(b"", &bad).is_err());
        assert!(rx.decrypt(b"x", &a).is_err());
        let other = SframeKey::for_stream(&StreamKey::new([3; 16], 2, [8; 32]));
        assert!(other.decrypt(b"", &a).is_err());
        let rotated = SframeKey::for_stream(&StreamKey::new([3; 16], 3, [9; 32]));
        assert!(rotated.decrypt(b"", &a).is_err()); // kid mismatch after rotation
                                                    // Mesh audio key and SFrame key are domain-separated.
        assert_ne!(stream_base_key(&sk).to_vec(), sk.secret()[..16].to_vec());
    }

    #[test]
    fn relay_forwards_ciphertext_unchanged() {
        // A relaying viewer copies the SFrame bytes; the leaf still decrypts.
        let sk = StreamKey::new([1; 16], 0, [5; 32]);
        let mut tx = SframeSender::new(SframeKey::for_stream(&sk));
        let f = tx.protect(&[], b"keyframe").unwrap();
        let relayed = f.clone();
        assert_eq!(
            SframeKey::for_stream(&sk).decrypt(&[], &relayed).unwrap(),
            b"keyframe"
        );
    }
}

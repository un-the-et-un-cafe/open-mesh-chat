//! Private-group traffic on the group's own MLS mailbox tags (§22.7(5), (9)).
//!
//! Members who are not each other's contacts (for example someone who joined
//! by a §20.7 invite) still exchange group messages: every group frame is
//! sealed with a key from the group's MLS exporter and published on the
//! group's chat tag (`mailbox_tag(MLS-Exporter("OMCP-tag", epoch))`). Relays
//! and the team see only an opaque 1:1-style envelope: no group ID, member
//! list or roster ever appears outside the seal.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

use crate::error::{OmcpError, Result};
use crate::framing::flags;
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{mailbox_tag, put_tag_in_aad, MailboxTag};
use crate::EnvelopeV1;

const KDF_CTX: &[u8] = b"OMCP-group-frame-v1\0";

/// Group chat tag for tag epoch `e` from `tag_exporter(e)`.
pub fn group_tag(tag_secret: &[u8; 32], e: u64) -> Result<MailboxTag> {
    mailbox_tag(tag_secret, e)
}

/// Frame key: BLAKE2b-256(ctx || tag_secret).
pub fn frame_key(tag_secret: &[u8; 32]) -> [u8; 32] {
    use blake2::digest::{Update, VariableOutput};
    let mut h = blake2::Blake2bVar::new(32).expect("32");
    h.update(KDF_CTX);
    h.update(tag_secret);
    let mut o = [0u8; 32];
    h.finalize_variable(&mut o).expect("32");
    o
}

fn aad(env: &EnvelopeV1) -> Vec<u8> {
    let mut a = KDF_CTX.to_vec();
    a.extend_from_slice(&env.envelope_id);
    a.extend_from_slice(&env.created_ms.to_be_bytes());
    a.extend_from_slice(&env.header_aad);
    a
}

/// Seal `frame` into a group envelope on `tag`, padded to a §22.3 bucket.
pub fn seal(
    tag_secret: &[u8; 32],
    tag: &MailboxTag,
    frame: &[u8],
    envelope_id: [u8; 16],
    created_ms: u64,
    expires_ms: u64,
    ttl_hops: u8,
) -> Result<EnvelopeV1> {
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, tag);
    put_expires_in_aad(&mut header_aad, expires_ms);
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
    let want = crate::sync::compress::bucket_for(frame.len() + 4 + 12 + 16);
    let mut pt = (frame.len() as u32).to_be_bytes().to_vec();
    pt.extend_from_slice(frame);
    pt.resize(want - 12 - 16, 0);
    let mut nonce = [0u8; 12];
    getrandom::getrandom(&mut nonce).map_err(|_| OmcpError::Sync("rng"))?;
    let c = ChaCha20Poly1305::new(&frame_key(tag_secret).into());
    let a = aad(&env);
    let ct = c
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: &pt, aad: &a })
        .map_err(|_| OmcpError::Sync("group seal"))?;
    env.ciphertext = nonce.to_vec();
    env.ciphertext.extend_from_slice(&ct);
    Ok(env)
}

/// Open a group envelope with the frame key for its tag epoch.
pub fn open(tag_secret: &[u8; 32], env: &EnvelopeV1) -> Result<Vec<u8>> {
    if env.ciphertext.len() < 12 + 16 + 4 {
        return Err(OmcpError::Sync("group frame short"));
    }
    let c = ChaCha20Poly1305::new(&frame_key(tag_secret).into());
    let a = aad(env);
    let pt = c
        .decrypt(
            Nonce::from_slice(&env.ciphertext[..12]),
            Payload {
                msg: &env.ciphertext[12..],
                aad: &a,
            },
        )
        .map_err(|_| OmcpError::Sync("group open"))?;
    let n = u32::from_be_bytes(pt[..4].try_into().unwrap()) as usize;
    pt.get(4..4 + n)
        .map(|b| b.to_vec())
        .ok_or(OmcpError::Sync("group frame length"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip_and_wrong_key_fails() {
        let ts = [7u8; 32];
        let tag = group_tag(&ts, 5).unwrap();
        let env = seal(&ts, &tag, b"hello group", [1; 16], 10, 20, 4).unwrap();
        assert_eq!(open(&ts, &env).unwrap(), b"hello group");
        assert!(open(&[8u8; 32], &env).is_err());
        // Outer header: tag + expiry only; nothing else.
        assert!(!env.header_aad.windows(11).any(|w| w == b"hello group"));
        let mut bad = env.clone();
        bad.created_ms += 1;
        assert!(open(&ts, &bad).is_err());
    }
}

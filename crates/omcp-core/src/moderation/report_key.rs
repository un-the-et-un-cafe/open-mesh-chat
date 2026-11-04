//! Per-community / per-group report key (PROTOCOL §23.2, §23.3; TM34; MR6,
//! MR70, MR71).
//!
//! `seed = HKDF-SHA256(ikm = identity secret seed, salt = community_ref or
//! group_ref, info = "OMCP-report-key-v1", L = 32)`. Deterministic (same on
//! every device of the identity and after a restore), so it is never stored
//! in backups: it is re-derived. It signs only reports, attestations and
//! evidence files; the identity key never signs those.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::error::{OmcpError, Result};

pub const REPORT_KEY_INFO: &[u8] = b"OMCP-report-key-v1";
const COMMUNITY_REPORT_DOMAIN: &[u8] = b"OMCP-community-report-v2\x00";

/// What a report is about: the salt of the report key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportScope {
    /// `group_ref` (§23.10).
    Group([u8; 32]),
    /// `community_ref` (§23.10.11(2)).
    Community([u8; 32]),
}

impl ReportScope {
    pub fn salt(&self) -> &[u8; 32] {
        match self {
            Self::Group(r) | Self::Community(r) => r,
        }
    }
}

/// Derive the report key for one community or group.
pub fn report_key(identity: &SigningKey, scope: &ReportScope) -> SigningKey {
    let mut ikm = identity.to_bytes();
    let k = derive_seed(&ikm, scope.salt());
    ikm.zeroize();
    k
}

/// Raw derivation from the 32-byte identity seed (known-answer tests).
pub fn derive_seed(identity_seed: &[u8; 32], salt: &[u8; 32]) -> SigningKey {
    let hk = Hkdf::<Sha256>::new(Some(salt), identity_seed);
    let mut seed = [0u8; 32];
    hk.expand(REPORT_KEY_INFO, &mut seed)
        .expect("32 bytes is a valid HKDF length");
    let k = SigningKey::from_bytes(&seed);
    seed.zeroize();
    k
}

/// Key separation (§23.2, MUST): a report/attestation key that is also a
/// roster identity key (or a message sender) is rejected.
pub fn reporter_key_allowed<'a>(
    reporter_key: &[u8; 32],
    identity_keys: impl IntoIterator<Item = &'a [u8; 32]>,
) -> bool {
    let h = super::peer_id_hash(reporter_key);
    !identity_keys
        .into_iter()
        .any(|k| k == reporter_key || *k == h)
}

/// A signed report about a community post (§23.2): signed with the
/// community's report key, never the identity key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityReport {
    pub community_ref: [u8; 32],
    /// The author-signed post bytes being reported (CBOR).
    pub post: Vec<u8>,
    pub reason_code: u8,
    pub note: String,
    pub reporter_key: [u8; 32],
    pub sig: [u8; 64],
}

impl CommunityReport {
    fn body(&self) -> Vec<u8> {
        let mut b = COMMUNITY_REPORT_DOMAIN.to_vec();
        b.extend_from_slice(&self.community_ref);
        b.extend_from_slice(&(self.post.len() as u32).to_be_bytes());
        b.extend_from_slice(&self.post);
        b.push(self.reason_code);
        b.extend_from_slice(&(self.note.len() as u16).to_be_bytes());
        b.extend_from_slice(self.note.as_bytes());
        b.extend_from_slice(&self.reporter_key);
        b
    }

    /// Sign with the report key derived for `community_ref`.
    pub fn new(
        identity: &SigningKey,
        community_ref: [u8; 32],
        post: Vec<u8>,
        reason_code: u8,
        note: &str,
    ) -> Result<Self> {
        if !matches!(reason_code, 1..=7 | 9 | 10) || note.len() > 500 {
            return Err(OmcpError::Sync("report fields"));
        }
        let k = report_key(identity, &ReportScope::Community(community_ref));
        let mut r = Self {
            community_ref,
            post,
            reason_code,
            note: note.to_string(),
            reporter_key: k.verifying_key().to_bytes(),
            sig: [0u8; 64],
        };
        r.sig = k.sign(&r.body()).to_bytes();
        Ok(r)
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.body()[COMMUNITY_REPORT_DOMAIN.len()..].to_vec();
        b.extend_from_slice(&self.sig);
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        let e = || OmcpError::Sync("community report");
        let take = |o: usize, n: usize| b.get(o..o + n).ok_or_else(e);
        let community_ref = take(0, 32)?.try_into().expect("32");
        let pl = u32::from_be_bytes(take(32, 4)?.try_into().expect("4")) as usize;
        let post = take(36, pl)?.to_vec();
        let mut o = 36 + pl;
        let reason_code = take(o, 1)?[0];
        o += 1;
        let nl = u16::from_be_bytes(take(o, 2)?.try_into().expect("2")) as usize;
        o += 2;
        let note = std::str::from_utf8(take(o, nl)?)
            .map_err(|_| e())?
            .to_string();
        o += nl;
        let reporter_key = take(o, 32)?.try_into().expect("32");
        o += 32;
        let sig = take(o, 64)?.try_into().expect("64");
        if b.len() != o + 64 {
            return Err(e());
        }
        Ok(Self {
            community_ref,
            post,
            reason_code,
            note,
            reporter_key,
            sig,
        })
    }

    /// Signature check plus key separation against the community roster.
    pub fn verify<'a>(&self, roster: impl IntoIterator<Item = &'a [u8; 32]>) -> bool {
        reporter_key_allowed(&self.reporter_key, roster)
            && VerifyingKey::from_bytes(&self.reporter_key)
                .and_then(|vk| vk.verify_strict(&self.body(), &Signature::from_bytes(&self.sig)))
                .is_ok()
    }
}

//! Private invites from a community into a private group (PROTOCOL §20.7).
//!
//! Pieces, all pure and transport-free:
//! - [`InviteKey`] / [`InvitePk`]: the per-community hybrid invite key
//!   (X25519 + ML-KEM-768) published as profile field `0x10` (1,228 B).
//! - [`InviteKeyring`]: per-community setting (default off), 30-day rotation
//!   with a 7-day overlap, at most 16 keys tried per unknown envelope.
//! - [`seal`] / [`open`]: the hybrid HPKE-style seal (HKDF-SHA384,
//!   ChaCha20-Poly1305). Not RFC 9180 wire format (same as `rendezvous::sealed`).
//! - [`elligator`]: Elligator2 representatives for "dirty" ephemeral keys.
//! - [`build_envelope`] / [`try_open_envelope`]: the outer `EnvelopeV1`, built
//!   like a 1:1 envelope of the same bucket (same TLVs, flags, expiry, and a
//!   ciphertext framed like an MLS `PrivateMessage` of the same size).
//! - [`InviteOffer`] / [`InviteReply`] / [`InviteWelcome`] (types 1/2/3).
//! - [`SenderLimits`] / [`ReceiverLimits`]: the item-6 rate limits.
//! - [`offer_report`]: an offer report carries only the signed offer.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use ml_kem::kem::{Decapsulate, Encapsulate, Kem, KeyExport, TryKeyInit};
use ml_kem::{EncapsulationKey, MlKem768};
use sha2::Sha384;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::{OmcpError, Result};
use crate::framing::flags;
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::budget::DEFAULT_EXPIRY_MS;
use crate::sync::compress::bucket_for;
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{put_subtags_in_aad, put_tag_in_aad, MailboxTag};
use crate::EnvelopeV1;

/// Profile field key carrying `invite_pk` (§20.7(1)).
pub const PROFILE_FIELD_INVITE_PK: u8 = 0x10;
pub const MLKEM_PK_LEN: usize = 1184;
pub const MLKEM_CT_LEN: usize = 1088;
/// Encoded [`InvitePk`]: 8 + 32 + 1,184 + 4.
pub const INVITE_PK_LEN: usize = 8 + 32 + MLKEM_PK_LEN + 4;
/// HPKE `enc`: X25519 ephemeral public key + ML-KEM-768 ciphertext.
pub const ENC_LEN: usize = 32 + MLKEM_CT_LEN;
const TAG_LEN: usize = 16;

pub const DAY_S: u64 = 86_400;
/// Keys rotate at least every 30 days (§20.7(1)).
pub const ROTATE_S: u64 = 30 * DAY_S;
/// Previous private keys are kept this long, then erased.
pub const OVERLAP_S: u64 = 7 * DAY_S;
/// Receivers try at most this many invite keys per unknown envelope.
pub const MAX_KEYS_TRIED: usize = 16;
/// Default offer lifetime (matches the 72 h store expiry).
pub const OFFER_TTL_S: u64 = 72 * 3600;
pub const MAX_INVITATION_TEXT: usize = 280;
pub const MAX_GROUP_NAME: usize = 64;

pub const INFO_INVITE: &[u8] = b"OMCP-invite-v1\0";
pub const INFO_REPLY: &[u8] = b"OMCP-invite-reply-v1\0";
const INFO_TAG: &[u8] = b"OMCP-inv-tag-v1\0";
const OFFER_SIG_CTX: &[u8] = b"OMCP-invite-offer-v1\0";
/// Same prefix the 1:1 path binds into the MLS authenticated data.
pub const MESH_AAD_PREFIX: &[u8] = b"OMCP-mesh-v1";

fn err(s: &'static str) -> OmcpError {
    OmcpError::Invite(s)
}

fn rand_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("getrandom");
    b
}

// ---------------------------------------------------------------- keys

/// Public half of an invite key: profile field `0x10`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InvitePk {
    pub key_id: [u8; 8],
    pub x25519_pk: [u8; 32],
    pub mlkem_pk: Vec<u8>,
    pub not_after: u32,
}

impl InvitePk {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(INVITE_PK_LEN);
        o.extend_from_slice(&self.key_id);
        o.extend_from_slice(&self.x25519_pk);
        o.extend_from_slice(&self.mlkem_pk);
        o.extend_from_slice(&self.not_after.to_be_bytes());
        o
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != INVITE_PK_LEN {
            return Err(err("invite_pk length"));
        }
        Ok(Self {
            key_id: b[..8].try_into().unwrap(),
            x25519_pk: b[8..40].try_into().unwrap(),
            mlkem_pk: b[40..40 + MLKEM_PK_LEN].to_vec(),
            not_after: u32::from_be_bytes(b[INVITE_PK_LEN - 4..].try_into().unwrap()),
        })
    }
}

/// Private invite key (X25519 secret + ML-KEM-768 seed). Also used for the
/// one-time `reply_pk` the inviter puts in an offer.
#[derive(Clone)]
pub struct InviteKey {
    pub key_id: [u8; 8],
    x_sk: [u8; 32],
    mlkem_seed: [u8; 64],
    pub created: u64,
    pub not_after: u32,
}

impl std::fmt::Debug for InviteKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "InviteKey({:02x?})", self.key_id)
    }
}

impl InviteKey {
    pub fn generate(now_s: u64, lifetime_s: u64) -> Self {
        Self {
            key_id: rand_bytes(),
            x_sk: rand_bytes(),
            mlkem_seed: rand_bytes(),
            created: now_s,
            not_after: u32::try_from(now_s + lifetime_s).unwrap_or(u32::MAX),
        }
    }
    fn dk(&self) -> <MlKem768 as Kem>::DecapsulationKey {
        let seed = ml_kem::Seed::try_from(&self.mlkem_seed[..]).expect("64");
        <MlKem768 as Kem>::DecapsulationKey::from_seed(seed)
    }
    pub fn public(&self) -> InvitePk {
        let ek = self.dk().encapsulation_key().to_bytes();
        InvitePk {
            key_id: self.key_id,
            x25519_pk: PublicKey::from(&StaticSecret::from(self.x_sk)).to_bytes(),
            mlkem_pk: ek.as_slice().to_vec(),
            not_after: self.not_after,
        }
    }
    fn x_dh(&self, peer: &[u8; 32]) -> [u8; 32] {
        StaticSecret::from(self.x_sk)
            .diffie_hellman(&PublicKey::from(*peer))
            .to_bytes()
    }
    /// Persisted form: key_id 8 | x_sk 32 | seed 64 | created u64 | not_after u32.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(116);
        o.extend_from_slice(&self.key_id);
        o.extend_from_slice(&self.x_sk);
        o.extend_from_slice(&self.mlkem_seed);
        o.extend_from_slice(&self.created.to_be_bytes());
        o.extend_from_slice(&self.not_after.to_be_bytes());
        o
    }
    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        if b.len() != 116 {
            return Err(err("invite key length"));
        }
        Ok(Self {
            key_id: b[..8].try_into().unwrap(),
            x_sk: b[8..40].try_into().unwrap(),
            mlkem_seed: b[40..104].try_into().unwrap(),
            created: u64::from_be_bytes(b[104..112].try_into().unwrap()),
            not_after: u32::from_be_bytes(b[112..116].try_into().unwrap()),
        })
    }
}

/// Per-community invite keys and the "Invites from people who aren't
/// contacts" setting (default off for every community).
#[derive(Default, Clone, Debug)]
pub struct InviteKeyring {
    enabled: BTreeSet<[u8; 32]>,
    /// Newest key last. Keys of a disabled community stay only for the overlap.
    keys: BTreeMap<[u8; 32], Vec<InviteKey>>,
    /// When a community's setting was turned off (erase after the overlap).
    disabled_at: BTreeMap<[u8; 32], u64>,
}

impl InviteKeyring {
    pub fn is_enabled(&self, community: &[u8; 32]) -> bool {
        self.enabled.contains(community)
    }

    pub fn set_enabled(&mut self, community: [u8; 32], on: bool, now_s: u64) {
        if on {
            self.enabled.insert(community);
            self.disabled_at.remove(&community);
            self.rotate(now_s);
        } else if self.enabled.remove(&community) {
            self.disabled_at.insert(community, now_s);
        }
    }

    /// Create / rotate keys and erase expired ones. Idempotent.
    pub fn rotate(&mut self, now_s: u64) {
        for c in self.enabled.clone() {
            let v = self.keys.entry(c).or_default();
            if v.last().is_none_or(|k| now_s >= k.created + ROTATE_S) {
                v.push(InviteKey::generate(now_s, ROTATE_S + OVERLAP_S));
            }
        }
        let disabled = self.disabled_at.clone();
        for (c, v) in self.keys.iter_mut() {
            if let Some(off) = disabled.get(c) {
                if now_s >= off + OVERLAP_S {
                    v.clear();
                }
            }
            // A key superseded by a newer one is kept for the overlap only.
            if let Some(newest) = v.last().map(|k| k.created) {
                v.retain(|k| k.created == newest || now_s < newest + OVERLAP_S);
            }
        }
        self.keys.retain(|_, v| !v.is_empty());
        self.disabled_at
            .retain(|c, off| now_s < *off + OVERLAP_S && self.keys.contains_key(c));
    }

    /// The `invite_pk` to publish, present only while the setting is on.
    pub fn published(&self, community: &[u8; 32]) -> Option<InvitePk> {
        if !self.is_enabled(community) {
            return None;
        }
        self.keys.get(community)?.last().map(InviteKey::public)
    }

    /// Keys to try on an unknown envelope (newest first, capped at 16).
    pub fn candidate_keys(&self) -> Vec<(&[u8; 32], &InviteKey)> {
        let mut all: Vec<_> = self
            .keys
            .iter()
            .flat_map(|(c, v)| v.iter().map(move |k| (c, k)))
            .collect();
        all.sort_by_key(|k| std::cmp::Reverse(k.1.created));
        all.truncate(MAX_KEYS_TRIED);
        all
    }

    pub fn key_count(&self) -> usize {
        self.keys.values().map(Vec::len).sum()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(&(self.enabled.len() as u32).to_be_bytes());
        for c in &self.enabled {
            o.extend_from_slice(c);
        }
        o.extend_from_slice(&(self.disabled_at.len() as u32).to_be_bytes());
        for (c, t) in &self.disabled_at {
            o.extend_from_slice(c);
            o.extend_from_slice(&t.to_be_bytes());
        }
        let n: usize = self.key_count();
        o.extend_from_slice(&(n as u32).to_be_bytes());
        for (c, v) in &self.keys {
            for k in v {
                o.extend_from_slice(c);
                o.extend_from_slice(&k.to_bytes());
            }
        }
        o
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        let mut r = Reader(b);
        let mut s = Self::default();
        for _ in 0..r.u32()? {
            s.enabled.insert(r.arr()?);
        }
        for _ in 0..r.u32()? {
            let c = r.arr()?;
            s.disabled_at.insert(c, r.u64()?);
        }
        for _ in 0..r.u32()? {
            let c: [u8; 32] = r.arr()?;
            let k = InviteKey::from_bytes(r.take(116)?)?;
            s.keys.entry(c).or_default().push(k);
        }
        for v in s.keys.values_mut() {
            v.sort_by_key(|k| k.created);
        }
        Ok(s)
    }
}

// ---------------------------------------------------------------- seal

fn kdf(enc: &[u8], ikm: &[u8], info: &[u8]) -> ([u8; 32], [u8; 12]) {
    let hk = Hkdf::<Sha384>::new(Some(enc), ikm);
    let mut okm = [0u8; 44];
    hk.expand(info, &mut okm).expect("44 <= 255*48");
    (okm[..32].try_into().unwrap(), okm[32..].try_into().unwrap())
}

/// Hybrid seal to `pk` (base mode, `aad = ""`): `enc (1,120 B) || AEAD`.
pub fn seal(pk: &InvitePk, info: &[u8], pt: &[u8]) -> Result<Vec<u8>> {
    let eph = StaticSecret::from(rand_bytes::<32>());
    let eph_pub = PublicKey::from(&eph).to_bytes();
    let ss_x = eph
        .diffie_hellman(&PublicKey::from(pk.x25519_pk))
        .to_bytes();
    let ek =
        EncapsulationKey::<MlKem768>::new_from_slice(&pk.mlkem_pk).map_err(|_| err("mlkem pk"))?;
    let (ct, ss_k) = ek.encapsulate();
    let mut enc = eph_pub.to_vec();
    enc.extend_from_slice(ct.as_slice());
    let mut ikm = ss_x.to_vec();
    ikm.extend_from_slice(ss_k.as_slice());
    let (key, nonce) = kdf(&enc, &ikm, info);
    let c = ChaCha20Poly1305::new_from_slice(&key).map_err(|_| err("key"))?;
    let body = c
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: pt, aad: b"" })
        .map_err(|_| err("seal"))?;
    enc.extend_from_slice(&body);
    Ok(enc)
}

/// Open a [`seal`] output with `key`.
pub fn open(key: &InviteKey, info: &[u8], sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < ENC_LEN + TAG_LEN {
        return Err(err("sealed too short"));
    }
    let (enc, body) = sealed.split_at(ENC_LEN);
    let ss_x = key.x_dh(enc[..32].try_into().unwrap());
    let ct: ml_kem::kem::Ciphertext<MlKem768> =
        enc[32..].try_into().map_err(|_| err("mlkem ct"))?;
    let ss_k = key.dk().decapsulate(&ct);
    let mut ikm = ss_x.to_vec();
    ikm.extend_from_slice(ss_k.as_slice());
    let (k, nonce) = kdf(enc, &ikm, info);
    let c = ChaCha20Poly1305::new_from_slice(&k).map_err(|_| err("key"))?;
    c.decrypt(
        Nonce::from_slice(&nonce),
        Payload {
            msg: body,
            aad: b"",
        },
    )
    .map_err(|_| err("open"))
}

pub fn invite_info(key_id: &[u8; 8]) -> Vec<u8> {
    let mut i = INFO_INVITE.to_vec();
    i.extend_from_slice(key_id);
    i
}

// ----------------------------------------------------------- elligator2

/// Elligator2 over Curve25519 (RFC 9380 map, Z = 2) for ephemeral keys.
pub mod elligator {
    use curve25519_dalek::constants::EIGHT_TORSION;
    use curve25519_dalek::edwards::EdwardsPoint;
    use num_bigint::BigUint;

    fn p() -> BigUint {
        (BigUint::from(1u8) << 255u32) - BigUint::from(19u8)
    }
    fn a() -> BigUint {
        BigUint::from(486_662u32)
    }
    fn le(x: &BigUint) -> [u8; 32] {
        let mut v = x.to_bytes_le();
        v.resize(32, 0);
        v.try_into().unwrap()
    }
    fn inv(x: &BigUint, p: &BigUint) -> BigUint {
        x.modpow(&(p - BigUint::from(2u8)), p)
    }
    fn is_square(x: &BigUint, p: &BigUint) -> bool {
        let e = (p - BigUint::from(1u8)) >> 1u32;
        let l = x.modpow(&e, p);
        l == BigUint::from(0u8) || l == BigUint::from(1u8)
    }
    /// Square root mod p (p ≡ 5 mod 8), the smaller of the two roots.
    fn sqrt(x: &BigUint, p: &BigUint) -> Option<BigUint> {
        let mut r = x.modpow(&((p + BigUint::from(3u8)) >> 3u32), p);
        if (&r * &r) % p != *x {
            let i = BigUint::from(2u8).modpow(&((p - BigUint::from(1u8)) >> 2u32), p);
            r = (r * i) % p;
            if (&r * &r) % p != *x {
                return None;
            }
        }
        let half = (p - BigUint::from(1u8)) >> 1u32;
        if r > half {
            r = p - r;
        }
        Some(r)
    }

    /// Forward map: representative → Montgomery u.
    pub fn map(rep: &[u8; 32]) -> [u8; 32] {
        let p = p();
        let mut b = *rep;
        b[31] &= 0x3f;
        let r = BigUint::from_bytes_le(&b) % &p;
        let one = BigUint::from(1u8);
        let den = (&one + BigUint::from(2u8) * &r * &r) % &p;
        let d = (&p - (a() * inv(&den, &p)) % &p) % &p;
        let f = (&d * &d * &d + a() * &d * &d + &d) % &p;
        let u = if is_square(&f, &p) {
            d
        } else {
            (&p + &p - a() - d) % &p
        };
        le(&u)
    }

    /// Inverse map: Montgomery u → representative, if one exists.
    /// The two unused top bits are filled from `rnd`.
    pub fn representative(u: &[u8; 32], rnd: u8) -> Option<[u8; 32]> {
        let p = p();
        let u = BigUint::from_bytes_le(u) % &p;
        let upa = (&u + a()) % &p;
        if u == BigUint::from(0u8) || upa == BigUint::from(0u8) {
            return None;
        }
        // r² = -u / (2(u + A))
        let num = (&p - &u) % &p;
        let den = (BigUint::from(2u8) * &upa) % &p;
        let r2 = (num * inv(&den, &p)) % &p;
        let r = sqrt(&r2, &p)?;
        let mut out = le(&r);
        if map(&out) != le(&u) {
            return None;
        }
        out[31] |= rnd & 0xc0;
        Some(out)
    }

    /// Fresh ephemeral key whose public point has a representative: the
    /// public point is `e·B + T` (T a random 8-torsion point), so X25519 with
    /// the clamped scalar `e` still gives the right shared secret.
    pub fn ephemeral() -> ([u8; 32], [u8; 32]) {
        loop {
            let e = super::rand_bytes::<32>();
            let r = super::rand_bytes::<2>();
            let pt: EdwardsPoint =
                EdwardsPoint::mul_base_clamped(e) + EIGHT_TORSION[(r[0] & 7) as usize];
            let u = pt.to_montgomery().to_bytes();
            if let Some(rep) = representative(&u, r[1]) {
                return (e, rep);
            }
        }
    }
}

/// `t = HKDF-SHA-384(salt = key_id, ikm = s, info = "OMCP-inv-tag-v1\0", 16)`.
pub fn derive_tags(key_id: &[u8; 8], shared: &[u8; 32]) -> (MailboxTag, MailboxTag) {
    let hk = Hkdf::<Sha384>::new(Some(key_id), shared);
    let mut t = [0u8; 16];
    hk.expand(INFO_TAG, &mut t).expect("16");
    (t[..8].try_into().unwrap(), t[8..].try_into().unwrap())
}

// ------------------------------------------------------------- envelope

fn put_varint(o: &mut Vec<u8>, v: usize) {
    if v < 64 {
        o.push(v as u8);
    } else if v < 16_384 {
        o.extend_from_slice(&(0x4000u16 | v as u16).to_be_bytes());
    } else {
        o.extend_from_slice(&(0x8000_0000u32 | v as u32).to_be_bytes());
    }
}

fn get_varint(b: &[u8]) -> Option<(usize, usize)> {
    let f = *b.first()?;
    match f >> 6 {
        0 => Some((f as usize, 1)),
        1 => Some(((u16::from_be_bytes([f & 0x3f, *b.get(1)?])) as usize, 2)),
        2 => {
            let v = u32::from_be_bytes([f & 0x3f, *b.get(1)?, *b.get(2)?, *b.get(3)?]);
            Some((v as usize, 4))
        }
        _ => None,
    }
}

/// MLS `PrivateMessage` overhead in the 1:1 path: the inner ciphertext field
/// of a 1:1 message whose padded body fills bucket `b` is `b + 128` bytes
/// (body `b - 16` + framing/signature/AEAD 144). Measured against OpenMLS
/// 0.9.2 in `tests_invite::mls_shape_matches_real_1to1`.
pub const MLS_CT_OVERHEAD: usize = 128;
/// `encrypted_sender_data` length in a 1:1 `PrivateMessage`.
pub const MLS_ESD_LEN: usize = 28;
const MLS_HEAD: [u8; 5] = [0x00, 0x01, 0x00, 0x02, 0x20];

/// The MLS authenticated data a 1:1 envelope binds (same as the FFI path).
pub fn mesh_aad(env: &EnvelopeV1) -> Vec<u8> {
    let mut a = MESH_AAD_PREFIX.to_vec();
    a.extend_from_slice(&env.version.to_be_bytes());
    a.extend_from_slice(&env.suite_id.to_be_bytes());
    a.extend_from_slice(&env.envelope_id);
    a.extend_from_slice(&env.created_ms.to_be_bytes());
    a.extend_from_slice(&env.flags.to_be_bytes());
    a.extend_from_slice(&env.header_aad);
    a
}

/// Capacity for the padded invite payload in bucket `b`.
fn capacity(b: usize) -> usize {
    b + MLS_CT_OVERHEAD - ENC_LEN - TAG_LEN
}

/// Smallest §22.3 bucket whose capacity fits `msg_len` (+4 length prefix).
pub fn bucket_for_payload(msg_len: usize) -> usize {
    let need = msg_len + 4;
    let mut b = bucket_for(need.saturating_sub(MLS_CT_OVERHEAD) + ENC_LEN + TAG_LEN);
    while capacity(b) < need {
        b = bucket_for(b + 1);
    }
    b
}

/// Build an invite envelope to `to` (an `invite_pk` or a one-time `reply_pk`)
/// with the given payload, padded to bucket `bucket` (≥ the payload's own).
#[allow(clippy::too_many_arguments)]
pub fn build_envelope(
    to: &InvitePk,
    info: &[u8],
    msg: &[u8],
    bucket: usize,
    envelope_id: [u8; 16],
    created_ms: u64,
    ttl_hops: u8,
    epoch: u64,
) -> Result<EnvelopeV1> {
    let bucket = bucket.max(bucket_for_payload(msg.len()));
    let cap = capacity(bucket);
    let mut pt = Vec::with_capacity(cap);
    pt.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    pt.extend_from_slice(msg);
    pt.resize(cap, 0);

    let (e, rep) = elligator::ephemeral();
    let s = StaticSecret::from(e)
        .diffie_hellman(&PublicKey::from(to.x25519_pk))
        .to_bytes();
    let (tag, sub) = derive_tags(&to.key_id, &s);
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &tag);
    put_subtags_in_aad(&mut header_aad, &[sub])?;
    put_expires_in_aad(
        &mut header_aad,
        created_ms.saturating_add(DEFAULT_EXPIRY_MS),
    );
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
    let sealed = seal(to, info, &pt)?;
    debug_assert_eq!(sealed.len(), bucket + MLS_CT_OVERHEAD);
    let aad = mesh_aad(&env);
    let mut c = MLS_HEAD.to_vec();
    c.extend_from_slice(&rep);
    c.extend_from_slice(&epoch.to_be_bytes());
    c.push(0x01);
    put_varint(&mut c, aad.len());
    c.extend_from_slice(&aad);
    put_varint(&mut c, MLS_ESD_LEN);
    c.extend_from_slice(&rand_bytes::<MLS_ESD_LEN>());
    put_varint(&mut c, sealed.len());
    c.extend_from_slice(&sealed);
    env.ciphertext = c;
    Ok(env)
}

/// Pull `(rep, sealed)` out of an MLS-shaped ciphertext.
fn split_ct(c: &[u8]) -> Option<([u8; 32], &[u8])> {
    if c.len() < 47 || c[..5] != MLS_HEAD {
        return None;
    }
    let rep: [u8; 32] = c[5..37].try_into().ok()?;
    let mut i = 46;
    for _ in 0..2 {
        let (n, w) = get_varint(&c[i..])?;
        i += w + n;
    }
    let (n, w) = get_varint(c.get(i..)?)?;
    let body = c.get(i + w..i + w + n)?;
    Some((rep, body))
}

/// Cheap recognition (one X25519 per key, ≤ 16 keys): index of the key
/// whose derived mailbox tag matches the envelope's, if any.
pub fn recognise(env: &EnvelopeV1, keys: &[InviteKey]) -> Option<usize> {
    let tag = crate::sync::tag::tag_from_aad(&env.header_aad).ok()??;
    let (rep, _) = split_ct(&env.ciphertext)?;
    let u = elligator::map(&rep);
    keys.iter()
        .take(MAX_KEYS_TRIED)
        .position(|k| derive_tags(&k.key_id, &k.x_dh(&u)).0 == tag)
}

/// Try `keys` (at most [`MAX_KEYS_TRIED`]) against an envelope whose
/// mailbox tag is unknown. Returns the index of the matching key and the
/// opened payload.
pub fn try_open_envelope(
    env: &EnvelopeV1,
    keys: &[&InviteKey],
    info_for: impl Fn(&InviteKey) -> Vec<u8>,
) -> Option<(usize, Vec<u8>)> {
    let tag = crate::sync::tag::tag_from_aad(&env.header_aad).ok()??;
    let (rep, sealed) = split_ct(&env.ciphertext)?;
    let u = elligator::map(&rep);
    for (i, k) in keys.iter().take(MAX_KEYS_TRIED).enumerate() {
        let (t, _) = derive_tags(&k.key_id, &k.x_dh(&u));
        if t != tag {
            continue;
        }
        let pt = open(k, &info_for(k), sealed).ok()?;
        let n = u32::from_be_bytes(pt.get(..4)?.try_into().ok()?) as usize;
        return Some((i, pt.get(4..4 + n)?.to_vec()));
    }
    None
}

// ------------------------------------------------------------- messages

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.0.len() < n {
            return Err(err("truncated"));
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.arr()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.arr()?))
    }
    fn bytes16(&mut self) -> Result<&'a [u8]> {
        let n = self.u16()? as usize;
        self.take(n)
    }
    fn bytes32(&mut self) -> Result<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

pub const TYPE_OFFER: u8 = 0x01;
pub const TYPE_REPLY: u8 = 0x02;
pub const TYPE_WELCOME: u8 = 0x03;

/// `0x01 InviteOffer` (§20.7(3a)). No group ID, ref, members or MLS material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteOffer {
    pub offer_id: [u8; 16],
    pub reply_pk: InvitePk,
    pub inviter_pseudonym_pk: [u8; 32],
    pub invitation_text: String,
    pub group_name: String,
    pub group_avatar_hash: Option<[u8; 32]>,
    pub expires: u32,
    pub sig: [u8; 64],
}

impl InviteOffer {
    fn signed_part(&self) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(&self.offer_id);
        o.extend_from_slice(&self.reply_pk.encode());
        o.extend_from_slice(&self.inviter_pseudonym_pk);
        o.extend_from_slice(&(self.invitation_text.len() as u16).to_be_bytes());
        o.extend_from_slice(self.invitation_text.as_bytes());
        o.extend_from_slice(&(self.group_name.len() as u16).to_be_bytes());
        o.extend_from_slice(self.group_name.as_bytes());
        match &self.group_avatar_hash {
            Some(h) => {
                o.push(1);
                o.extend_from_slice(h);
            }
            None => o.push(0),
        }
        o.extend_from_slice(&self.expires.to_be_bytes());
        o
    }

    /// Build and sign an offer with the inviter's community pseudonym key.
    pub fn new_signed(
        key: &SigningKey,
        reply_pk: InvitePk,
        text: &str,
        group_name: &str,
        group_avatar_hash: Option<[u8; 32]>,
        now_s: u64,
    ) -> Result<Self> {
        if text.len() > MAX_INVITATION_TEXT || group_name.len() > MAX_GROUP_NAME {
            return Err(err("text too long"));
        }
        let mut o = Self {
            offer_id: rand_bytes(),
            reply_pk,
            inviter_pseudonym_pk: key.verifying_key().to_bytes(),
            invitation_text: text.into(),
            group_name: group_name.into(),
            group_avatar_hash,
            expires: u32::try_from(now_s + OFFER_TTL_S).unwrap_or(u32::MAX),
            sig: [0; 64],
        };
        let mut m = OFFER_SIG_CTX.to_vec();
        m.extend_from_slice(&o.signed_part());
        o.sig = key.sign(&m).to_bytes();
        Ok(o)
    }

    pub fn verify(&self) -> Result<()> {
        let vk = VerifyingKey::from_bytes(&self.inviter_pseudonym_pk)
            .map_err(|_| err("pseudonym key"))?;
        let mut m = OFFER_SIG_CTX.to_vec();
        m.extend_from_slice(&self.signed_part());
        vk.verify(&m, &Signature::from_bytes(&self.sig))
            .map_err(|_| err("offer signature"))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut o = vec![TYPE_OFFER];
        o.extend_from_slice(&self.signed_part());
        o.extend_from_slice(&self.sig);
        o
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader(b);
        if r.u8()? != TYPE_OFFER {
            return Err(err("not an offer"));
        }
        let offer_id = r.arr()?;
        let reply_pk = InvitePk::decode(r.take(INVITE_PK_LEN)?)?;
        let inviter_pseudonym_pk = r.arr()?;
        let text = r.bytes16()?;
        let name = r.bytes16()?;
        if text.len() > MAX_INVITATION_TEXT || name.len() > MAX_GROUP_NAME {
            return Err(err("text too long"));
        }
        let invitation_text = String::from_utf8(text.to_vec()).map_err(|_| err("utf8"))?;
        let group_name = String::from_utf8(name.to_vec()).map_err(|_| err("utf8"))?;
        let group_avatar_hash = match r.u8()? {
            0 => None,
            1 => Some(r.arr()?),
            _ => return Err(err("avatar flag")),
        };
        let expires = r.u32()?;
        let sig = r.arr()?;
        if !r.0.is_empty() {
            return Err(err("trailing bytes"));
        }
        let o = Self {
            offer_id,
            reply_pk,
            inviter_pseudonym_pk,
            invitation_text,
            group_name,
            group_avatar_hash,
            expires,
            sig,
        };
        o.verify()?;
        Ok(o)
    }
}

pub const DECISION_ACCEPT: u8 = 1;
pub const DECISION_DECLINE: u8 = 2;

/// `0x02 InviteReply` (§20.7(3b)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteReply {
    pub offer_id: [u8; 16],
    pub decision: u8,
    pub key_package: Vec<u8>,
    /// The invitee's `invite_pk` the Welcome is sealed to (accept only).
    pub welcome_pk: Option<InvitePk>,
}

/// Both decisions are padded to this bucket (the accept size, §20.7(3b)).
pub const REPLY_BUCKET: usize = 4096;

impl InviteReply {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = vec![TYPE_REPLY];
        o.extend_from_slice(&self.offer_id);
        o.push(self.decision);
        o.extend_from_slice(&(self.key_package.len() as u32).to_be_bytes());
        o.extend_from_slice(&self.key_package);
        match &self.welcome_pk {
            Some(pk) => {
                o.push(1);
                o.extend_from_slice(&pk.encode());
            }
            None => o.push(0),
        }
        o
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader(b);
        if r.u8()? != TYPE_REPLY {
            return Err(err("not a reply"));
        }
        let offer_id = r.arr()?;
        let decision = r.u8()?;
        let key_package = r.bytes32()?.to_vec();
        let welcome_pk = match r.u8()? {
            0 => None,
            1 => Some(InvitePk::decode(r.take(INVITE_PK_LEN)?)?),
            _ => return Err(err("welcome pk flag")),
        };
        if !matches!(decision, DECISION_ACCEPT | DECISION_DECLINE)
            || (decision == DECISION_DECLINE) != key_package.is_empty()
        {
            return Err(err("decision"));
        }
        Ok(Self {
            offer_id,
            decision,
            key_package,
            welcome_pk,
        })
    }
    /// Padded bucket for a reply: accept and decline share it.
    pub fn bucket(accept_len: usize) -> usize {
        REPLY_BUCKET.max(bucket_for_payload(accept_len))
    }
}

/// `0x03 InviteWelcome` (§20.7(3c)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteWelcome {
    pub offer_id: [u8; 16],
    pub welcome: Vec<u8>,
}

impl InviteWelcome {
    pub fn encode(&self) -> Vec<u8> {
        let mut o = vec![TYPE_WELCOME];
        o.extend_from_slice(&self.offer_id);
        o.extend_from_slice(&(self.welcome.len() as u32).to_be_bytes());
        o.extend_from_slice(&self.welcome);
        o
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut r = Reader(b);
        if r.u8()? != TYPE_WELCOME {
            return Err(err("not a welcome"));
        }
        Ok(Self {
            offer_id: r.arr()?,
            welcome: r.bytes32()?.to_vec(),
        })
    }
}

// --------------------------------------------------------- rate limits

/// Inviter side (§20.7(6), MUST): ≤ 10 offers/h and ≤ 30/day in total,
/// ≤ 1 open offer per invitee `invite_pk`, 7-day block after decline/expiry.
#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SenderLimits {
    sent: Vec<u64>,
    open: HashMap<[u8; 8], u64>,
    blocked_until: HashMap<[u8; 8], u64>,
}

impl SenderLimits {
    pub fn check(&self, invitee_key_id: &[u8; 8], now_s: u64) -> Result<()> {
        let h = self.sent.iter().filter(|t| now_s < **t + 3600).count();
        let d = self.sent.iter().filter(|t| now_s < **t + DAY_S).count();
        if h >= 10 || d >= 30 {
            return Err(err("rate limit"));
        }
        if self
            .open
            .get(invitee_key_id)
            .is_some_and(|exp| now_s < *exp)
        {
            return Err(err("offer already open"));
        }
        if self
            .blocked_until
            .get(invitee_key_id)
            .is_some_and(|t| now_s < *t)
        {
            return Err(err("repeat within 7 days"));
        }
        Ok(())
    }
    pub fn record_sent(&mut self, invitee_key_id: [u8; 8], expires_s: u64, now_s: u64) {
        self.sent.retain(|t| now_s < *t + DAY_S);
        self.sent.push(now_s);
        self.open.insert(invitee_key_id, expires_s);
    }
    /// A declined or expired offer cannot be repeated for 7 days.
    pub fn record_closed(
        &mut self,
        invitee_key_id: [u8; 8],
        declined_or_expired: bool,
        now_s: u64,
    ) {
        self.open.remove(&invitee_key_id);
        if declined_or_expired {
            self.blocked_until.insert(invitee_key_id, now_s + 7 * DAY_S);
        }
    }
    /// Expire open offers (they then count as expired).
    pub fn tick(&mut self, now_s: u64) {
        let expired: Vec<_> = self
            .open
            .iter()
            .filter(|(_, e)| now_s >= **e)
            .map(|(k, _)| *k)
            .collect();
        for k in expired {
            self.record_closed(k, true, now_s);
        }
    }
}

/// Invitee side (§20.7(6), MUST): ≤ 3 offers per inviter pseudonym per 7 days,
/// ≤ 20 per day in total; blocked pseudonyms dropped silently.
#[derive(Default, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ReceiverLimits {
    shown: Vec<(u64, [u8; 32])>,
    pub blocked: BTreeSet<[u8; 32]>,
}

impl ReceiverLimits {
    /// Should this offer be shown? Records it when yes.
    pub fn admit(&mut self, inviter: &[u8; 32], now_s: u64) -> bool {
        if self.blocked.contains(inviter) {
            return false;
        }
        self.shown.retain(|(t, _)| now_s < t + 7 * DAY_S);
        let per = self.shown.iter().filter(|(_, p)| p == inviter).count();
        let day = self.shown.iter().filter(|(t, _)| now_s < t + DAY_S).count();
        if per >= 3 || day >= 20 {
            return false;
        }
        self.shown.push((now_s, *inviter));
        true
    }
    pub fn block(&mut self, inviter: [u8; 32]) {
        self.blocked.insert(inviter);
    }
}

// --------------------------------------------------------------- report

/// The only content of an offer report (§20.7(6), MR38): the signed offer.
pub fn offer_report(offer: &InviteOffer) -> Vec<u8> {
    offer.encode()
}

#[cfg(test)]
#[path = "tests_invite.rs"]
mod tests_invite;

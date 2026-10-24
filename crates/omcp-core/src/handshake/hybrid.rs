//! Noise_IK + ML-KEM-768 hybrid handshake for suite `OMCP-2026-A`.
//!
//! # Normative construction — `OMCP-Noise-Hybrid-v1` (PROTOCOL §21.5)
//!
//! Pattern **Noise_IK** with classical DH **X25519**, AEAD **ChaCha20-Poly1305**,
//! hash **BLAKE2b**, plus ML-KEM-768 (FIPS 203) mixed into the Noise chaining key.
//!
//! 1. Classical `Noise_IK_25519_ChaChaPoly_BLAKE2b` (initiator knows responder's
//!    static X25519 public key).
//! 2. **ML-KEM transcript binding (§21.5):** the initiator's static ML-KEM-768
//!    encapsulation key is sent in message 1 (cleartext) and `MixHash`'d; the
//!    responder encapsulates to it and returns the ciphertext in message 2
//!    (cleartext) and `MixHash`'d.
//! 3. After classical Noise_IK DH `MixKey` steps have updated `ck`, both sides:
//!    ```text
//!    ck' = HKDF-SHA-384(
//!            salt = ck,
//!            ikm  = mlkem_shared_secret,   // 32-byte ML-KEM-768 ss
//!            info = "OMCP-Noise-Hybrid-v1",
//!            L    = 32
//!          )
//!    ck ← ck'
//!    ```
//!    Then Noise Split / transport derivation continues from `ck` as usual.
//!
//! **Wire compatibility:** OMCP-defined only — **not** wire-compatible with snow
//! HFS / `Noise_*hfs_*` names or libp2p `/noise-mlkem768-hfs/*`.

use hkdf::Hkdf;
use ml_kem::kem::{Ciphertext, Decapsulate, Encapsulate, Kem, KeyExport};
use ml_kem::{EncapsulationKey, MlKem768};
use sha2::Sha384;
use x25519_dalek::{PublicKey as X25519Public, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::symmetric::{sealed_dh_len, split_transport_keys, SymmetricState, DHLEN, HASHLEN};
use crate::error::{OmcpError, Result};

/// Protocol name mixed into the Noise prologue hash.
///
/// Not a registered Noise name (hybrid is outside stock Noise). Kept stable for M1 vectors.
pub const HYBRID_PROTOCOL_NAME: &[u8] = b"OMCP_IK_25519+MLKEM768_ChaChaPoly_BLAKE2b";

/// HKDF info label for the hybrid chaining-key mix (PROTOCOL §21.5).
pub const HYBRID_HKDF_INFO: &[u8] = b"OMCP-Noise-Hybrid-v1";

/// ML-KEM-768 encapsulation key size (FIPS 203).
pub const MLKEM768_EK_LEN: usize = 1184;
/// ML-KEM-768 ciphertext size (FIPS 203).
pub const MLKEM768_CT_LEN: usize = 1088;
/// ML-KEM-768 shared secret size.
pub const MLKEM768_SS_LEN: usize = 32;

fn random_32() -> Result<[u8; 32]> {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(b)
}

/// Static identity keys for one peer (X25519 + ML-KEM-768).
pub struct StaticIdentity {
    dh: StaticSecret,
    kem_dk: <MlKem768 as Kem>::DecapsulationKey,
    kem_ek_bytes: [u8; MLKEM768_EK_LEN],
    dh_public: [u8; DHLEN],
}

impl StaticIdentity {
    /// Generate a fresh static identity.
    pub fn generate() -> Result<Self> {
        let dh = StaticSecret::from(random_32()?);
        let dh_public = X25519Public::from(&dh).to_bytes();
        let (kem_dk, kem_ek) = MlKem768::generate_keypair();
        let ek_encoded = kem_ek.to_bytes();
        let mut kem_ek_bytes = [0u8; MLKEM768_EK_LEN];
        kem_ek_bytes.copy_from_slice(ek_encoded.as_slice());
        Ok(Self {
            dh,
            kem_dk,
            kem_ek_bytes,
            dh_public,
        })
    }

    /// Deterministically derive a static identity from a 32-byte master seed
    /// (e.g. recovery-phrase entropy, see [`crate::recovery`]).
    ///
    /// HKDF-SHA-384(salt = "OMCP-Identity-Seed-v1", ikm = seed) expands to
    /// 32 bytes X25519 secret (info "x25519") and the 64-byte FIPS 203 seed
    /// `d || z` (info "mlkem768").
    pub fn from_seed(seed: &[u8; 32]) -> Result<Self> {
        let hk = Hkdf::<Sha384>::new(Some(b"OMCP-Identity-Seed-v1"), seed);
        let mut dh_bytes = [0u8; 32];
        hk.expand(b"x25519", &mut dh_bytes)
            .map_err(|_| OmcpError::Crypto("hkdf"))?;
        let mut kem_seed = [0u8; 64];
        hk.expand(b"mlkem768", &mut kem_seed)
            .map_err(|_| OmcpError::Crypto("hkdf"))?;
        let dh = StaticSecret::from(dh_bytes);
        dh_bytes.zeroize();
        let dh_public = X25519Public::from(&dh).to_bytes();
        let kem_dk = <MlKem768 as Kem>::DecapsulationKey::from_seed(kem_seed.into());
        kem_seed.zeroize();
        let ek_encoded = kem_dk.encapsulation_key().to_bytes();
        let mut kem_ek_bytes = [0u8; MLKEM768_EK_LEN];
        kem_ek_bytes.copy_from_slice(ek_encoded.as_slice());
        Ok(Self {
            dh,
            kem_dk,
            kem_ek_bytes,
            dh_public,
        })
    }

    /// X25519 static public key bytes.
    pub fn dh_public(&self) -> [u8; DHLEN] {
        self.dh_public
    }

    /// ML-KEM-768 encapsulation key bytes (initiator identity KEM for §21.5).
    pub fn kem_public(&self) -> &[u8; MLKEM768_EK_LEN] {
        &self.kem_ek_bytes
    }
}

/// Completed hybrid session secrets (PROTOCOL §21.5 / §11 FS).
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct HybridSessionKeys {
    /// Initiator → responder transport key (32 bytes).
    pub k_i2r: [u8; 32],
    /// Responder → initiator transport key (32 bytes).
    pub k_r2i: [u8; 32],
    /// Noise handshake hash (public transcript commitment).
    #[zeroize(skip)]
    pub handshake_hash: [u8; HASHLEN],
}

/// PROTOCOL §21.5: mix ML-KEM ss into Noise `ck`, then Noise Split for transport keys.
fn derive_session(
    ck: &[u8; HASHLEN],
    mlkem_ss: &[u8; MLKEM768_SS_LEN],
    handshake_hash: &[u8; HASHLEN],
) -> Result<HybridSessionKeys> {
    // ck' = HKDF-SHA-384(salt=ck, ikm=mlkem_ss, info="OMCP-Noise-Hybrid-v1", L=32)
    let hk = Hkdf::<Sha384>::new(Some(ck.as_slice()), mlkem_ss);
    let mut ck_prime = [0u8; 32];
    hk.expand(HYBRID_HKDF_INFO, &mut ck_prime)
        .map_err(|_| OmcpError::Crypto("hkdf expand"))?;

    let (k_i2r, k_r2i) = split_transport_keys(&ck_prime);
    ck_prime.zeroize();

    Ok(HybridSessionKeys {
        k_i2r,
        k_r2i,
        handshake_hash: *handshake_hash,
    })
}

fn parse_kem_ek(bytes: &[u8; MLKEM768_EK_LEN]) -> Result<EncapsulationKey<MlKem768>> {
    use ml_kem::kem::TryKeyInit;
    EncapsulationKey::<MlKem768>::new_from_slice(bytes.as_slice())
        .map_err(|_| OmcpError::HandshakeMalformed("kem ek"))
}

fn dh(secret: &StaticSecret, public: &X25519Public) -> [u8; DHLEN] {
    secret.diffie_hellman(public).to_bytes()
}

/// Initiator state (knows responder static X25519 public; sends own ML-KEM ek).
pub struct HybridInitiator {
    sym: SymmetricState,
    e: StaticSecret,
    s: StaticIdentity,
    rs: X25519Public,
    msg1_sent: bool,
}

/// Responder state.
pub struct HybridResponder {
    sym: SymmetricState,
    s: StaticIdentity,
    e: Option<StaticSecret>,
    re: Option<X25519Public>,
    rs: Option<X25519Public>,
    /// Initiator's ML-KEM encapsulation key from msg1 (§21.5).
    initiator_kem_ek: Option<[u8; MLKEM768_EK_LEN]>,
    msg1_ok: bool,
}

impl HybridInitiator {
    /// Create initiator; `responder_dh` is the peer's static X25519 public.
    ///
    /// The initiator's static ML-KEM encapsulation key is sent in message 1
    /// (PROTOCOL §21.5). The responder's KEM key is not required for this handshake.
    pub fn new(static_id: StaticIdentity, responder_dh: &[u8; DHLEN]) -> Result<Self> {
        let mut sym = SymmetricState::initialize(HYBRID_PROTOCOL_NAME);
        sym.mix_hash(responder_dh);
        Ok(Self {
            sym,
            e: StaticSecret::from(random_32()?),
            s: static_id,
            rs: X25519Public::from(*responder_dh),
            msg1_sent: false,
        })
    }

    /// Write Noise IK message 1 + initiator ML-KEM encapsulation key.
    ///
    /// Layout: `e (32) || enc(s) (48) || mlkem_ek (1184) || enc(payload)`
    ///
    /// The EK is cleartext and `MixHash`'d before the sealed payload so tampering
    /// fails AEAD (PROTOCOL §21.5 transcript binding).
    pub fn write_message_1(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        self.write_message_1_with(|_| Ok(payload.to_vec()))
    }

    /// [`Self::write_message_1`] with a payload built from the handshake hash at
    /// the point the payload is sealed (device binding, PROTOCOL §3.4).
    pub fn write_message_1_with(
        &mut self,
        payload: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        if self.msg1_sent {
            return Err(OmcpError::HandshakeState);
        }

        let e_pub = X25519Public::from(&self.e).to_bytes();
        self.sym.mix_hash(&e_pub);
        self.sym.mix_key(&dh(&self.e, &self.rs));

        let mut msg = Vec::new();
        msg.extend_from_slice(&e_pub);

        let s_pub = self.s.dh_public;
        let enc_s = self.sym.encrypt_and_hash(&s_pub)?;
        msg.extend_from_slice(&enc_s);

        self.sym.mix_key(&dh(&self.s.dh, &self.rs));

        // Initiator static ML-KEM ek (cleartext, MixHash'd) — §21.5 role.
        let ek = self.s.kem_ek_bytes;
        self.sym.mix_hash(&ek);
        msg.extend_from_slice(&ek);

        let payload = payload(&self.sym.handshake_hash())?;
        let enc_payload = self.sym.encrypt_and_hash(&payload)?;
        msg.extend_from_slice(&enc_payload);

        self.msg1_sent = true;
        Ok(msg)
    }

    /// Read Noise IK message 2 (with responder ML-KEM CT) and derive session keys.
    ///
    /// Layout: `e (32) || mlkem_ct (1088) || enc(payload)`
    pub fn read_message_2(&mut self, message: &[u8]) -> Result<(Vec<u8>, HybridSessionKeys)> {
        self.read_message_2_bound(message).map(|(p, _, k)| (p, k))
    }

    /// [`Self::read_message_2`] also returning the handshake hash the responder
    /// saw when sealing its payload.
    pub fn read_message_2_bound(
        &mut self,
        message: &[u8],
    ) -> Result<(Vec<u8>, [u8; HASHLEN], HybridSessionKeys)> {
        if !self.msg1_sent {
            return Err(OmcpError::HandshakeState);
        }
        // msg2 min: e + ct + enc(empty payload) = 32 + 1088 + 16
        const MSG2_MIN: usize = DHLEN + MLKEM768_CT_LEN + 16;
        if message.len() < MSG2_MIN {
            return Err(OmcpError::HandshakeMalformed("msg2 short"));
        }

        let (e_bytes, rest) = message.split_at(DHLEN);
        let mut e_arr = [0u8; DHLEN];
        e_arr.copy_from_slice(e_bytes);
        let re = X25519Public::from(e_arr);
        self.sym.mix_hash(&e_arr);
        self.sym.mix_key(&dh(&self.e, &re));
        self.sym.mix_key(&dh(&self.s.dh, &re));

        if rest.len() < MLKEM768_CT_LEN + 16 {
            return Err(OmcpError::HandshakeMalformed("msg2 ct/payload"));
        }
        let (ct_bytes, enc_payload) = rest.split_at(MLKEM768_CT_LEN);
        self.sym.mix_hash(ct_bytes);

        let ct: Ciphertext<MlKem768> = ct_bytes
            .try_into()
            .map_err(|_| OmcpError::HandshakeMalformed("kem ct length"))?;
        let ss = self.s.kem_dk.decapsulate(&ct);
        let mut mlkem_ss = [0u8; MLKEM768_SS_LEN];
        mlkem_ss.copy_from_slice(ss.as_slice());

        let h_pre = self.sym.handshake_hash();
        let payload = self.sym.decrypt_and_hash(enc_payload)?;

        let h = self.sym.handshake_hash();
        let sym = std::mem::replace(&mut self.sym, SymmetricState::initialize(b"consumed"));
        let ck = sym.chaining_key();
        let keys = derive_session(&ck, &mlkem_ss, &h)?;
        mlkem_ss.zeroize();
        Ok((payload, h_pre, keys))
    }
}

impl HybridResponder {
    /// Create responder with its static identity (DH public already known to initiator).
    pub fn new(static_id: StaticIdentity) -> Self {
        let mut sym = SymmetricState::initialize(HYBRID_PROTOCOL_NAME);
        sym.mix_hash(&static_id.dh_public);
        Self {
            sym,
            s: static_id,
            e: None,
            re: None,
            rs: None,
            initiator_kem_ek: None,
            msg1_ok: false,
        }
    }

    /// Read message 1; returns initiator payload (cleartext after AEAD).
    ///
    /// Parses initiator ML-KEM ek (cleartext, MixHash'd) per §21.5.
    pub fn read_message_1(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        self.read_message_1_bound(message).map(|(p, _)| p)
    }

    /// [`Self::read_message_1`] also returning the handshake hash the initiator
    /// saw when sealing its payload.
    pub fn read_message_1_bound(&mut self, message: &[u8]) -> Result<(Vec<u8>, [u8; HASHLEN])> {
        if self.msg1_ok {
            return Err(OmcpError::HandshakeState);
        }

        // e + enc(s) + ek + enc(payload>=0)
        let min = DHLEN + sealed_dh_len() + MLKEM768_EK_LEN + 16;
        if message.len() < min {
            return Err(OmcpError::HandshakeMalformed("msg1 short"));
        }

        let (e_bytes, rest) = message.split_at(DHLEN);
        let mut e_arr = [0u8; DHLEN];
        e_arr.copy_from_slice(e_bytes);
        let re = X25519Public::from(e_arr);
        self.sym.mix_hash(&e_arr);
        self.sym.mix_key(&dh(&self.s.dh, &re));

        if rest.len() < sealed_dh_len() + MLKEM768_EK_LEN + 16 {
            return Err(OmcpError::HandshakeMalformed("msg1 body"));
        }
        let (enc_s, after_s) = rest.split_at(sealed_dh_len());
        let s_bytes = self.sym.decrypt_and_hash(enc_s)?;
        if s_bytes.len() != DHLEN {
            return Err(OmcpError::HandshakeMalformed("initiator static len"));
        }
        let mut s_arr = [0u8; DHLEN];
        s_arr.copy_from_slice(&s_bytes);
        let rs = X25519Public::from(s_arr);
        self.sym.mix_key(&dh(&self.s.dh, &rs));

        let (ek_bytes, enc_payload) = after_s.split_at(MLKEM768_EK_LEN);
        let mut ek_arr = [0u8; MLKEM768_EK_LEN];
        ek_arr.copy_from_slice(ek_bytes);
        // Validate EK parses; MixHash for transcript binding.
        let _ = parse_kem_ek(&ek_arr)?;
        self.sym.mix_hash(&ek_arr);

        let h_pre = self.sym.handshake_hash();
        let payload = self.sym.decrypt_and_hash(enc_payload)?;

        self.re = Some(re);
        self.rs = Some(rs);
        self.initiator_kem_ek = Some(ek_arr);
        self.e = Some(StaticSecret::from(random_32()?));
        self.msg1_ok = true;
        Ok((payload, h_pre))
    }

    /// Initiator static X25519 public key learned from message 1 (None before).
    pub fn remote_static(&self) -> Option<[u8; DHLEN]> {
        self.rs.map(|k| k.to_bytes())
    }

    /// Write message 2 (ML-KEM CT + payload) and derive session keys.
    ///
    /// Layout: `e (32) || mlkem_ct (1088) || enc(payload)`
    pub fn write_message_2(&mut self, payload: &[u8]) -> Result<(Vec<u8>, HybridSessionKeys)> {
        self.write_message_2_with(|_| Ok(payload.to_vec()))
    }

    /// [`Self::write_message_2`] with a payload built from the handshake hash.
    pub fn write_message_2_with(
        &mut self,
        payload: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
    ) -> Result<(Vec<u8>, HybridSessionKeys)> {
        if !self.msg1_ok {
            return Err(OmcpError::HandshakeState);
        }
        let e = self.e.as_ref().ok_or(OmcpError::HandshakeState)?;
        let re = self.re.ok_or(OmcpError::HandshakeState)?;
        let rs = self.rs.ok_or(OmcpError::HandshakeState)?;
        let ek_bytes = self
            .initiator_kem_ek
            .as_ref()
            .ok_or(OmcpError::HandshakeState)?;

        let e_pub = X25519Public::from(e).to_bytes();
        self.sym.mix_hash(&e_pub);
        self.sym.mix_key(&dh(e, &re));
        self.sym.mix_key(&dh(e, &rs));

        let ek = parse_kem_ek(ek_bytes)?;
        let (ct, ss) = ek.encapsulate();
        let mut mlkem_ss = [0u8; MLKEM768_SS_LEN];
        mlkem_ss.copy_from_slice(ss.as_slice());
        let ct_bytes = ct.as_slice();
        debug_assert_eq!(ct_bytes.len(), MLKEM768_CT_LEN);
        self.sym.mix_hash(ct_bytes);

        let payload = payload(&self.sym.handshake_hash())?;
        let enc_payload = self.sym.encrypt_and_hash(&payload)?;
        let mut msg = Vec::with_capacity(DHLEN + MLKEM768_CT_LEN + enc_payload.len());
        msg.extend_from_slice(&e_pub);
        msg.extend_from_slice(ct_bytes);
        msg.extend_from_slice(&enc_payload);

        let h = self.sym.handshake_hash();
        let sym = std::mem::replace(&mut self.sym, SymmetricState::initialize(b"consumed"));
        let ck = sym.chaining_key();
        let keys = derive_session(&ck, &mlkem_ss, &h)?;
        mlkem_ss.zeroize();
        Ok((msg, keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_handshake_shared_secrets_match() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();

        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let msg1 = init.write_message_1(b"hello-responder").unwrap();
        let p1 = resp.read_message_1(&msg1).unwrap();
        assert_eq!(p1, b"hello-responder");

        let (msg2, keys_r) = resp.write_message_2(b"hello-initiator").unwrap();
        let (p2, keys_i) = init.read_message_2(&msg2).unwrap();
        assert_eq!(p2, b"hello-initiator");

        assert_eq!(keys_i.k_i2r, keys_r.k_i2r);
        assert_eq!(keys_i.k_r2i, keys_r.k_r2i);
        assert_eq!(keys_i.handshake_hash, keys_r.handshake_hash);
        assert_ne!(keys_i.k_i2r, [0u8; 32]);
        assert_ne!(keys_i.k_r2i, keys_i.k_i2r);
    }

    #[test]
    fn hybrid_hkdf_info_is_normative_label() {
        assert_eq!(HYBRID_HKDF_INFO, b"OMCP-Noise-Hybrid-v1");
    }

    #[test]
    fn tampered_msg1_aead_fails() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let mut msg1 = init.write_message_1(b"x").unwrap();
        // Flip a byte inside encrypted static key region (after ephemeral pubkey).
        let idx = DHLEN + 3;
        msg1[idx] ^= 0x01;
        let err = resp.read_message_1(&msg1).unwrap_err();
        assert!(matches!(err, OmcpError::HandshakeAead));
    }

    #[test]
    fn tampered_msg2_aead_fails() {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let msg1 = init.write_message_1(b"x").unwrap();
        resp.read_message_1(&msg1).unwrap();
        let (mut msg2, _) = resp.write_message_2(b"y").unwrap();
        let last = msg2.len() - 1;
        msg2[last] ^= 0xff;
        let err = match init.read_message_2(&msg2) {
            Err(e) => e,
            Ok(_) => panic!("expected AEAD failure"),
        };
        assert!(matches!(err, OmcpError::HandshakeAead));
    }

    #[test]
    fn tampered_mlkem_ek_breaks_transcript() {
        // Initiator EK is MixHash'd before msg1 payload AEAD (§21.5).
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let mut msg1 = init.write_message_1(b"x").unwrap();
        // EK sits after e || enc(s); flip a byte in the EK region.
        let ek_idx = DHLEN + sealed_dh_len() + 7;
        msg1[ek_idx] ^= 0x5a;
        let err = resp.read_message_1(&msg1).unwrap_err();
        // Either EK parse fails or payload AEAD fails from divergent MixHash.
        assert!(
            matches!(
                err,
                OmcpError::HandshakeAead | OmcpError::HandshakeMalformed(_)
            ),
            "unexpected {err:?}"
        );
    }

    #[test]
    fn tampered_mlkem_ct_breaks_transcript() {
        // Responder CT is MixHash'd before msg2 payload AEAD (§21.5).
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);

        let msg1 = init.write_message_1(b"x").unwrap();
        resp.read_message_1(&msg1).unwrap();
        let (mut msg2, _) = resp.write_message_2(b"y").unwrap();
        // CT sits after e; flip a byte in the CT region.
        let ct_idx = DHLEN + 11;
        msg2[ct_idx] ^= 0x5a;
        let err = match init.read_message_2(&msg2) {
            Err(e) => e,
            Ok(_) => panic!("expected AEAD failure from divergent transcript"),
        };
        assert!(matches!(err, OmcpError::HandshakeAead));
    }

    #[test]
    fn truncated_msg1_rejected() {
        let responder_id = StaticIdentity::generate().unwrap();
        let mut resp = HybridResponder::new(responder_id);
        let err = resp.read_message_1(&[0u8; 10]).unwrap_err();
        assert!(matches!(err, OmcpError::HandshakeMalformed(_)));
    }
}

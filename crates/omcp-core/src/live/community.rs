//! Live inside a community (PROTOCOL §20.3 "Livestream media keys are derived
//! from the community/channel MLS epoch exporter + per-stream random").
//!
//! ```text
//! exporter   = MLS-Exporter("OMCP-live-v1", community_id ‖ stream_id, 32)
//! secret     = HKDF-SHA-256(salt = nonce[32], ikm = exporter,
//!                           info = "OMCP-community-live-v1" ‖ community_id ‖ stream_id)
//! StreamKey  = (stream_id, key_epoch 0, secret)        // then as in `live::StreamKey`
//! ```
//!
//! The announcement ([`CommunityLive`]) is an MLS application message of the
//! community group, so its sender is authenticated by MLS and only members
//! can read it. It carries the random `nonce` (not secret) and the host's
//! Ed25519 identity key, which must hash to the MLS sender's PeerID.
//!
//! Removing a viewer rotates to a random secret at `key_epoch + 1` that the
//! removed member can't derive. It travels as a host-signed [`Rekey`] inside
//! pairwise E2EE chats, forwarded member to member, never to the excluded ids.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use super::{err, StreamKey, MAX_TITLE};
use crate::error::{OmcpError, Result};

pub const LIVE_EXPORTER_LABEL: &str = "OMCP-live-v1";
const SECRET_INFO: &[u8] = b"OMCP-community-live-v1";
const REKEY_DOMAIN: &[u8] = b"OMCP-live-rekey-v1\x00";
const MAX_EXCLUDED: usize = 64;

/// Epoch-0 stream secret of a community live.
pub fn community_secret(
    exporter: &[u8; 32],
    nonce: &[u8; 32],
    community_id: &[u8; 32],
    stream_id: &[u8; 16],
) -> [u8; 32] {
    let hk = hkdf::Hkdf::<sha2::Sha256>::new(Some(nonce), exporter);
    let mut info = SECRET_INFO.to_vec();
    info.extend_from_slice(community_id);
    info.extend_from_slice(stream_id);
    let mut k = [0u8; 32];
    hk.expand(&info, &mut k)
        .expect("32 bytes is a valid HKDF length");
    k
}

pub fn community_stream_key(
    exporter: &[u8; 32],
    nonce: &[u8; 32],
    community_id: &[u8; 32],
    stream_id: [u8; 16],
) -> StreamKey {
    StreamKey::new(
        stream_id,
        0,
        community_secret(exporter, nonce, community_id, &stream_id),
    )
}

/// Live announcement inside the community MLS group (start and end).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityLive {
    pub community_id: [u8; 32],
    pub stream_id: [u8; 16],
    pub nonce: [u8; 32],
    pub title: String,
    pub started_ms: u64,
    pub video: bool,
    pub max_hops: u8,
    /// `false` = ended.
    pub live: bool,
    pub host_key: [u8; 32],
}

impl CommunityLive {
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.title.len() > MAX_TITLE {
            return err("live title too long");
        }
        let mut b = Vec::new();
        ciborium::into_writer(self, &mut b).map_err(|_| OmcpError::Sync("community live cbor"))?;
        Ok(b)
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() > 1024 {
            return err("community live too large");
        }
        let v: Self =
            ciborium::from_reader(b).map_err(|_| OmcpError::Sync("community live cbor"))?;
        if v.title.len() > MAX_TITLE || v.max_hops == 0 || v.max_hops > 8 {
            return err("community live fields");
        }
        Ok(v)
    }

    /// The host key must belong to the MLS-authenticated sender.
    pub fn host_matches(&self, sender_peer_id: &[u8; 32]) -> bool {
        &crate::device::peer_id_from_identity(&self.host_key) == sender_peer_id
    }
}

/// Host-signed key rotation for a community live (sent pairwise, E2EE).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rekey {
    pub community_id: [u8; 32],
    pub stream_id: [u8; 16],
    pub key_epoch: u32,
    secret: [u8; 32],
    /// PeerIDs that must not receive this rekey (removed viewers).
    pub excluded: Vec<[u8; 32]>,
    pub host_key: [u8; 32],
    #[serde(with = "serde_bytes_64")]
    sig: [u8; 64],
}

mod serde_bytes_64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: serde_bytes_buf::Buf = Deserialize::deserialize(d)?;
        v.0.try_into()
            .map_err(|_| serde::de::Error::custom("signature length"))
    }
    pub mod serde_bytes_buf {
        pub struct Buf(pub Vec<u8>);
        impl<'de> serde::Deserialize<'de> for Buf {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> serde::de::Visitor<'de> for V {
                    type Value = Buf;
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        f.write_str("bytes")
                    }
                    fn visit_bytes<E>(self, v: &[u8]) -> Result<Buf, E> {
                        Ok(Buf(v.to_vec()))
                    }
                    fn visit_seq<A: serde::de::SeqAccess<'de>>(
                        self,
                        mut a: A,
                    ) -> Result<Buf, A::Error> {
                        let mut v = Vec::new();
                        while let Some(b) = a.next_element::<u8>()? {
                            v.push(b);
                        }
                        Ok(Buf(v))
                    }
                }
                d.deserialize_bytes(V)
            }
        }
    }
}

impl Rekey {
    fn body(&self) -> Vec<u8> {
        let mut b = REKEY_DOMAIN.to_vec();
        b.extend_from_slice(&self.community_id);
        b.extend_from_slice(&self.stream_id);
        b.extend_from_slice(&self.key_epoch.to_be_bytes());
        b.extend_from_slice(&self.secret);
        b.push(self.excluded.len() as u8);
        for e in &self.excluded {
            b.extend_from_slice(e);
        }
        b.extend_from_slice(&self.host_key);
        b
    }

    pub fn sign(
        community_id: [u8; 32],
        key: &StreamKey,
        excluded: Vec<[u8; 32]>,
        host: &SigningKey,
    ) -> Result<Self> {
        if excluded.len() > MAX_EXCLUDED {
            return err("too many excluded viewers");
        }
        let mut r = Self {
            community_id,
            stream_id: key.stream_id,
            key_epoch: key.key_epoch,
            secret: *key.secret(),
            excluded,
            host_key: host.verifying_key().to_bytes(),
            sig: [0; 64],
        };
        r.sig = host.sign(&r.body()).to_bytes();
        Ok(r)
    }

    /// Check the host signature against the key from the announcement.
    pub fn verify(&self, host_key: &[u8; 32]) -> bool {
        if &self.host_key != host_key || self.excluded.len() > MAX_EXCLUDED {
            return false;
        }
        VerifyingKey::from_bytes(host_key)
            .and_then(|vk| vk.verify_strict(&self.body(), &Signature::from_bytes(&self.sig)))
            .is_ok()
    }

    pub fn key(&self) -> StreamKey {
        StreamKey::new(self.stream_id, self.key_epoch, self.secret)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut b = Vec::new();
        ciborium::into_writer(self, &mut b).map_err(|_| OmcpError::Sync("rekey cbor"))?;
        Ok(b)
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() > 4096 {
            return err("rekey too large");
        }
        ciborium::from_reader(b).map_err(|_| OmcpError::Sync("rekey cbor"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mls::group::OmcpMlsMember;

    fn community() -> (OmcpMlsMember, OmcpMlsMember, OmcpMlsMember) {
        let mut a = OmcpMlsMember::generate(b"a".to_vec()).unwrap();
        let mut b = OmcpMlsMember::generate(b"b".to_vec()).unwrap();
        let outsider = {
            let mut o = OmcpMlsMember::generate(b"o".to_vec()).unwrap();
            o.create_group().unwrap();
            o
        };
        a.create_group().unwrap();
        let kp = a
            .parse_key_package(&b.key_package_bytes().unwrap())
            .unwrap();
        let (_c, w) = a.add_members(&[kp]).unwrap();
        b.join_from_welcome(&w).unwrap();
        (a, b, outsider)
    }

    #[test]
    fn members_derive_the_same_key_outsiders_and_other_streams_dont() {
        let (a, b, o) = community();
        let cid = [7u8; 32];
        let sid = [1u8; 16];
        let nonce = [9u8; 32];
        let ka = community_stream_key(&a.live_exporter(&cid, &sid).unwrap(), &nonce, &cid, sid);
        let kb = community_stream_key(&b.live_exporter(&cid, &sid).unwrap(), &nonce, &cid, sid);
        assert_eq!(ka, kb);
        let ko = community_stream_key(&o.live_exporter(&cid, &sid).unwrap(), &nonce, &cid, sid);
        assert_ne!(ka, ko, "non-member can't derive");
        let other_nonce =
            community_stream_key(&a.live_exporter(&cid, &sid).unwrap(), &[8; 32], &cid, sid);
        assert_ne!(ka, other_nonce, "per-stream random");
        let other_stream = community_stream_key(
            &a.live_exporter(&cid, &[2; 16]).unwrap(),
            &nonce,
            &cid,
            [2; 16],
        );
        assert_ne!(ka.secret(), other_stream.secret());
        // Domain-separated from the mailbox tag exporter.
        assert_ne!(
            &a.tag_exporter(0).unwrap(),
            &a.live_exporter(&cid, &sid).unwrap()
        );
        // Media still opens with the derived key on the other member.
        let p = super::super::audio::seal_audio(&ka, 0, 3, 1, &[vec![1, 2, 3]]).unwrap();
        assert!(super::super::audio::open_audio(&kb, &p).is_ok());
        assert!(super::super::audio::open_audio(&ko, &p).is_err());
    }

    #[test]
    fn announce_and_rekey_roundtrip_and_signatures() {
        let host = SigningKey::from_bytes(&[3u8; 32]);
        let hk = host.verifying_key().to_bytes();
        let a = CommunityLive {
            community_id: [1; 32],
            stream_id: [2; 16],
            nonce: [3; 32],
            title: "Q&A".into(),
            started_ms: 5,
            video: true,
            max_hops: 3,
            live: true,
            host_key: hk,
        };
        let d = CommunityLive::decode(&a.encode().unwrap()).unwrap();
        assert_eq!(d, a);
        assert!(a.host_matches(&crate::device::peer_id_from_identity(&hk)));
        assert!(!a.host_matches(&[0; 32]));
        let k = StreamKey::random([2; 16], 1).unwrap();
        let r = Rekey::sign([1; 32], &k, vec![[4; 32]], &host).unwrap();
        let r2 = Rekey::decode(&r.encode().unwrap()).unwrap();
        assert!(r2.verify(&hk));
        assert_eq!(r2.key(), k);
        let mut forged = r2.clone();
        forged.excluded.clear();
        assert!(!forged.verify(&hk), "excluded list is signed");
        let other = SigningKey::from_bytes(&[4u8; 32]);
        let fake = Rekey::sign([1; 32], &k, vec![], &other).unwrap();
        assert!(!fake.verify(&hk), "only the host can rekey");
    }
}

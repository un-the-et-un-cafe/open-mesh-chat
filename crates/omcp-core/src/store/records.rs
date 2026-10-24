//! Records kept in the encrypted local store (CBOR inside each sealed row).

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::burn::BurnSnapshot;

/// [`MessageRec::status`] values.
pub const STATUS_QUEUED: u8 = 0;
pub const STATUS_SENT: u8 = 1;
pub const STATUS_RECEIVED: u8 = 2;
/// Our message, end-to-end acked over the mesh (§22.8(3)).
pub const STATUS_DELIVERED: u8 = 4;

/// [`MessageRec::burn_state`] values.
pub const BURN_NONE: u8 = 0;
pub const BURN_UNOPENED: u8 = 1;
pub const BURN_OPEN: u8 = 2;
/// View-once media placeholder ("🔥 Photo opened"); no content anywhere.
pub const BURN_OPENED: u8 = 3;

/// [`FileRec::state`] values (mirror the apps' `FileAttachment.State`).
pub const FILE_QUEUED: u8 = 0;
pub const FILE_SENDING: u8 = 1;
pub const FILE_RECEIVING: u8 = 2;
pub const FILE_PAUSED: u8 = 3;
pub const FILE_SENT: u8 = 4;
pub const FILE_RECEIVED: u8 = 5;
pub const FILE_FAILED: u8 = 6;
pub const FILE_INTEGRITY_FAILED: u8 = 7;
/// Restored from a chat backup without its bytes: a missing-media placeholder
/// that is re-requested from the partner by `locator` + `hash` (REQUIREMENTS CB3).
pub const FILE_MISSING: u8 = 8;
/// A re-request is in flight.
pub const FILE_REQUESTED: u8 = 9;
/// The partner answered that it no longer has the file ("Ask to resend").
pub const FILE_UNAVAILABLE: u8 = 10;

/// [`MessageStatus`] for an in-chat system notice (e.g. the restore notice, RS1).
pub const STATUS_SYSTEM: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactRec {
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    pub name: String,
    pub verified: bool,
    /// Device-key state learned from this contact's device certificates (RS1).
    #[serde(default)]
    pub device: Option<crate::device::ContactDevice>,
    /// Current X25519 Noise key (v0.1.4a). `None` = legacy record whose `peer`
    /// is that X25519 key; re-keyed to the PeerID on the first handshake.
    #[serde(default, with = "opt_bytes")]
    pub dh: Option<Vec<u8>>,
}

/// One 1:1 conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatRec {
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    pub last_ms: u64,
    pub last_read_ms: u64,
}

/// File-transfer metadata and, for received files, the app's reference
/// (`local_path`) to the verified plaintext in app-private storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRec {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub state: u8,
    pub done_bytes: u64,
    pub local_path: Option<String>,
    /// BLAKE3 content hash (PROTOCOL §7); None for rows written before v0.1.4.
    #[serde(default, with = "opt_bytes")]
    pub hash: Option<Vec<u8>>,
    /// Media locator: the 16-byte transfer id the partner knows the file by.
    #[serde(default, with = "opt_bytes")]
    pub locator: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    pub mine: bool,
    pub time_ms: u64,
    pub status: u8,
    /// Always empty for burn messages (content lives only in the burn table).
    pub text: String,
    pub mime: Option<String>,
    /// Encoded 9-byte `burn` header, if any.
    #[serde(with = "opt_bytes")]
    pub burn: Option<Vec<u8>>,
    pub burn_state: u8,
    pub burn_at_ms: Option<u64>,
    pub file: Option<FileRec>,
}

/// One pending outbound message (envelope id fixed at queue time).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    pub created_ms: u64,
    /// `InnerV1` plaintext (the row itself is sealed).
    #[serde(with = "bytes")]
    pub inner: Vec<u8>,
    pub control: bool,
    /// Stored form only: `inner` is additionally sealed under the burn key.
    #[serde(default)]
    pub burn: bool,
}

impl std::fmt::Debug for OutboxRec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutboxRec")
            .field("id", &self.id)
            .field("control", &self.control)
            .finish_non_exhaustive()
    }
}

impl Drop for OutboxRec {
    fn drop(&mut self) {
        self.inner.zeroize();
    }
}

/// Index row of a kept local copy of sent media (REQUIREMENTS "Keep sent media").
/// The bytes are a separate sealed row so eviction never decrypts them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptMediaRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    #[serde(with = "b32")]
    pub hash: [u8; 32],
    pub kept_ms: u64,
    pub size: u64,
}

/// Settings > Storage limits for kept sent media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptMediaLimits {
    pub max_days: u32,
    pub max_bytes: u64,
}

impl Default for KeptMediaLimits {
    /// 90 days or 2 GB, whichever is hit first.
    fn default() -> Self {
        Self {
            max_days: 90,
            max_bytes: 2_000_000_000,
        }
    }
}

/// An outgoing file transfer; its sealed chunk payloads are separate rows.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutFileRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    pub name: String,
    pub size: u64,
    pub total: u32,
    /// `InnerV1` file offer (contains the Content Key; the row is sealed).
    #[serde(with = "bytes")]
    pub offer: Vec<u8>,
}

impl Drop for OutFileRec {
    fn drop(&mut self) {
        self.offer.zeroize();
    }
}

/// Which side of a burn message this node holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurnDir {
    /// Received (receiver's copy).
    In = 0,
    /// Our sent copy, kept until `OPENED` + timer or unread expiry.
    Out = 1,
}

#[derive(Serialize, Deserialize)]
pub(super) struct BurnRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    #[serde(with = "b32")]
    pub peer: [u8; 32],
    pub dir: u8,
    pub mime: Option<String>,
    #[serde(with = "bytes")]
    pub header: Vec<u8>,
    pub expires_at_ms: u64,
    pub burn_at_ms: Option<u64>,
    pub opened: bool,
    /// Content sealed under the per-message key (kept in `burn_keys`).
    #[serde(with = "bytes")]
    pub sealed: Vec<u8>,
}

impl Drop for BurnRec {
    fn drop(&mut self) {
        self.sealed.zeroize();
    }
}

/// A burn entry loaded from the store.
#[derive(Debug)]
pub struct StoredBurn {
    pub snapshot: BurnSnapshot,
    pub peer: [u8; 32],
    pub dir: BurnDir,
    pub mime: Option<String>,
}

pub(super) mod bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let v: ciborium::Value = Deserialize::deserialize(d)?;
        v.into_bytes()
            .map_err(|_| serde::de::Error::custom("expected bytes"))
    }
}

mod opt_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_some(&ciborium::Value::Bytes(b.clone())),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let v: Option<ciborium::Value> = Deserialize::deserialize(d)?;
        v.map(|v| {
            v.into_bytes()
                .map_err(|_| serde::de::Error::custom("expected bytes"))
        })
        .transpose()
    }
}

macro_rules! fixed {
    ($m:ident, $n:literal) => {
        mod $m {
            use serde::{Deserializer, Serializer};
            pub fn serialize<S: Serializer>(v: &[u8; $n], s: S) -> Result<S::Ok, S::Error> {
                s.serialize_bytes(v)
            }
            pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; $n], D::Error> {
                let v = super::bytes::deserialize(d)?;
                v.try_into()
                    .map_err(|_| serde::de::Error::custom(concat!("expected ", $n, " bytes")))
            }
        }
    };
}
fixed!(b16, 16);
fixed!(b32, 32);

/// One mesh carry-store item (v0.2): the encoded opaque `EnvelopeV1`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    #[serde(with = "bytes")]
    pub envelope: Vec<u8>,
    pub expires_at: u64,
    pub hop_count: u8,
    /// Our own outbound mesh copy (not relayed for someone else).
    pub own: bool,
    /// `DELIVERY_ACK` envelope: the acked `msg_hash` (§22.8(3)).
    #[serde(default)]
    pub ack_of: Option<[u8; 16]>,
}

/// A mesh envelope id already delivered to us (dedup across restarts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshSeenRec {
    #[serde(with = "b16")]
    pub id: [u8; 16],
    pub until_ms: u64,
}

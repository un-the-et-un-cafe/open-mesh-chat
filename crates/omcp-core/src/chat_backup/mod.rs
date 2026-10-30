//! Encrypted chat backup (REQUIREMENTS "Encrypted chat backup (v0.1.4)", CB1–CB5;
//! PROTOCOL §11.1).
//!
//! - **Key**: `HKDF-SHA-256(salt = none, ikm = 24-word master seed,
//!   info = "hopla/chat-backup/v1")`. Without the 24 words the blobs can't be
//!   opened (CB2). The Argon2id identity backup is separate and no longer carries
//!   history (its old `history` field is still read, see [`crate::backup`]).
//! - **Blob**: `"HCB1" | kind u8 | seq u64 BE | nonce 24 | XChaCha20-Poly1305(ct)`
//!   with AAD = `"hopla/chat-backup/v1" | kind | seq` (media: `| content hash`).
//!   The plaintext is `len u32 BE | CBOR | zero padding` so every blob is an exact
//!   multiple of [`BUCKET`] (64 KiB): the provider sees padded sizes only.
//! - **Incremental**: an append-only log (`kind = 1`) of changed or deleted
//!   messages since the previous upload, plus a compacted snapshot (`kind = 0`)
//!   at most weekly. After a snapshot uploads, every blob with a lower `seq` can be
//!   deleted. Text uploads run at most hourly, Wi-Fi only by default.
//! - **Never backed up**: burn-after-read / view-once content (CB4), media bytes
//!   unless media backup is on (CB5), received-file paths of this device.
//! - **Restore**: messages come back; media come back as **missing-media
//!   placeholders** ([`crate::store::FILE_MISSING`]) that keep type, size, sender,
//!   time, caption, content hash and locator, and are re-requested from the
//!   partner (`MEDIA_REQUEST`, PROTOCOL §7.1).
//! - **Media backup** (CB5, off by default): one blob per media item, named by a
//!   keyed hash of its content hash, uploaded in a separate daily pass.

use std::collections::{BTreeMap, BTreeSet};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{OmcpError, Result};
use crate::store::{
    ChatRec, MessageRec, Store, FILE_MISSING, FILE_RECEIVED, FILE_SENT, STATUS_SYSTEM,
};

/// HKDF info label and AAD prefix.
pub const LABEL: &[u8] = b"hopla/chat-backup/v1";
/// Padding bucket.
pub const BUCKET: usize = 64 * 1024;
pub const KIND_SNAPSHOT: u8 = 0;
pub const KIND_LOG: u8 = 1;
pub const KIND_MEDIA: u8 = 2;
/// Text backup runs at most hourly.
pub const TEXT_INTERVAL_MS: u64 = 3600 * 1000;
/// Media backup runs in a separate daily pass.
pub const MEDIA_INTERVAL_MS: u64 = 24 * 3600 * 1000;
/// Compacted snapshot cadence.
pub const SNAPSHOT_INTERVAL_MS: u64 = 7 * 24 * 3600 * 1000;

const MAGIC: &[u8; 4] = b"HCB1";
const HEADER_LEN: usize = 4 + 1 + 8 + 24;
const TAG_LEN: usize = 16;
const SEGMENT_VERSION: u8 = 1;
const META_STATE: &[u8] = b"chat_backup_state";
const META_MEDIA: &[u8] = b"media_backup_settings";

/// Chat-backup key from the 24-word master seed.
pub fn chat_backup_key(seed: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>> {
    let mut k = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(None, seed)
        .expand(LABEL, k.as_mut())
        .map_err(|_| OmcpError::Crypto("hkdf chat backup"))?;
    Ok(k)
}

fn aad(kind: u8, seq: u64, extra: &[u8]) -> Vec<u8> {
    let mut a = LABEL.to_vec();
    a.push(kind);
    a.extend_from_slice(&seq.to_be_bytes());
    a.extend_from_slice(extra);
    a
}

fn seal(key: &[u8; 32], kind: u8, seq: u64, extra: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    let body = 4 + plain.len();
    let total = (body + HEADER_LEN + TAG_LEN).div_ceil(BUCKET) * BUCKET;
    let mut pt = Zeroizing::new(Vec::with_capacity(total - HEADER_LEN - TAG_LEN));
    let len = u32::try_from(plain.len()).map_err(|_| OmcpError::Backup("chat backup too large"))?;
    pt.extend_from_slice(&len.to_be_bytes());
    pt.extend_from_slice(plain);
    pt.resize(total - HEADER_LEN - TAG_LEN, 0);
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|_| OmcpError::Crypto("getrandom"))?;
    let ct = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| OmcpError::Crypto("chat backup key"))?
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &pt,
                aad: &aad(kind, seq, extra),
            },
        )
        .map_err(|_| OmcpError::Crypto("chat backup seal"))?;
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    debug_assert_eq!(out.len() % BUCKET, 0);
    Ok(out)
}

/// `(kind, seq)` from a blob header (no decryption).
pub fn blob_header(blob: &[u8]) -> Result<(u8, u64)> {
    if blob.len() < HEADER_LEN + TAG_LEN
        || &blob[..4] != MAGIC
        || !blob.len().is_multiple_of(BUCKET)
    {
        return Err(OmcpError::Backup("not a chat backup blob"));
    }
    Ok((
        blob[4],
        u64::from_be_bytes(blob[5..13].try_into().expect("len")),
    ))
}

fn open(key: &[u8; 32], blob: &[u8], extra: &[u8]) -> Result<(u8, u64, Zeroizing<Vec<u8>>)> {
    let (kind, seq) = blob_header(blob)?;
    let pt = Zeroizing::new(
        XChaCha20Poly1305::new_from_slice(key)
            .map_err(|_| OmcpError::Crypto("chat backup key"))?
            .decrypt(
                XNonce::from_slice(&blob[13..37]),
                Payload {
                    msg: &blob[HEADER_LEN..],
                    aad: &aad(kind, seq, extra),
                },
            )
            .map_err(|_| OmcpError::BackupAuth)?,
    );
    let len = u32::from_be_bytes(pt[..4].try_into().expect("len")) as usize;
    if 4 + len > pt.len() {
        return Err(OmcpError::Backup("chat backup length"));
    }
    Ok((kind, seq, Zeroizing::new(pt[4..4 + len].to_vec())))
}

/// A contact's display name as it was when backed up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupName {
    pub peer: Vec<u8>,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
struct Segment {
    v: u8,
    created_ms: u64,
    names: Vec<BackupName>,
    chats: Vec<ChatRec>,
    messages: Vec<MessageRec>,
    deleted: Vec<Vec<u8>>,
}

/// Upload bookkeeping, kept sealed in the local store.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupState {
    pub seq: u64,
    pub snapshot_seq: u64,
    pub last_snapshot_ms: u64,
    pub last_text_ms: u64,
    pub last_media_ms: u64,
    /// message id → 16-byte digest of what was last uploaded.
    digests: BTreeMap<Vec<u8>, Vec<u8>>,
    names: BTreeMap<Vec<u8>, String>,
    /// Content hashes already in the media backup.
    pub media_uploaded: BTreeSet<Vec<u8>>,
}

/// One blob to upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upload {
    pub kind: u8,
    pub seq: u64,
    /// Provider file / record name.
    pub name: String,
    pub blob: Vec<u8>,
    /// After a successful snapshot upload, blobs with `seq` below this can go.
    pub delete_below_seq: Option<u64>,
}

/// File name for a text blob.
pub fn blob_name(kind: u8, seq: u64) -> String {
    match kind {
        KIND_SNAPSHOT => format!("hopla-chat-{seq:016}-snap.bin"),
        _ => format!("hopla-chat-{seq:016}-log.bin"),
    }
}

/// True if this message may go into a chat backup (CB4: never burn content).
pub fn backupable(m: &MessageRec) -> bool {
    m.burn.is_none()
}

fn for_backup(m: &MessageRec) -> MessageRec {
    let mut m = m.clone();
    if let Some(f) = m.file.as_mut() {
        f.local_path = None;
    }
    m
}

fn digest<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    ciborium::into_writer(v, &mut b).map_err(|_| OmcpError::Backup("cbor"))?;
    Ok(blake3::hash(&b).as_bytes()[..16].to_vec())
}

fn cbor<T: Serialize>(v: &T) -> Result<Zeroizing<Vec<u8>>> {
    let mut b = Zeroizing::new(Vec::new());
    ciborium::into_writer(v, &mut *b).map_err(|_| OmcpError::Backup("cbor"))?;
    Ok(b)
}

/// Text backup is due (hourly; Wi-Fi only unless `allow_mobile`).
pub fn text_backup_due(st: &BackupState, now_ms: u64, on_wifi: bool, allow_mobile: bool) -> bool {
    (on_wifi || allow_mobile) && now_ms.saturating_sub(st.last_text_ms) >= TEXT_INTERVAL_MS
}

/// Build the next text upload from the store, or `None` if nothing changed.
/// Returns the state to save once the upload has succeeded.
pub fn plan(
    store: &Store,
    prev: &BackupState,
    names: &[BackupName],
    now_ms: u64,
    force_snapshot: bool,
) -> Result<Option<(Upload, BackupState)>> {
    let msgs: Vec<MessageRec> = store
        .all_messages()?
        .iter()
        .filter(|m| backupable(m))
        .map(for_backup)
        .collect();
    let mut next = prev.clone();
    let snapshot = force_snapshot
        || prev.snapshot_seq == 0
        || now_ms.saturating_sub(prev.last_snapshot_ms) >= SNAPSHOT_INTERVAL_MS;
    let mut digests = BTreeMap::new();
    for m in &msgs {
        digests.insert(m.id.to_vec(), digest(m)?);
    }
    let name_map: BTreeMap<Vec<u8>, String> = names
        .iter()
        .map(|n| (n.peer.clone(), n.name.clone()))
        .collect();
    let seg = if snapshot {
        Segment {
            v: SEGMENT_VERSION,
            created_ms: now_ms,
            names: names.to_vec(),
            chats: store.chats()?,
            messages: msgs,
            deleted: vec![],
        }
    } else {
        let changed: Vec<MessageRec> = msgs
            .into_iter()
            .filter(|m| prev.digests.get(m.id.as_slice()) != digests.get(m.id.as_slice()))
            .collect();
        let deleted: Vec<Vec<u8>> = prev
            .digests
            .keys()
            .filter(|k| !digests.contains_key(*k))
            .cloned()
            .collect();
        let names_changed: Vec<BackupName> = names
            .iter()
            .filter(|n| prev.names.get(&n.peer) != Some(&n.name))
            .cloned()
            .collect();
        if changed.is_empty() && deleted.is_empty() && names_changed.is_empty() {
            return Ok(None);
        }
        Segment {
            v: SEGMENT_VERSION,
            created_ms: now_ms,
            names: names_changed,
            chats: store.chats()?,
            messages: changed,
            deleted,
        }
    };
    next.seq += 1;
    next.digests = digests;
    next.names = name_map;
    next.last_text_ms = now_ms;
    let kind = if snapshot { KIND_SNAPSHOT } else { KIND_LOG };
    if snapshot {
        next.snapshot_seq = next.seq;
        next.last_snapshot_ms = now_ms;
    }
    Ok(Some((
        Upload {
            kind,
            seq: next.seq,
            name: blob_name(kind, next.seq),
            blob: cbor(&seg)?.to_vec(),
            delete_below_seq: snapshot.then_some(next.seq),
        },
        next,
    )))
}

/// Encrypt and pad a planned upload (the plan carries plaintext CBOR).
pub fn seal_upload(key: &[u8; 32], mut u: Upload) -> Result<Upload> {
    let plain = Zeroizing::new(std::mem::take(&mut u.blob));
    u.blob = seal(key, u.kind, u.seq, &[], &plain)?;
    Ok(u)
}

/// What a restore brought back.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Restored {
    pub messages: usize,
    pub media_placeholders: usize,
    pub names: Vec<BackupName>,
    /// State to continue the backup chain from this device.
    pub state: BackupState,
}

/// Restore text history from all downloaded blobs into `store`.
/// Media items become missing-media placeholders (CB1, CB3).
pub fn restore(key: &[u8; 32], blobs: &[Vec<u8>], store: &Store) -> Result<Restored> {
    let mut segs: Vec<(u8, u64, Segment)> = Vec::new();
    for b in blobs {
        let (kind, _) = blob_header(b)?;
        if kind == KIND_MEDIA {
            continue;
        }
        let (kind, seq, pt) = open(key, b, &[])?;
        let s: Segment =
            ciborium::from_reader(pt.as_slice()).map_err(|_| OmcpError::Backup("segment"))?;
        if s.v != SEGMENT_VERSION {
            return Err(OmcpError::BackupTooNew(s.v));
        }
        segs.push((kind, seq, s));
    }
    segs.sort_by_key(|(_, seq, _)| *seq);
    let start = segs
        .iter()
        .rposition(|(k, _, _)| *k == KIND_SNAPSHOT)
        .unwrap_or(0);
    let mut msgs: BTreeMap<Vec<u8>, MessageRec> = BTreeMap::new();
    // Original conversation order: the order messages first appear in the
    // backup stream (snapshot then logs), not their random ids (CB1).
    let mut order: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut chats: BTreeMap<Vec<u8>, ChatRec> = BTreeMap::new();
    let mut names: BTreeMap<Vec<u8>, String> = BTreeMap::new();
    let mut state = BackupState::default();
    for (kind, seq, s) in segs.into_iter().skip(start) {
        for m in s.messages {
            if backupable(&m) {
                let n = order.len();
                order.entry(m.id.to_vec()).or_insert(n);
                msgs.insert(m.id.to_vec(), m);
            }
        }
        for d in s.deleted {
            msgs.remove(&d);
        }
        for c in s.chats {
            chats.insert(c.peer.to_vec(), c);
        }
        for n in s.names {
            names.insert(n.peer, n.name);
        }
        state.seq = seq;
        if kind == KIND_SNAPSHOT {
            state.snapshot_seq = seq;
            state.last_snapshot_ms = s.created_ms;
        }
    }
    let mut out = Restored::default();
    let mut ordered: Vec<&MessageRec> = msgs.values().collect();
    ordered.sort_by_key(|m| order.get(m.id.as_slice()).copied().unwrap_or(usize::MAX));
    store.transaction(|s| {
        for m in ordered {
            let mut m = m.clone();
            if let Some(f) = m.file.as_mut() {
                f.local_path = None;
                if matches!(f.state, FILE_SENT | FILE_RECEIVED) || f.hash.is_some() {
                    f.state = FILE_MISSING;
                    f.done_bytes = 0;
                    out.media_placeholders += 1;
                }
            }
            if m.status != STATUS_SYSTEM {
                out.messages += 1;
            }
            state
                .digests
                .insert(m.id.to_vec(), digest(&for_backup(&m))?);
            s.put_message(&m)?;
        }
        for c in chats.values() {
            if let Some(cur) = s.chat(&c.peer)? {
                s.put_chat(&ChatRec {
                    last_read_ms: cur.last_read_ms.max(c.last_read_ms),
                    ..c.clone()
                })?;
            } else {
                s.put_chat(c)?;
            }
        }
        Ok(())
    })?;
    // Placeholders differ from the uploaded form: re-upload them on the next run
    // only if they change again (digest above is of the placeholder form).
    state.names = names.clone();
    out.names = names
        .into_iter()
        .map(|(peer, name)| BackupName { peer, name })
        .collect();
    out.state = state;
    Ok(out)
}

// ---- media backup (CB5) ----

/// Media type classes with their own toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaClass {
    Photo,
    Video,
    Voice,
    File,
}

/// MIME → class.
pub fn media_class(mime: &str) -> MediaClass {
    let m = mime.to_ascii_lowercase();
    if m.starts_with("image/") {
        MediaClass::Photo
    } else if m.starts_with("video/") {
        MediaClass::Video
    } else if m.starts_with("audio/") {
        MediaClass::Voice
    } else {
        MediaClass::File
    }
}

/// Settings > Backup > "Also back up photos, videos and files".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaSettings {
    pub enabled: bool,
    pub photos: bool,
    pub videos: bool,
    pub voice: bool,
    pub files: bool,
    /// "Back up media on mobile data".
    pub mobile_data: bool,
}

impl Default for MediaSettings {
    /// Off; when switched on, photos, voice notes and files are pre-selected, videos not.
    fn default() -> Self {
        Self {
            enabled: false,
            photos: true,
            videos: false,
            voice: true,
            files: true,
            mobile_data: false,
        }
    }
}

impl MediaSettings {
    pub fn includes(&self, mime: &str) -> bool {
        self.enabled
            && match media_class(mime) {
                MediaClass::Photo => self.photos,
                MediaClass::Video => self.videos,
                MediaClass::Voice => self.voice,
                MediaClass::File => self.files,
            }
    }
}

/// Media pass is due (daily; Wi-Fi only unless the mobile-data toggle is on).
pub fn media_backup_due(s: &MediaSettings, st: &BackupState, now_ms: u64, on_wifi: bool) -> bool {
    s.enabled
        && (on_wifi || s.mobile_data)
        && now_ms.saturating_sub(st.last_media_ms) >= MEDIA_INTERVAL_MS
}

/// Provider name for a media blob: keyed hash of the content hash, so the
/// provider can't match files across accounts.
pub fn media_blob_name(key: &[u8; 32], hash: &[u8; 32]) -> String {
    let k = blake3::derive_key("hopla/chat-backup/v1 media-name", key);
    let h = blake3::keyed_hash(&k, hash);
    let hex: String = h.as_bytes()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("hopla-media-{hex}.bin")
}

/// Seal one media item; refuses bytes that don't match `hash`.
pub fn seal_media(key: &[u8; 32], hash: &[u8; 32], bytes: &[u8]) -> Result<Upload> {
    if blake3::hash(bytes).as_bytes() != hash {
        return Err(OmcpError::FileHashMismatch);
    }
    Ok(Upload {
        kind: KIND_MEDIA,
        seq: 0,
        name: media_blob_name(key, hash),
        blob: seal(key, KIND_MEDIA, 0, hash, bytes)?,
        delete_below_seq: None,
    })
}

/// Open a media blob and verify its content hash.
pub fn open_media(key: &[u8; 32], hash: &[u8; 32], blob: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let (kind, _, pt) = open(key, blob, hash)?;
    if kind != KIND_MEDIA || blake3::hash(&pt).as_bytes() != hash {
        return Err(OmcpError::FileHashMismatch);
    }
    Ok(pt)
}

/// A media item the app should read from disk and upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaTodo {
    pub msg_id: [u8; 16],
    pub hash: [u8; 32],
    pub mime: String,
    pub local_path: String,
}

/// Media items selected by `settings` that aren't uploaded yet.
pub fn plan_media(
    store: &Store,
    settings: &MediaSettings,
    st: &BackupState,
) -> Result<Vec<MediaTodo>> {
    if !settings.enabled {
        return Ok(vec![]);
    }
    Ok(store
        .all_messages()?
        .into_iter()
        .filter(backupable)
        .filter_map(|m| {
            let f = m.file?;
            let hash: [u8; 32] = f.hash.as_deref()?.try_into().ok()?;
            let path = f.local_path?;
            (settings.includes(&f.mime) && !st.media_uploaded.contains(hash.as_slice())).then_some(
                MediaTodo {
                    msg_id: m.id,
                    hash,
                    mime: f.mime,
                    local_path: path,
                },
            )
        })
        .collect())
}

// ---- state persistence ----

pub fn load_state(store: &Store) -> Result<BackupState> {
    Ok(store.get_meta(META_STATE)?.unwrap_or_default())
}

pub fn save_state(store: &Store, st: &BackupState) -> Result<()> {
    store.put_meta(META_STATE, st)
}

pub fn load_media_settings(store: &Store) -> Result<MediaSettings> {
    Ok(store.get_meta(META_MEDIA)?.unwrap_or_default())
}

pub fn save_media_settings(store: &Store, s: &MediaSettings) -> Result<()> {
    store.put_meta(META_MEDIA, s)
}

#[cfg(test)]
mod tests;

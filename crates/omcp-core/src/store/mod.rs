//! Encrypted local store: contacts, verification state, chats, messages,
//! file-transfer metadata, received-file references, the pending outbound
//! queue and burn-after-read state survive an app restart.
//!
//! Spec: PROTOCOL §3.2 item 4 ("Local storage key — device-bound (Android
//! Keystore / iOS Secure Enclave)"), §4 threat table ("Compromised device"),
//! §19.1 item 4 (crypto-shredding),
//! apps/REQUIREMENTS.md "Burn after read (v0.1.3)" (separate encrypted table,
//! crypto-shredding, B1 forensic test, B5 no burn content in backups).
//!
//! # Design
//!
//! - **SQLite** (bundled, via `rusqlite`), one file per identity. Every table has
//!   the same shape: `id BLOB PRIMARY KEY, grp BLOB, seq INTEGER, ct BLOB`.
//! - **Encryption at rest is done per row by this module**, not by SQLCipher:
//!   `ct = nonce(24) ‖ XChaCha20-Poly1305(row_key, CBOR(record), AAD)`, with
//!   `AAD = "OMCP-Store-v1" ‖ table ‖ 0x00 ‖ id`, so a row can't be moved to
//!   another table or id. The 256-bit *storage key* comes from the platform
//!   (Android Keystore-wrapped / iOS Keychain) through FFI and is never
//!   written to disk; `row_key` and `id_key` are BLAKE3 `derive_key` sub-keys.
//! - **Ids are opaque**: `id = BLAKE3-keyed(id_key, table ‖ 0x00 ‖ natural id)`,
//!   `grp` (chat / file grouping) likewise. The file reveals row counts, sizes
//!   and insertion order (`seq`) — not peers, names, text or ids.
//! - **Schema migrations** are numbered SQL batches applied in one transaction
//!   each, tracked with `PRAGMA user_version` ([`SCHEMA_VERSION`]). A store
//!   written by a newer app is refused instead of being guessed at.
//! - **Burn after read**: burn content lives only in the `burn` table, sealed
//!   under its own random per-message key (the [`crate::burn::BurnStore`] key);
//!   that key sits in a separate `burn_keys` row. Burning deletes both rows with
//!   `PRAGMA secure_delete = ON` (freed pages are zeroed) and a rollback journal
//!   (`journal_mode = DELETE`, no WAL copies). Message rows for burn messages
//!   never contain content, and a queued (unsent) burn message is sealed under a
//!   sub-key of the per-message key, so it's shredded with it.
//! - A wrong storage key fails cleanly ([`OmcpError::StoreAuth`]) via a sealed
//!   key-check row; a store bound to another identity is refused.

use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::burn::{BurnHeader, BurnSnapshot};
use crate::error::{OmcpError, Result};

mod records;
pub use records::*;

#[cfg(test)]
mod tests;

/// Current schema version (`PRAGMA user_version`).
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;
/// Version of the plaintext history snapshot carried in backups.
pub const HISTORY_VERSION: u8 = 1;
const AAD_PREFIX: &[u8] = b"OMCP-Store-v1";
const KEY_CHECK: &[u8] = b"OMCP-Store key check v1";
const CTX_ROW: &str = "OMCP-Store-v1 row-aead";
const CTX_ID: &str = "OMCP-Store-v1 row-id";
const CTX_BURN_OUTBOX: &str = "OMCP-Store-v1 burn-outbox";

/// One numbered migration per entry; index 0 = version 1. Never edit a shipped
/// entry: append a new one.
const MIGRATIONS: &[&str] = &[
    // v1: initial schema.
    "CREATE TABLE meta(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE TABLE contacts(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE TABLE chats(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE TABLE messages(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE INDEX messages_grp ON messages(grp, seq);
     CREATE TABLE outbox(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE TABLE out_files(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE TABLE file_chunks(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE INDEX file_chunks_grp ON file_chunks(grp, seq);
     CREATE TABLE burn(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE TABLE burn_keys(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);",
    // v2 (v0.1.4b): kept local copies of sent media (index + sealed bytes rows).
    "CREATE TABLE kept_media(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE INDEX kept_media_grp ON kept_media(grp, seq);",
    // v3 (v0.2 mesh relay): carry store (opaque envelopes relayed for others
    // and our own mesh copies) and the delivered-id dedup set.
    "CREATE TABLE relay(id BLOB PRIMARY KEY, grp BLOB, seq INTEGER NOT NULL, ct BLOB NOT NULL);
     CREATE INDEX relay_grp ON relay(grp, seq);",
];

const RELAY_CARRY_GRP: &[u8] = b"carry";
const RELAY_SEEN_GRP: &[u8] = b"seen";

const KEPT_INDEX_GRP: &[u8] = b"kept-index";
const DAY_MS: u64 = 86_400_000;

fn kept_key(tag: u8, id: &[u8; 16]) -> [u8; 17] {
    let mut k = [tag; 17];
    k[1..].copy_from_slice(id);
    k
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Table {
    Meta,
    Contacts,
    Chats,
    Messages,
    Outbox,
    OutFiles,
    FileChunks,
    Burn,
    BurnKeys,
    KeptMedia,
    Relay,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Table::Meta => "meta",
            Table::Contacts => "contacts",
            Table::Chats => "chats",
            Table::Messages => "messages",
            Table::Outbox => "outbox",
            Table::OutFiles => "out_files",
            Table::FileChunks => "file_chunks",
            Table::Burn => "burn",
            Table::BurnKeys => "burn_keys",
            Table::KeptMedia => "kept_media",
            Table::Relay => "relay",
        }
    }
}

fn db(_: rusqlite::Error) -> OmcpError {
    OmcpError::Store("sqlite")
}

fn cbor<T: Serialize>(v: &T) -> Result<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::new());
    ciborium::into_writer(v, &mut *out).map_err(|_| OmcpError::Store("cbor encode"))?;
    Ok(out)
}

fn uncbor<T: DeserializeOwned>(b: &[u8]) -> Result<T> {
    ciborium::from_reader(b).map_err(|_| OmcpError::Store("cbor decode"))
}

fn rand<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(b)
}

/// Fresh random 256-bit storage key (the platform keeps it in Keystore/Keychain).
pub fn generate_storage_key() -> Result<Zeroizing<[u8; 32]>> {
    Ok(Zeroizing::new(rand::<32>()?))
}

fn xseal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> Result<Vec<u8>> {
    let nonce = rand::<24>()?;
    let c = XChaCha20Poly1305::new_from_slice(key).map_err(|_| OmcpError::Crypto("xchacha key"))?;
    let ct = c
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plain, aad })
        .map_err(|_| OmcpError::Crypto("store seal"))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(out)
}

fn xopen(key: &[u8; 32], aad: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    if sealed.len() < 24 + 16 {
        return Err(OmcpError::StoreAuth);
    }
    let c = XChaCha20Poly1305::new_from_slice(key).map_err(|_| OmcpError::Crypto("xchacha key"))?;
    c.decrypt(
        XNonce::from_slice(&sealed[..24]),
        Payload {
            msg: &sealed[24..],
            aad,
        },
    )
    .map(Zeroizing::new)
    .map_err(|_| OmcpError::StoreAuth)
}

/// Encrypted local store. Not `Sync`: keep it behind a mutex.
pub struct Store {
    conn: Connection,
    row_key: Zeroizing<[u8; 32]>,
    id_key: Zeroizing<[u8; 32]>,
}

impl Store {
    /// Open (creating and migrating if needed) the store at `path`.
    pub fn open(path: &Path, storage_key: &[u8; 32]) -> Result<Self> {
        Self::init(Connection::open(path).map_err(db)?, storage_key)
    }

    /// In-memory store (tests, or a node without persistence).
    pub fn open_in_memory(storage_key: &[u8; 32]) -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(db)?, storage_key)
    }

    fn init(conn: Connection, storage_key: &[u8; 32]) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA secure_delete = ON; PRAGMA journal_mode = DELETE; PRAGMA temp_store = MEMORY;",
        )
        .map_err(db)?;
        let mut s = Self {
            conn,
            row_key: Zeroizing::new(blake3::derive_key(CTX_ROW, storage_key)),
            id_key: Zeroizing::new(blake3::derive_key(CTX_ID, storage_key)),
        };
        s.migrate()?;
        s.check_key()?;
        Ok(s)
    }

    fn migrate(&mut self) -> Result<()> {
        let v: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db)?;
        if v > SCHEMA_VERSION {
            return Err(OmcpError::Store("store schema is newer than this app"));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(v as usize) {
            let tx = self.conn.transaction().map_err(db)?;
            tx.execute_batch(sql).map_err(db)?;
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))
                .map_err(db)?;
            tx.commit().map_err(db)?;
        }
        Ok(())
    }

    /// The key-check row lives at a fixed, unkeyed id so a wrong storage key is
    /// detected (AEAD failure) instead of silently starting an empty store.
    fn check_key(&self) -> Result<()> {
        const RID: [u8; 32] = [0; 32];
        let aad = Self::aad(Table::Meta, &RID);
        let ct: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT ct FROM meta WHERE id = ?1",
                params![RID.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        match ct {
            Some(c) => match xopen(&self.row_key, &aad, &c) {
                Ok(p) if p.as_slice() == KEY_CHECK => Ok(()),
                _ => Err(OmcpError::StoreAuth),
            },
            None => {
                let c = xseal(&self.row_key, &aad, KEY_CHECK)?;
                self.conn
                    .execute(
                        "INSERT INTO meta(id, grp, seq, ct) VALUES (?1, NULL, 0, ?2)",
                        params![RID.as_slice(), c],
                    )
                    .map_err(db)?;
                Ok(())
            }
        }
    }

    /// `PRAGMA user_version` of the open store.
    pub fn schema_version(&self) -> Result<u32> {
        self.conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(db)
    }

    /// Bind the store to an identity on first use; refuse another identity later.
    pub fn bind_owner(&self, peer_id: &[u8; 32]) -> Result<()> {
        match self.get_raw(Table::Meta, b"owner")? {
            Some(p) if p.as_slice() == peer_id => Ok(()),
            Some(_) => Err(OmcpError::Store("store belongs to another identity")),
            None => self.put_raw(Table::Meta, b"owner", None, peer_id),
        }
    }

    /// v0.1.4a migration: move the owner binding from the legacy X25519 id to the PeerID.
    /// Returns true when the store was bound to the legacy id (an install from
    /// before per-device keys).
    pub fn rebind_owner(&self, legacy: &[u8; 32], peer_id: &[u8; 32]) -> Result<bool> {
        match self.get_raw(Table::Meta, b"owner")? {
            Some(p) if p.as_slice() == legacy => {
                self.put_raw(Table::Meta, b"owner", None, peer_id)?;
                Ok(true)
            }
            _ => self.bind_owner(peer_id).map(|_| false),
        }
    }

    /// v0.1.4a migration: re-key everything stored under a legacy contact id
    /// (their X25519 key) to their PeerID: contact, chat, messages, queue,
    /// outgoing files and burn entries. All-or-nothing.
    pub fn rekey_peer(&self, old: &[u8; 32], new: &[u8; 32]) -> Result<()> {
        self.transaction(|s| {
            if let Some(mut c) = s.get::<ContactRec>(Table::Contacts, old)? {
                c.peer = *new;
                s.put(Table::Contacts, new, None, &c)?;
                s.del_raw(Table::Contacts, old)?;
            }
            if let Some(mut c) = s.get::<ChatRec>(Table::Chats, old)? {
                c.peer = *new;
                s.put(Table::Chats, new, None, &c)?;
                s.del_raw(Table::Chats, old)?;
            }
            for mut m in s.list::<MessageRec>(Table::Messages, Some(old))? {
                m.peer = *new;
                s.put(Table::Messages, &m.id, Some(new), &m)?;
            }
            for mut q in s.list::<OutboxRec>(Table::Outbox, Some(old))? {
                q.peer = *new;
                s.put(Table::Outbox, &q.id, Some(new), &q)?;
            }
            for mut f in s.list::<OutFileRec>(Table::OutFiles, Some(old))? {
                f.peer = *new;
                s.put(Table::OutFiles, &f.id, Some(new), &f)?;
            }
            for mut k in s.kept_media_index()? {
                if &k.peer == old {
                    k.peer = *new;
                    s.put(
                        Table::KeptMedia,
                        &kept_key(b'i', &k.id),
                        Some(KEPT_INDEX_GRP),
                        &k,
                    )?;
                }
            }
            for mut b in s.list::<BurnRec>(Table::Burn, None)? {
                if &b.peer == old {
                    b.peer = *new;
                    s.put(Table::Burn, &b.id, None, &b)?;
                }
            }
            Ok(())
        })
    }

    /// Sealed app/core settings in the `meta` table (chat-backup state, device key).
    pub fn put_meta<T: Serialize>(&self, key: &[u8], v: &T) -> Result<()> {
        let mut k = b"app:".to_vec();
        k.extend_from_slice(key);
        self.put(Table::Meta, &k, None, v)
    }

    pub fn get_meta<T: DeserializeOwned>(&self, key: &[u8]) -> Result<Option<T>> {
        let mut k = b"app:".to_vec();
        k.extend_from_slice(key);
        self.get(Table::Meta, &k)
    }

    // ---- generic sealed rows ----

    fn rid(&self, t: Table, natural: &[u8]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&self.id_key);
        h.update(t.name().as_bytes());
        h.update(&[0]);
        h.update(natural);
        *h.finalize().as_bytes()
    }

    fn gid(&self, natural: &[u8]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&self.id_key);
        h.update(b"grp\0");
        h.update(natural);
        *h.finalize().as_bytes()
    }

    fn aad(t: Table, rid: &[u8; 32]) -> Vec<u8> {
        let mut a = AAD_PREFIX.to_vec();
        a.extend_from_slice(t.name().as_bytes());
        a.push(0);
        a.extend_from_slice(rid);
        a
    }

    fn put_raw(&self, t: Table, natural: &[u8], grp: Option<&[u8]>, plain: &[u8]) -> Result<()> {
        let rid = self.rid(t, natural);
        let ct = xseal(&self.row_key, &Self::aad(t, &rid), plain)?;
        let grp = grp.map(|g| self.gid(g).to_vec());
        let sql = format!(
            "INSERT INTO {n}(id, grp, seq, ct) VALUES (?1, ?2, (SELECT IFNULL(MAX(seq), 0) + 1 FROM {n}), ?3)
             ON CONFLICT(id) DO UPDATE SET ct = excluded.ct, grp = excluded.grp",
            n = t.name()
        );
        self.conn
            .execute(&sql, params![rid.as_slice(), grp, ct])
            .map_err(db)?;
        Ok(())
    }

    fn get_raw(&self, t: Table, natural: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>> {
        let rid = self.rid(t, natural);
        let ct: Option<Vec<u8>> = self
            .conn
            .query_row(
                &format!("SELECT ct FROM {} WHERE id = ?1", t.name()),
                params![rid.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        ct.map(|c| xopen(&self.row_key, &Self::aad(t, &rid), &c))
            .transpose()
    }

    fn del_raw(&self, t: Table, natural: &[u8]) -> Result<()> {
        let rid = self.rid(t, natural);
        self.conn
            .execute(
                &format!("DELETE FROM {} WHERE id = ?1", t.name()),
                params![rid.as_slice()],
            )
            .map_err(db)?;
        Ok(())
    }

    fn list_raw(&self, t: Table, grp: Option<&[u8]>) -> Result<Vec<Zeroizing<Vec<u8>>>> {
        let rows: Vec<(Vec<u8>, Vec<u8>)> = match grp {
            Some(g) => {
                let g = self.gid(g);
                let mut st = self
                    .conn
                    .prepare(&format!(
                        "SELECT id, ct FROM {} WHERE grp = ?1 ORDER BY seq",
                        t.name()
                    ))
                    .map_err(db)?;
                let it = st
                    .query_map(params![g.as_slice()], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(db)?;
                it.collect::<rusqlite::Result<_>>().map_err(db)?
            }
            None => {
                let mut st = self
                    .conn
                    .prepare(&format!("SELECT id, ct FROM {} ORDER BY seq", t.name()))
                    .map_err(db)?;
                let it = st
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map_err(db)?;
                it.collect::<rusqlite::Result<_>>().map_err(db)?
            }
        };
        rows.into_iter()
            .map(|(id, ct)| {
                let rid: [u8; 32] = id.try_into().map_err(|_| OmcpError::StoreAuth)?;
                xopen(&self.row_key, &Self::aad(t, &rid), &ct)
            })
            .collect()
    }

    fn put<T: Serialize>(&self, t: Table, natural: &[u8], grp: Option<&[u8]>, v: &T) -> Result<()> {
        self.put_raw(t, natural, grp, &cbor(v)?)
    }

    fn get<T: DeserializeOwned>(&self, t: Table, natural: &[u8]) -> Result<Option<T>> {
        self.get_raw(t, natural)?.map(|p| uncbor(&p)).transpose()
    }

    fn list<T: DeserializeOwned>(&self, t: Table, grp: Option<&[u8]>) -> Result<Vec<T>> {
        self.list_raw(t, grp)?.iter().map(|p| uncbor(p)).collect()
    }

    /// Run `f` in one transaction (all-or-nothing writes).
    pub fn transaction<R>(&self, f: impl FnOnce(&Self) -> Result<R>) -> Result<R> {
        self.conn.execute_batch("BEGIN IMMEDIATE").map_err(db)?;
        match f(self) {
            Ok(r) => {
                self.conn.execute_batch("COMMIT").map_err(db)?;
                Ok(r)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    // ---- contacts ----

    pub fn put_contact(&self, c: &ContactRec) -> Result<()> {
        self.put(Table::Contacts, &c.peer, None, c)
    }
    pub fn delete_contact(&self, peer: &[u8; 32]) -> Result<()> {
        self.del_raw(Table::Contacts, peer)
    }
    pub fn contacts(&self) -> Result<Vec<ContactRec>> {
        self.list(Table::Contacts, None)
    }

    // ---- chats ----

    pub fn put_chat(&self, c: &ChatRec) -> Result<()> {
        self.put(Table::Chats, &c.peer, None, c)
    }
    pub fn chat(&self, peer: &[u8; 32]) -> Result<Option<ChatRec>> {
        self.get(Table::Chats, peer)
    }
    pub fn chats(&self) -> Result<Vec<ChatRec>> {
        self.list(Table::Chats, None)
    }

    // ---- messages ----

    /// Insert or update a message (keeps its original position). Burn messages
    /// never store content: `text` is cleared defensively.
    pub fn put_message(&self, m: &MessageRec) -> Result<()> {
        let mut m = m.clone();
        if m.burn.is_some() {
            m.text.zeroize();
            m.text.clear();
        }
        self.put(Table::Messages, &m.id, Some(&m.peer), &m)?;
        let chat = self.chat(&m.peer)?;
        if chat.as_ref().is_none_or(|c| m.time_ms > c.last_ms) {
            self.put_chat(&ChatRec {
                peer: m.peer,
                last_ms: m.time_ms,
                last_read_ms: chat.map_or(0, |c| c.last_read_ms),
            })?;
        }
        Ok(())
    }
    pub fn message(&self, id: &[u8; 16]) -> Result<Option<MessageRec>> {
        self.get(Table::Messages, id)
    }
    pub fn delete_message(&self, id: &[u8; 16]) -> Result<()> {
        self.del_raw(Table::Messages, id)
    }
    /// Messages of one chat, oldest first.
    pub fn messages(&self, peer: &[u8; 32]) -> Result<Vec<MessageRec>> {
        self.list(Table::Messages, Some(peer))
    }
    pub fn all_messages(&self) -> Result<Vec<MessageRec>> {
        self.list(Table::Messages, None)
    }

    // ---- outbound queue ----

    /// Queue an outbound message. With `burn_key` (the message's per-message burn
    /// key) the payload is sealed under a sub-key of it, so burning the message
    /// also shreds its unsent copy.
    pub fn put_outbox(&self, q: &OutboxRec, burn_key: Option<&[u8; 32]>) -> Result<()> {
        let stored = match burn_key {
            None => q.clone(),
            Some(k) => {
                let sub = Zeroizing::new(blake3::derive_key(CTX_BURN_OUTBOX, k));
                let mut s = q.clone();
                s.inner.zeroize();
                s.inner = xseal(&sub, &q.id, &q.inner)?;
                s.burn = true;
                s
            }
        };
        self.put(Table::Outbox, &q.id, Some(&q.peer), &stored)
    }
    pub fn delete_outbox(&self, id: &[u8; 16]) -> Result<()> {
        self.del_raw(Table::Outbox, id)
    }
    /// The pending queue, oldest first. Burn entries whose key is gone are dropped.
    pub fn outbox(&self) -> Result<Vec<OutboxRec>> {
        let mut out = Vec::new();
        for mut q in self.list::<OutboxRec>(Table::Outbox, None)? {
            if q.burn {
                let Some(k) = self.burn_key(&q.id)? else {
                    self.delete_outbox(&q.id)?;
                    continue;
                };
                let sub = Zeroizing::new(blake3::derive_key(CTX_BURN_OUTBOX, &k[..]));
                let plain = xopen(&sub, &q.id, &q.inner)?;
                q.inner.zeroize();
                q.inner = plain.to_vec();
                q.burn = false;
            }
            out.push(q);
        }
        Ok(out)
    }

    // ---- mesh relay carry store (v0.2) ----

    /// Persist one carry-store item (an opaque envelope, sealed at rest).
    pub fn put_carry(&self, c: &CarryRec) -> Result<()> {
        self.put(
            Table::Relay,
            &kept_key(b'c', &c.id),
            Some(RELAY_CARRY_GRP),
            c,
        )
    }
    pub fn delete_carry(&self, id: &[u8; 16]) -> Result<()> {
        self.del_raw(Table::Relay, &kept_key(b'c', id))
    }
    pub fn carry_items(&self) -> Result<Vec<CarryRec>> {
        self.list(Table::Relay, Some(RELAY_CARRY_GRP))
    }
    /// Remember a mesh envelope id delivered to us until `until_ms`.
    pub fn put_mesh_seen(&self, id: &[u8; 16], until_ms: u64) -> Result<()> {
        let r = MeshSeenRec { id: *id, until_ms };
        self.put(Table::Relay, &kept_key(b's', id), Some(RELAY_SEEN_GRP), &r)
    }
    /// Delivered ids still within their lifetime; expired rows are deleted.
    pub fn mesh_seen(&self, now_ms: u64) -> Result<Vec<MeshSeenRec>> {
        let mut live = Vec::new();
        for r in self.list::<MeshSeenRec>(Table::Relay, Some(RELAY_SEEN_GRP))? {
            if r.until_ms > now_ms {
                live.push(r);
            } else {
                self.del_raw(Table::Relay, &kept_key(b's', &r.id))?;
            }
        }
        Ok(live)
    }

    // ---- outgoing file transfers (so they resume after a restart) ----

    pub fn put_out_file<C: AsRef<[u8]>>(&self, f: &OutFileRec, chunks: &[C]) -> Result<()> {
        self.transaction(|s| {
            s.put(Table::OutFiles, &f.id, Some(&f.peer), f)?;
            for (i, c) in chunks.iter().enumerate() {
                s.put_raw(
                    Table::FileChunks,
                    &chunk_id(&f.id, i as u32),
                    Some(&f.id),
                    c.as_ref(),
                )?;
            }
            Ok(())
        })
    }
    pub fn delete_out_file(&self, id: &[u8; 16]) -> Result<()> {
        let Some(f) = self.get::<OutFileRec>(Table::OutFiles, id)? else {
            return Ok(());
        };
        self.transaction(|s| {
            for i in 0..f.total.max(1) {
                s.del_raw(Table::FileChunks, &chunk_id(id, i))?;
            }
            s.del_raw(Table::OutFiles, id)
        })
    }
    /// Outgoing transfers (oldest first) with their sealed chunk payloads.
    pub fn out_files(&self) -> Result<Vec<(OutFileRec, SealedChunks)>> {
        let files: Vec<OutFileRec> = self.list(Table::OutFiles, None)?;
        files
            .into_iter()
            .map(|f| {
                let chunks = self.list_raw(Table::FileChunks, Some(&f.id))?;
                Ok((f, chunks))
            })
            .collect()
    }

    // ---- kept copies of sent media ----

    /// Keep a sealed local copy of media we sent (never burn content: callers
    /// MUST NOT pass burn/view-once items), then evict to `limits`.
    pub fn put_kept_media(
        &self,
        rec: &KeptMediaRec,
        bytes: &[u8],
        limits: KeptMediaLimits,
        now_ms: u64,
    ) -> Result<Vec<[u8; 16]>> {
        self.transaction(|s| {
            s.put_raw(Table::KeptMedia, &kept_key(b'd', &rec.id), None, bytes)?;
            s.put(
                Table::KeptMedia,
                &kept_key(b'i', &rec.id),
                Some(KEPT_INDEX_GRP),
                rec,
            )
        })?;
        self.evict_kept_media(limits, now_ms)
    }

    /// The kept copy's bytes, if still kept.
    pub fn kept_media(&self, id: &[u8; 16]) -> Result<Option<Zeroizing<Vec<u8>>>> {
        if self
            .get_raw(Table::KeptMedia, &kept_key(b'i', id))?
            .is_none()
        {
            return Ok(None);
        }
        self.get_raw(Table::KeptMedia, &kept_key(b'd', id))
    }

    /// Kept copies, oldest first.
    pub fn kept_media_index(&self) -> Result<Vec<KeptMediaRec>> {
        let mut v: Vec<KeptMediaRec> = self.list(Table::KeptMedia, Some(KEPT_INDEX_GRP))?;
        v.sort_by_key(|k| k.kept_ms);
        Ok(v)
    }

    pub fn delete_kept_media(&self, id: &[u8; 16]) -> Result<()> {
        self.transaction(|s| {
            s.del_raw(Table::KeptMedia, &kept_key(b'd', id))?;
            s.del_raw(Table::KeptMedia, &kept_key(b'i', id))
        })
    }

    /// Drop copies older than `max_days`, then the oldest until the total is
    /// within `max_bytes`. Returns the evicted message ids.
    pub fn evict_kept_media(&self, limits: KeptMediaLimits, now_ms: u64) -> Result<Vec<[u8; 16]>> {
        let idx = self.kept_media_index()?;
        let cutoff = now_ms.saturating_sub(u64::from(limits.max_days) * DAY_MS);
        let mut total: u64 = idx.iter().map(|k| k.size).sum();
        let mut gone = Vec::new();
        for k in &idx {
            if k.kept_ms < cutoff || total > limits.max_bytes {
                self.delete_kept_media(&k.id)?;
                total -= k.size;
                gone.push(k.id);
            }
        }
        Ok(gone)
    }

    // ---- burn after read ----

    /// Persist a live burn entry: state row + separate per-message key row.
    pub fn put_burn(
        &self,
        s: &BurnSnapshot,
        peer: &[u8; 32],
        dir: BurnDir,
        mime: Option<&str>,
    ) -> Result<()> {
        let rec = BurnRec {
            id: s.msg_id,
            peer: *peer,
            dir: dir as u8,
            mime: mime.map(String::from),
            header: s.header.encode().to_vec(),
            expires_at_ms: s.expires_at_ms,
            burn_at_ms: s.burn_at_ms,
            opened: s.opened,
            sealed: s.sealed.clone(),
        };
        self.transaction(|st| {
            st.put(Table::Burn, &s.msg_id, None, &rec)?;
            st.put_raw(Table::BurnKeys, &s.msg_id, None, s.key.as_ref())
        })
    }

    /// Crypto-shred a burn message: its key row, content row and any unsent copy.
    pub fn delete_burn(&self, id: &[u8; 16]) -> Result<()> {
        self.transaction(|s| {
            s.del_raw(Table::BurnKeys, id)?;
            s.del_raw(Table::Burn, id)?;
            s.del_raw(Table::Outbox, id)
        })
    }

    fn burn_key(&self, id: &[u8; 16]) -> Result<Option<Zeroizing<[u8; 32]>>> {
        let Some(k) = self.get_raw(Table::BurnKeys, id)? else {
            return Ok(None);
        };
        let arr: [u8; 32] = k[..].try_into().map_err(|_| OmcpError::StoreAuth)?;
        Ok(Some(Zeroizing::new(arr)))
    }

    /// All persisted burn entries. Rows whose key is missing are dropped.
    pub fn burns(&self) -> Result<Vec<StoredBurn>> {
        let mut out = Vec::new();
        for r in self.list::<BurnRec>(Table::Burn, None)? {
            let Some(key) = self.burn_key(&r.id)? else {
                self.delete_burn(&r.id)?;
                continue;
            };
            out.push(StoredBurn {
                snapshot: BurnSnapshot {
                    msg_id: r.id,
                    header: BurnHeader::decode(&r.header)?,
                    key,
                    sealed: r.sealed.clone(),
                    expires_at_ms: r.expires_at_ms,
                    burn_at_ms: r.burn_at_ms,
                    opened: r.opened,
                },
                peer: r.peer,
                dir: if r.dir == BurnDir::Out as u8 {
                    BurnDir::Out
                } else {
                    BurnDir::In
                },
                mime: r.mime.clone(),
            });
        }
        Ok(out)
    }

    // ---- backup snapshot (PROTOCOL §11 item 9; REQUIREMENTS B5) ----

    /// Plaintext CBOR history for the encrypted backup: chats, non-burn messages
    /// and the non-burn, non-control outbound queue. Burn content, burn message
    /// placeholders, keys and outgoing file payloads are never included.
    pub fn export_history(&self) -> Result<Zeroizing<Vec<u8>>> {
        let h = History {
            v: HISTORY_VERSION,
            chats: self.chats()?,
            messages: self
                .all_messages()?
                .into_iter()
                .filter(|m| m.burn.is_none())
                .collect(),
            outbox: self
                .list::<OutboxRec>(Table::Outbox, None)?
                .into_iter()
                .filter(|q| !q.burn && !q.control)
                .collect(),
        };
        cbor(&h)
    }

    /// Merge a history snapshot from a backup. Received-file paths from the old
    /// device are dropped (the files themselves are not in the backup).
    pub fn import_history(&self, bytes: &[u8]) -> Result<()> {
        let h: History = uncbor(bytes)?;
        if h.v != HISTORY_VERSION {
            return Err(OmcpError::Store("unsupported history version"));
        }
        self.transaction(|s| {
            for c in &h.chats {
                s.put_chat(c)?;
            }
            for m in &h.messages {
                if m.burn.is_some() {
                    continue;
                }
                let mut m = m.clone();
                if let Some(f) = m.file.as_mut() {
                    f.local_path = None;
                    if !matches!(f.state, FILE_SENT | FILE_RECEIVED) {
                        f.state = FILE_FAILED;
                    }
                }
                s.put_message(&m)?;
            }
            for q in &h.outbox {
                if !q.burn && !q.control {
                    s.put_outbox(q, None)?;
                }
            }
            Ok(())
        })
    }
}

/// Decrypted chunk payloads of one outgoing transfer, in order.
pub type SealedChunks = Vec<Zeroizing<Vec<u8>>>;

fn chunk_id(file: &[u8; 16], i: u32) -> [u8; 20] {
    let mut b = [0u8; 20];
    b[..16].copy_from_slice(file);
    b[16..].copy_from_slice(&i.to_be_bytes());
    b
}

#[derive(Serialize, Deserialize)]
struct History {
    v: u8,
    chats: Vec<ChatRec>,
    messages: Vec<MessageRec>,
    outbox: Vec<OutboxRec>,
}

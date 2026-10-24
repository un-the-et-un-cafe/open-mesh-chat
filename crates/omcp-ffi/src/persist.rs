//! Persistence glue: every state change on [`crate::OmcpNode`] is written
//! through to the encrypted [`omcp_core::store::Store`], and a node opened with
//! [`crate::OmcpNode::open`] reloads contacts, queue, burn state and outgoing
//! transfers from it. The storage key comes from the platform keystore.

use omcp_core::burn::BurnHeader;
use omcp_core::store::{
    self as st, BurnDir, ContactRec, FileRec, MessageRec, OutFileRec, OutboxRec, Store,
};
use zeroize::Zeroizing;

use crate::files::OutgoingFile;
use crate::{BurnMeta, BurnSpec, Contact, FfiError, LinkOutput, NodeState, Queued};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MessageStatus {
    Queued,
    Sent,
    Received,
    /// In-chat system notice (restore notice, RS1); `text` = `restored:<ms>`.
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StoredBurnState {
    None,
    Unopened,
    Open,
    /// View-once media placeholder ("🔥 Photo opened").
    Opened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StoredFileState {
    Queued,
    Sending,
    Receiving,
    Paused,
    Sent,
    Received,
    Failed,
    IntegrityFailed,
    /// Restored without bytes: missing-media placeholder (CB3).
    Missing,
    /// Placeholder re-requested from the partner.
    Requested,
    /// The partner no longer has it ("Ask to resend").
    Unavailable,
}

/// File-transfer metadata; `local_path` = where the app saved a received file.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoredFile {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub state: StoredFileState,
    pub done_bytes: u64,
    pub local_path: Option<String>,
}

/// A message as persisted. Burn messages never carry content here: it is only
/// available once, via `open_burn`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoredMessage {
    pub id_hex: String,
    pub contact_id: String,
    pub text: String,
    pub mine: bool,
    pub time_ms: u64,
    pub status: MessageStatus,
    pub mime: Option<String>,
    pub burn: Option<BurnSpec>,
    pub burn_state: StoredBurnState,
    pub burn_at_ms: Option<u64>,
    pub file: Option<StoredFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChatSummary {
    pub contact_id: String,
    pub last_ms: u64,
    pub last_read_ms: u64,
}

pub(crate) fn file_state(s: u8) -> StoredFileState {
    match s {
        st::FILE_QUEUED => StoredFileState::Queued,
        st::FILE_SENDING => StoredFileState::Sending,
        st::FILE_RECEIVING => StoredFileState::Receiving,
        st::FILE_PAUSED => StoredFileState::Paused,
        st::FILE_SENT => StoredFileState::Sent,
        st::FILE_RECEIVED => StoredFileState::Received,
        st::FILE_INTEGRITY_FAILED => StoredFileState::IntegrityFailed,
        st::FILE_MISSING => StoredFileState::Missing,
        st::FILE_REQUESTED => StoredFileState::Requested,
        st::FILE_UNAVAILABLE => StoredFileState::Unavailable,
        _ => StoredFileState::Failed,
    }
}

pub(crate) fn to_stored(m: &MessageRec) -> StoredMessage {
    StoredMessage {
        id_hex: hex::encode(m.id),
        contact_id: hex::encode(m.peer),
        text: m.text.clone(),
        mine: m.mine,
        time_ms: m.time_ms,
        status: match m.status {
            st::STATUS_QUEUED => MessageStatus::Queued,
            st::STATUS_SENT => MessageStatus::Sent,
            st::STATUS_SYSTEM => MessageStatus::System,
            _ => MessageStatus::Received,
        },
        mime: m.mime.clone(),
        burn: m
            .burn
            .as_ref()
            .and_then(|b| BurnHeader::decode(b).ok())
            .map(|h| BurnSpec::from_header(&h)),
        burn_state: match m.burn_state {
            st::BURN_UNOPENED => StoredBurnState::Unopened,
            st::BURN_OPEN => StoredBurnState::Open,
            st::BURN_OPENED => StoredBurnState::Opened,
            _ => StoredBurnState::None,
        },
        burn_at_ms: m.burn_at_ms,
        file: m.file.as_ref().map(|f| StoredFile {
            name: f.name.clone(),
            mime: f.mime.clone(),
            size: f.size,
            state: file_state(f.state),
            done_bytes: f.done_bytes,
            local_path: f.local_path.clone(),
        }),
    }
}

pub(crate) fn is_view_once_media(mime: Option<&str>) -> bool {
    mime.is_some_and(|m| m.starts_with("image/") || m.starts_with("video/"))
}

impl NodeState {
    pub(crate) fn db(&self) -> &Store {
        &self.db
    }

    pub(crate) fn persist_contact(&self, id: &[u8; 32]) -> Result<(), FfiError> {
        if let Some(c) = self.contacts.get(id) {
            self.db.put_contact(&ContactRec {
                peer: *id,
                name: c.name.clone(),
                verified: c.verified,
                device: Some(c.device.clone()),
                dh: Some(c.dh.to_vec()),
            })?;
        }
        Ok(())
    }

    /// Write the live burn entry for `id` (sent or received), or shred it from disk.
    pub(crate) fn persist_burn(&self, id: &[u8; 16]) -> Result<(), FfiError> {
        let meta = self.burn_meta.get(id);
        let (snap, dir) = match (self.burn_in.snapshot(id), self.burn_out.snapshot(id)) {
            (Some(s), _) => (s, BurnDir::In),
            (None, Some(s)) => (s, BurnDir::Out),
            (None, None) => return Ok(self.db.delete_burn(id)?),
        };
        let Some(meta) = meta else {
            return Ok(self.db.delete_burn(id)?);
        };
        Ok(self
            .db
            .put_burn(&snap, &meta.peer, dir, meta.mime.as_deref())?)
    }

    pub(crate) fn persist_queued(&self, peer: &[u8; 32], q: &Queued) -> Result<(), FfiError> {
        let burn_key = self.burn_out.snapshot(&q.envelope_id).map(|s| s.key);
        let rec = OutboxRec {
            id: q.envelope_id,
            peer: *peer,
            created_ms: q.created_ms,
            inner: q.inner.to_vec(),
            control: q.control,
            burn: false,
        };
        Ok(self.db.put_outbox(&rec, burn_key.as_deref())?)
    }

    pub(crate) fn update_message(
        &self,
        id: &[u8; 16],
        f: impl FnOnce(&mut MessageRec),
    ) -> Result<(), FfiError> {
        if let Some(mut m) = self.db.message(id)? {
            f(&mut m);
            self.db.put_message(&m)?;
        }
        Ok(())
    }

    /// The burn copy is gone: drop a text bubble, keep a view-once media placeholder.
    pub(crate) fn burn_message_now(&self, id: &[u8; 16]) -> Result<(), FfiError> {
        let Some(m) = self.db.message(id)? else {
            return Ok(());
        };
        if is_view_once_media(m.mime.as_deref()) {
            self.update_message(id, |m| {
                m.burn_state = st::BURN_OPENED;
                m.burn_at_ms = None;
            })
        } else {
            Ok(self.db.delete_message(id)?)
        }
    }

    pub(crate) fn persist_out_file(
        &self,
        peer: &[u8; 32],
        f: &OutgoingFile,
    ) -> Result<(), FfiError> {
        Ok(self.db.put_out_file(
            &OutFileRec {
                id: f.id,
                peer: *peer,
                name: f.name.clone(),
                size: f.size,
                total: f.total,
                offer: f.offer.to_vec(),
            },
            &f.chunks,
        )?)
    }

    /// Persist file events produced by one `deliver`/`flush` step.
    pub(crate) fn persist_file_events(
        &self,
        out: &LinkOutput,
        from: (usize, usize, usize),
        now: u64,
    ) -> Result<(), FfiError> {
        for p in &out.file_progress[from.0..] {
            let id = crate::parse_msg_id(&p.transfer_id_hex)?;
            let peer = crate::parse_id(&p.contact_id)?;
            let done = if p.total_chunks == 0 {
                p.size
            } else {
                p.size * u64::from(p.done_chunks) / u64::from(p.total_chunks)
            };
            let finished = p.done_chunks >= p.total_chunks;
            match self.db.message(&id)? {
                None if p.incoming => self.db.put_message(&MessageRec {
                    id,
                    peer,
                    mine: false,
                    time_ms: now,
                    status: st::STATUS_RECEIVED,
                    text: String::new(),
                    mime: None,
                    burn: None,
                    burn_state: st::BURN_NONE,
                    burn_at_ms: None,
                    file: Some(FileRec {
                        name: p.name.clone(),
                        mime: String::new(),
                        size: p.size,
                        state: st::FILE_RECEIVING,
                        done_bytes: done,
                        local_path: None,
                        hash: None,
                        locator: None,
                    }),
                })?,
                None => {}
                Some(_) => self.update_message(&id, |m| {
                    let Some(f) = m.file.as_mut() else { return };
                    if matches!(f.state, st::FILE_RECEIVED | st::FILE_INTEGRITY_FAILED) {
                        return;
                    }
                    f.done_bytes = done;
                    f.state = if p.incoming {
                        st::FILE_RECEIVING
                    } else if finished {
                        m.status = st::STATUS_SENT;
                        st::FILE_SENT
                    } else {
                        st::FILE_SENDING
                    };
                })?,
            }
        }
        for r in &out.files[from.1..] {
            let id = crate::parse_msg_id(&r.transfer_id_hex)?;
            let peer = crate::parse_id(&r.contact_id)?;
            let file = FileRec {
                name: r.name.clone(),
                mime: r.mime.clone(),
                size: r.size,
                state: st::FILE_RECEIVED,
                done_bytes: r.size,
                local_path: None,
                hash: Some(blake3::hash(&r.bytes).as_bytes().to_vec()),
                locator: Some(id.to_vec()),
            };
            match self.db.message(&id)? {
                Some(_) => self.update_message(&id, |m| m.file = Some(file))?,
                None => self.db.put_message(&MessageRec {
                    id,
                    peer,
                    mine: false,
                    time_ms: now,
                    status: st::STATUS_RECEIVED,
                    text: String::new(),
                    mime: None,
                    burn: None,
                    burn_state: st::BURN_NONE,
                    burn_at_ms: None,
                    file: Some(file),
                })?,
            }
        }
        for e in &out.file_failures[from.2..] {
            let id = crate::parse_msg_id(&e.transfer_id_hex)?;
            self.update_message(&id, |m| {
                if let Some(f) = m.file.as_mut() {
                    f.state = if e.reason == "integrity" {
                        st::FILE_INTEGRITY_FAILED
                    } else {
                        st::FILE_FAILED
                    };
                }
            })?;
        }
        Ok(())
    }

    /// Link to `peer` dropped: running transfers wait for the next link.
    pub(crate) fn pause_transfers(&self, peer: &[u8; 32]) -> Result<(), FfiError> {
        for m in self.db.messages(peer)? {
            if m.file
                .as_ref()
                .is_some_and(|f| matches!(f.state, st::FILE_SENDING | st::FILE_RECEIVING))
            {
                self.update_message(&m.id, |m| {
                    if let Some(f) = m.file.as_mut() {
                        // A re-fetch that didn't finish goes back to the placeholder.
                        f.state = if f.state == st::FILE_RECEIVING && f.hash.is_some() {
                            st::FILE_MISSING
                        } else {
                            st::FILE_PAUSED
                        };
                    }
                })?;
            } else if m
                .file
                .as_ref()
                .is_some_and(|f| f.state == st::FILE_REQUESTED)
            {
                self.update_message(&m.id, |m| {
                    if let Some(f) = m.file.as_mut() {
                        f.state = st::FILE_MISSING;
                    }
                })?;
            }
        }
        Ok(())
    }

    /// Rebuild the in-memory state from the store (node start, after a restore).
    pub(crate) fn load(&mut self, now_ms: u64) -> Result<(), FfiError> {
        for c in self.db.contacts()? {
            self.contacts.insert(
                c.peer,
                Contact {
                    name: c.name.clone(),
                    verified: c.verified,
                    device: c.device.clone().unwrap_or_default(),
                    dh: c
                        .dh
                        .as_deref()
                        .and_then(|d| d.try_into().ok())
                        .unwrap_or(c.peer),
                },
            );
        }
        for b in self.db.burns()? {
            let id = b.snapshot.msg_id;
            self.burn_meta.insert(
                id,
                BurnMeta {
                    peer: b.peer,
                    mime: b.mime,
                },
            );
            match b.dir {
                BurnDir::In => self.burn_in.restore(b.snapshot),
                BurnDir::Out => self.burn_out.restore(b.snapshot),
            }
        }
        self.queues.clear();
        for q in self.db.outbox()? {
            self.queues.entry(q.peer).or_default().push_back(Queued {
                envelope_id: q.id,
                created_ms: q.created_ms,
                inner: Zeroizing::new(q.inner.clone()),
                control: q.control,
            });
        }
        self.files.out.clear();
        for (f, chunks) in self.db.out_files()? {
            self.files
                .out
                .entry(f.peer)
                .or_default()
                .push_back(OutgoingFile {
                    id: f.id,
                    name: f.name.clone(),
                    size: f.size,
                    total: f.total,
                    cursor: 0,
                    link: None,
                    offer: Zeroizing::new(f.offer.clone()),
                    chunks,
                });
        }
        // Timers that ran out while the app was closed, then any message whose
        // burn copy no longer exists; interrupted transfers resume on the next link.
        let mut due = self.burn_in.tick(now_ms);
        due.extend(self.burn_out.tick(now_ms));
        for id in &due {
            self.db.delete_burn(id)?;
        }
        for m in self.db.all_messages()? {
            if m.burn.is_some()
                && m.burn_state != st::BURN_OPENED
                && !self.burn_in.is_live(&m.id)
                && !self.burn_out.is_live(&m.id)
            {
                self.burn_message_now(&m.id)?;
            } else if m
                .file
                .as_ref()
                .is_some_and(|f| matches!(f.state, st::FILE_SENDING | st::FILE_RECEIVING))
            {
                self.pause_transfers(&m.peer)?;
            }
        }
        self.load_mesh(now_ms)
    }
}

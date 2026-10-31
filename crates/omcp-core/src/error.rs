//! Error types for `omcp-core`.

use thiserror::Error;

/// Errors returned by framing, negotiation, handshake, BLE, MLS, and file APIs.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OmcpError {
    #[error("buffer truncated: need at least {need} bytes, have {have}")]
    Truncated { need: usize, have: usize },

    #[error("envelope too large: ciphertext_len {len} exceeds max {max}")]
    Oversized { len: u32, max: u32 },

    #[error("header_aad length mismatch: declared {declared}, remaining {remaining}")]
    HeaderAadLength { declared: u16, remaining: usize },

    #[error("bad magic: expected OMCP, got {0:?}")]
    BadMagic([u8; 4]),

    #[error("unsupported envelope version {0}")]
    UnknownVersion(u16),

    #[error("unsupported suite_id {0:#06x}")]
    UnsupportedSuite(u16),

    #[error("no overlapping protocol versions")]
    NoVersionOverlap,

    #[error("no overlapping cipher suites")]
    NoSuiteOverlap,

    #[error("handshake message malformed: {0}")]
    HandshakeMalformed(&'static str),

    #[error("handshake AEAD failure (tamper or wrong keys)")]
    HandshakeAead,

    #[error("handshake in unexpected state")]
    HandshakeState,

    #[error("cryptographic operation failed: {0}")]
    Crypto(&'static str),

    #[error("BLE fragment malformed: {0}")]
    FragmentMalformed(&'static str),

    #[error("BLE link AEAD failure (tamper or wrong keys)")]
    LinkAead,

    #[error("application AEAD failure (tamper, wrong keys, or bad UTF-8)")]
    AppAead,

    #[error("MLS operation failed: {0}")]
    Mls(&'static str),

    #[error("private invite: {0}")]
    Invite(&'static str),

    #[error("MLS-PQ extension malformed: {0}")]
    MlsPqMalformed(&'static str),

    #[error("backup error: {0}")]
    Backup(&'static str),

    #[error("backup format v{0} is newer than this app supports")]
    BackupTooNew(u8),

    #[error("backup authentication failed (wrong password or key)")]
    BackupAuth,

    #[error("file too large: {size} bytes exceeds max {max} bytes")]
    FileTooLarge { size: u64, max: u64 },

    #[error("file chunk protocol error: {0}")]
    File(&'static str),

    #[error("file content hash mismatch")]
    FileHashMismatch,

    #[error("gateway error: {0}")]
    Gateway(&'static str),

    #[error("rendezvous error: {0}")]
    Rendezvous(&'static str),

    #[error("rendezvous token rejected: {0}")]
    TokenRejected(&'static str),

    /// Scalable sync (§22) failure.
    #[error("sync: {0}")]
    Sync(&'static str),

    #[error("local store error: {0}")]
    Store(&'static str),

    #[error("local store authentication failed (wrong storage key or tampered row)")]
    StoreAuth,

    #[error("burn-after-read error: {0}")]
    Burn(&'static str),
}

/// Crate-wide result alias.
pub type Result<T> = core::result::Result<T, OmcpError>;

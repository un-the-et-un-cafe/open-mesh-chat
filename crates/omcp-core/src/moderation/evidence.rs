//! Evidence file for reports to the app team (PROTOCOL §23.10.1, §23.10.3).
//!
//! Deterministic CBOR (RFC 8949 §4.2.1), extension `.omcp-evidence`, media
//! type `application/vnd.omcp.evidence+cbor`. Signed by the reporter
//! (`"OMCP-evidence-v1" ‖ 0x00 ‖ CBOR(file without key 9)`). The file names
//! the group only by `group_ref` (plus the random group ID inside the signed
//! MLS bytes) and never contains a member list, roster, ratchet tree, group
//! secret or plaintext group metadata (MR12, MR19).
//!
//! Reported messages are verified with the MLS sender signature every
//! private-group message already carries (no franking, §23.10.1(4)).
//! Reports exist only for private groups, never for 1:1 chats (§23.10.2(0)).

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use super::{group_ref, h256};
use crate::error::{OmcpError, Result};

pub const EVIDENCE_EXTENSION: &str = "omcp-evidence";
pub const EVIDENCE_MIME: &str = "application/vnd.omcp.evidence+cbor";
pub const MAX_MESSAGES: usize = 500;
pub const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
pub const MAX_NOTE: usize = 2000;
/// Team-side retention of evidence after a decision (§23.10.4(4)).
pub const EVIDENCE_RETENTION_MS: u64 = 90 * 24 * 60 * 60 * 1000;
const SIG_DOMAIN: &[u8] = b"OMCP-evidence-v1\x00";
/// MLS `wire_format` of private-group application messages.
const WIRE_FORMAT_PRIVATE_MESSAGE: u16 = 2;
const MLS10: u16 = 1;

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

/// The group reference written into an evidence file. The one place that
/// decides what names the group (Jakob: the random group ID may appear inside
/// the signed MLS bytes; nothing readable ever does).
pub fn evidence_group_ref(group_id: &[u8]) -> [u8; 32] {
    group_ref(group_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaItem {
    pub mime: String,
    pub bytes: Vec<u8>,
    pub hash: [u8; 32],
}

/// One reported message with the bytes needed to re-check its MLS sender
/// signature (§23.10.1(2)); receiving apps keep these with each message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportedMessage {
    pub sender_id: [u8; 32],
    /// TLS-encoded MLS `FramedContent`, exactly as signed.
    pub framed_content: Vec<u8>,
    pub group_context_hash: [u8; 32],
    /// TLS-encoded `GroupContext` (random ID, epoch and hashes only).
    pub group_context: Vec<u8>,
    pub signature: Vec<u8>,
    /// TLS-encoded sender `LeafNode`.
    pub leaf_node: Vec<u8>,
    pub body: Vec<u8>,
    pub media: Vec<MediaItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReporterContact {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberDisclosure {
    pub sender_id: [u8; 32],
    pub label: Option<String>,
}

/// `EvidenceFileV1` (§23.10.3(3)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceFile {
    pub group_ref: [u8; 32],
    pub export_ms: u64,
    pub messages: Vec<ReportedMessage>,
    pub reason_code: u8,
    pub note: Option<String>,
    pub contact: Option<ReporterContact>,
    /// Only members the reporter explicitly chose to name.
    pub disclosures: Vec<MemberDisclosure>,
    pub reporter_key: [u8; 32],
    pub reporter_sig: [u8; 64],
}

// ---------------------------------------------------------------- CBOR ----

enum V {
    U(u64),
    B(Vec<u8>),
    T(String),
    A(Vec<V>),
    M(Vec<(u64, V)>),
}

fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let m = major << 5;
    if n < 24 {
        out.push(m | n as u8);
    } else if n <= 0xff {
        out.extend_from_slice(&[m | 24, n as u8]);
    } else if n <= 0xffff {
        out.push(m | 25);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= 0xffff_ffff {
        out.push(m | 26);
        out.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&n.to_be_bytes());
    }
}

fn enc(v: &V, out: &mut Vec<u8>) {
    match v {
        V::U(n) => head(out, 0, *n),
        V::B(b) => {
            head(out, 2, b.len() as u64);
            out.extend_from_slice(b);
        }
        V::T(s) => {
            head(out, 3, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        V::A(a) => {
            head(out, 4, a.len() as u64);
            for x in a {
                enc(x, out);
            }
        }
        V::M(m) => {
            // Integer keys in ascending order = deterministic key order.
            let mut m: Vec<&(u64, V)> = m.iter().collect();
            m.sort_by_key(|(k, _)| *k);
            head(out, 5, m.len() as u64);
            for (k, x) in m {
                head(out, 0, *k);
                enc(x, out);
            }
        }
    }
}

struct Dec<'a>(&'a [u8]);
impl Dec<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.0.len() < n {
            return err("evidence truncated");
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn head(&mut self) -> Result<(u8, u64)> {
        let b = self.take(1)?[0];
        let (major, ai) = (b >> 5, b & 31);
        let n = match ai {
            0..=23 => ai as u64,
            24 => self.take(1)?[0] as u64,
            25 => u16::from_be_bytes(self.take(2)?.try_into().expect("2")) as u64,
            26 => u32::from_be_bytes(self.take(4)?.try_into().expect("4")) as u64,
            27 => u64::from_be_bytes(self.take(8)?.try_into().expect("8")),
            _ => return err("evidence cbor head"),
        };
        Ok((major, n))
    }
    fn val(&mut self, depth: u8) -> Result<V> {
        if depth > 8 {
            return err("evidence too deep");
        }
        let (major, n) = self.head()?;
        let len = usize::try_from(n).map_err(|_| OmcpError::Sync("evidence len"))?;
        Ok(match major {
            0 => V::U(n),
            2 => V::B(self.take(len)?.to_vec()),
            3 => V::T(
                String::from_utf8(self.take(len)?.to_vec())
                    .map_err(|_| OmcpError::Sync("evidence utf-8"))?,
            ),
            4 => {
                if len > self.0.len() {
                    return err("evidence array len");
                }
                V::A(
                    (0..len)
                        .map(|_| self.val(depth + 1))
                        .collect::<Result<_>>()?,
                )
            }
            5 => {
                if len > self.0.len() {
                    return err("evidence map len");
                }
                let mut m = Vec::with_capacity(len);
                for _ in 0..len {
                    let (km, k) = self.head()?;
                    if km != 0 {
                        return err("evidence map key");
                    }
                    m.push((k, self.val(depth + 1)?));
                }
                V::M(m)
            }
            _ => return err("evidence cbor type"),
        })
    }
}

fn map(v: V) -> Result<Vec<(u64, V)>> {
    match v {
        V::M(m) => Ok(m),
        _ => err("evidence: map expected"),
    }
}
fn get(m: &mut Vec<(u64, V)>, k: u64) -> Option<V> {
    let i = m.iter().position(|(x, _)| *x == k)?;
    Some(m.remove(i).1)
}
fn need(m: &mut Vec<(u64, V)>, k: u64) -> Result<V> {
    get(m, k).ok_or(OmcpError::Sync("evidence: missing key"))
}
fn u(v: V) -> Result<u64> {
    match v {
        V::U(n) => Ok(n),
        _ => err("evidence: uint expected"),
    }
}
fn b(v: V) -> Result<Vec<u8>> {
    match v {
        V::B(b) => Ok(b),
        _ => err("evidence: bstr expected"),
    }
}
fn b32(v: V) -> Result<[u8; 32]> {
    b(v)?
        .try_into()
        .map_err(|_| OmcpError::Sync("evidence: 32 bytes expected"))
}
fn t(v: V) -> Result<String> {
    match v {
        V::T(s) => Ok(s),
        _ => err("evidence: tstr expected"),
    }
}
fn a(v: V) -> Result<Vec<V>> {
    match v {
        V::A(a) => Ok(a),
        _ => err("evidence: array expected"),
    }
}
fn done(m: Vec<(u64, V)>) -> Result<()> {
    if m.is_empty() {
        Ok(())
    } else {
        err("evidence: unknown key")
    }
}

impl ReportedMessage {
    fn to_v(&self) -> V {
        let mut m = vec![
            (0, V::B(self.sender_id.to_vec())),
            (1, V::B(self.framed_content.clone())),
            (2, V::B(self.group_context_hash.to_vec())),
            (3, V::B(self.group_context.clone())),
            (4, V::B(self.signature.clone())),
            (5, V::B(self.leaf_node.clone())),
            (6, V::B(self.body.clone())),
        ];
        if !self.media.is_empty() {
            m.push((
                7,
                V::A(
                    self.media
                        .iter()
                        .map(|x| {
                            V::M(vec![
                                (0, V::T(x.mime.clone())),
                                (1, V::B(x.bytes.clone())),
                                (2, V::B(x.hash.to_vec())),
                            ])
                        })
                        .collect(),
                ),
            ));
        }
        V::M(m)
    }
    fn from_v(v: V) -> Result<Self> {
        let mut m = map(v)?;
        let media = match get(&mut m, 7) {
            None => Vec::new(),
            Some(v) => a(v)?
                .into_iter()
                .map(|x| {
                    let mut x = map(x)?;
                    let r = MediaItem {
                        mime: t(need(&mut x, 0)?)?,
                        bytes: b(need(&mut x, 1)?)?,
                        hash: b32(need(&mut x, 2)?)?,
                    };
                    done(x).map(|_| r)
                })
                .collect::<Result<_>>()?,
        };
        let r = Self {
            sender_id: b32(need(&mut m, 0)?)?,
            framed_content: b(need(&mut m, 1)?)?,
            group_context_hash: b32(need(&mut m, 2)?)?,
            group_context: b(need(&mut m, 3)?)?,
            signature: b(need(&mut m, 4)?)?,
            leaf_node: b(need(&mut m, 5)?)?,
            body: b(need(&mut m, 6)?)?,
            media,
        };
        done(m).map(|_| r)
    }
}

impl EvidenceFile {
    fn to_v(&self, with_sig: bool) -> V {
        let mut m = vec![
            (0, V::U(1)),
            (1, V::B(self.group_ref.to_vec())),
            (2, V::U(self.export_ms)),
            (3, V::A(self.messages.iter().map(|x| x.to_v()).collect())),
            (4, V::U(self.reason_code as u64)),
            (8, V::B(self.reporter_key.to_vec())),
        ];
        if let Some(n) = &self.note {
            m.push((5, V::T(n.clone())));
        }
        if let Some(c) = &self.contact {
            let mut cm = Vec::new();
            for (i, f) in [&c.name, &c.email, &c.phone].into_iter().enumerate() {
                if let Some(s) = f {
                    cm.push((i as u64, V::T(s.clone())));
                }
            }
            m.push((6, V::M(cm)));
        }
        if !self.disclosures.is_empty() {
            m.push((
                7,
                V::A(
                    self.disclosures
                        .iter()
                        .map(|d| {
                            let mut x = vec![(0, V::B(d.sender_id.to_vec()))];
                            if let Some(l) = &d.label {
                                x.push((1, V::T(l.clone())));
                            }
                            V::M(x)
                        })
                        .collect(),
                ),
            ));
        }
        if with_sig {
            m.push((9, V::B(self.reporter_sig.to_vec())));
        }
        V::M(m)
    }

    fn cbor(&self, with_sig: bool) -> Vec<u8> {
        let mut o = Vec::new();
        enc(&self.to_v(with_sig), &mut o);
        o
    }

    fn signed_bytes(&self) -> Vec<u8> {
        let mut b = SIG_DOMAIN.to_vec();
        b.extend_from_slice(&self.cbor(false));
        b
    }

    fn check_limits(&self) -> Result<()> {
        if self.messages.is_empty() || self.messages.len() > MAX_MESSAGES {
            return err("evidence: 1..500 messages");
        }
        if self
            .note
            .as_ref()
            .map(|n| n.len() > MAX_NOTE)
            .unwrap_or(false)
        {
            return err("evidence: note too long");
        }
        if !(1..=9).contains(&self.reason_code) {
            return err("evidence: reason category");
        }
        Ok(())
    }

    /// Build, sign and encode an evidence file from the messages the reporter
    /// picked (nothing else is added). `group_id` is the private group's
    /// random 32-byte ID. Deterministic: same input → same bytes (MR20).
    #[allow(clippy::too_many_arguments)]
    pub fn export(
        group_id: &[u8],
        messages: Vec<ReportedMessage>,
        reason_code: u8,
        note: Option<String>,
        contact: Option<ReporterContact>,
        disclosures: Vec<MemberDisclosure>,
        reporter: &SigningKey,
        export_ms: u64,
    ) -> Result<Vec<u8>> {
        let mut f = Self {
            group_ref: evidence_group_ref(group_id),
            export_ms,
            messages,
            reason_code,
            note,
            contact,
            disclosures,
            reporter_key: reporter.verifying_key().to_bytes(),
            reporter_sig: [0u8; 64],
        };
        f.check_limits()?;
        f.reporter_sig = reporter.sign(&f.signed_bytes()).to_bytes();
        let out = f.cbor(true);
        if out.len() > MAX_FILE_BYTES {
            return err("evidence: file over 50 MB");
        }
        Ok(out)
    }

    /// Strict decode: canonical CBOR only (re-encoding must give the input).
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_FILE_BYTES {
            return err("evidence: file over 50 MB");
        }
        let mut d = Dec(bytes);
        let v = d.val(0)?;
        if !d.0.is_empty() {
            return err("evidence: trailing bytes");
        }
        let mut m = map(v)?;
        if u(need(&mut m, 0)?)? != 1 {
            return err("evidence: version");
        }
        let contact = match get(&mut m, 6) {
            None => None,
            Some(v) => {
                let mut c = map(v)?;
                let r = ReporterContact {
                    name: get(&mut c, 0).map(t).transpose()?,
                    email: get(&mut c, 1).map(t).transpose()?,
                    phone: get(&mut c, 2).map(t).transpose()?,
                };
                done(c)?;
                Some(r)
            }
        };
        let disclosures = match get(&mut m, 7) {
            None => Vec::new(),
            Some(v) => a(v)?
                .into_iter()
                .map(|x| {
                    let mut x = map(x)?;
                    let r = MemberDisclosure {
                        sender_id: b32(need(&mut x, 0)?)?,
                        label: get(&mut x, 1).map(t).transpose()?,
                    };
                    done(x).map(|_| r)
                })
                .collect::<Result<_>>()?,
        };
        let f = Self {
            group_ref: b32(need(&mut m, 1)?)?,
            export_ms: u(need(&mut m, 2)?)?,
            messages: a(need(&mut m, 3)?)?
                .into_iter()
                .map(ReportedMessage::from_v)
                .collect::<Result<_>>()?,
            reason_code: u8::try_from(u(need(&mut m, 4)?)?)
                .map_err(|_| OmcpError::Sync("evidence: reason"))?,
            note: get(&mut m, 5).map(t).transpose()?,
            contact,
            disclosures,
            reporter_key: b32(need(&mut m, 8)?)?,
            reporter_sig: b(need(&mut m, 9)?)?
                .try_into()
                .map_err(|_| OmcpError::Sync("evidence: sig size"))?,
        };
        done(m)?;
        if f.cbor(true) != bytes {
            return err("evidence: not deterministic CBOR");
        }
        f.check_limits()?;
        Ok(f)
    }

    /// `evidence_hash = BLAKE2b-256(whole file bytes)` (§23.10.3(3)).
    pub fn hash(bytes: &[u8]) -> [u8; 32] {
        h256(&[bytes])
    }

    /// Check `reporter_sig`.
    pub fn reporter_sig_ok(&self) -> bool {
        VerifyingKey::from_bytes(&self.reporter_key)
            .and_then(|k| {
                k.verify_strict(
                    &self.signed_bytes(),
                    &Signature::from_bytes(&self.reporter_sig),
                )
            })
            .is_ok()
    }
}

/// Result of verifying a whole evidence file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub evidence_hash: [u8; 32],
    pub reporter_sig_ok: bool,
    /// Per message: verified genuine (all four steps of §23.10.1(4)).
    pub messages: Vec<bool>,
}

impl Verification {
    pub fn verified(&self) -> usize {
        self.messages.iter().filter(|x| **x).count()
    }
}

/// Decode and verify an evidence file (team side, §23.10.3(6)).
pub fn verify(bytes: &[u8]) -> Result<Verification> {
    let f = EvidenceFile::decode(bytes)?;
    Ok(Verification {
        evidence_hash: EvidenceFile::hash(bytes),
        reporter_sig_ok: f.reporter_sig_ok(),
        messages: f
            .messages
            .iter()
            .map(|m| verify_message(&f.group_ref, m))
            .collect(),
    })
}

// ------------------------------------------------------ MLS TLS parsing ----

struct Tls<'a>(&'a [u8]);
impl<'a> Tls<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }
    /// RFC 9420 §2.1.2 variable-length vector.
    fn vec(&mut self) -> Option<&'a [u8]> {
        let f = self.u8()?;
        let len = match f >> 6 {
            0 => (f & 0x3f) as usize,
            1 => ((f & 0x3f) as usize) << 8 | self.u8()? as usize,
            2 => {
                let r = self.take(3)?;
                ((f & 0x3f) as usize) << 24
                    | (r[0] as usize) << 16
                    | (r[1] as usize) << 8
                    | r[2] as usize
            }
            _ => return None,
        };
        self.take(len)
    }
}

/// Encode an RFC 9420 variable-length vector.
pub fn tls_vec(out: &mut Vec<u8>, b: &[u8]) {
    let n = b.len();
    if n < 64 {
        out.push(n as u8);
    } else if n < 16384 {
        out.extend_from_slice(&((n as u16) | 0x4000).to_be_bytes());
    } else {
        out.extend_from_slice(&((n as u32) | 0x8000_0000).to_be_bytes());
    }
    out.extend_from_slice(b);
}

struct Framed<'a> {
    group_id: &'a [u8],
    epoch: u64,
    application_data: &'a [u8],
}

fn parse_framed(b: &[u8]) -> Option<Framed<'_>> {
    let mut r = Tls(b);
    let group_id = r.vec()?;
    let epoch = r.u64()?;
    if r.u8()? != 1 {
        return None; // sender_type member
    }
    r.u32()?; // leaf index
    r.vec()?; // authenticated_data
    if r.u8()? != 1 {
        return None; // content_type application
    }
    let application_data = r.vec()?;
    r.0.is_empty().then_some(Framed {
        group_id,
        epoch,
        application_data,
    })
}

fn parse_context(b: &[u8]) -> Option<(&[u8], u64)> {
    let mut r = Tls(b);
    if r.u16()? != MLS10 {
        return None;
    }
    r.u16()?; // cipher suite
    let gid = r.vec()?;
    let epoch = r.u64()?;
    Some((gid, epoch))
}

/// (signature_key, credential identity) of a basic-credential LeafNode.
fn parse_leaf(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut r = Tls(b);
    r.vec()?; // encryption_key
    let sig_key = r.vec()?;
    if r.u16()? != 1 {
        return None; // basic credential
    }
    Some((sig_key, r.vec()?))
}

/// RFC 9420 `SignWithLabel` content for `FramedContentTBS`.
pub fn framed_content_sign_content(framed_content: &[u8], group_context: &[u8]) -> Vec<u8> {
    let mut tbs = Vec::new();
    tbs.extend_from_slice(&MLS10.to_be_bytes());
    tbs.extend_from_slice(&WIRE_FORMAT_PRIVATE_MESSAGE.to_be_bytes());
    tbs.extend_from_slice(framed_content);
    tbs.extend_from_slice(group_context);
    let mut out = Vec::new();
    tls_vec(&mut out, b"MLS 1.0 FramedContentTBS");
    tls_vec(&mut out, &tbs);
    out
}

/// §23.10.1(4) steps a–d for one message.
pub fn verify_message(file_group_ref: &[u8; 32], m: &ReportedMessage) -> bool {
    let (Some(fc), Some((gid, epoch)), Some((sig_key, identity))) = (
        parse_framed(&m.framed_content),
        parse_context(&m.group_context),
        parse_leaf(&m.leaf_node),
    ) else {
        return false;
    };
    // (a) sender signature over FramedContentTBS.
    let Ok(pk) = <[u8; 32]>::try_from(sig_key) else {
        return false;
    };
    let Ok(sig) = <[u8; 64]>::try_from(m.signature.as_slice()) else {
        return false;
    };
    let msg = framed_content_sign_content(&m.framed_content, &m.group_context);
    let sig_ok = VerifyingKey::from_bytes(&pk)
        .and_then(|k| k.verify_strict(&msg, &Signature::from_bytes(&sig)))
        .is_ok();
    // (b) the credential binds the leaf to the sender (OMCP credential
    // identity = PeerID hash = BLAKE2b-256 of the identity key).
    let sender_ok = identity == m.sender_id;
    // (c) ids, epochs, context hash and group_ref agree.
    let ctx_ok = fc.group_id == gid
        && fc.epoch == epoch
        && h256(&[&m.group_context]) == m.group_context_hash
        && evidence_group_ref(gid) == *file_group_ref;
    // (d) body equals the signed application_data.
    let body_ok = fc.application_data == m.body.as_slice();
    sig_ok && sender_ok && ctx_ok && body_ok
}

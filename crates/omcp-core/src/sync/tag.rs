//! Rotating mailbox tags (PROTOCOL §22.1).
//!
//! `tag = BLAKE2b-64(key = exporter_secret, "OMCP-tag" || epoch_be)` with a 6 h
//! epoch. Only chat members (who hold the MLS / session exporter secret) can
//! compute it; the secret is `MLS-Exporter("OMCP-tag", epoch, 32)` for every
//! chat, 1:1 chats being 2-member MLS groups (§22.7(5)). Tags ride in
//! `EnvelopeV1::header_aad` as TLV `0x54` (len 8) per the §21.8 registry.

use blake2::digest::consts::U8;
use blake2::digest::Mac;
use blake2::Blake2bMac;

use crate::error::{OmcpError, Result};

/// 8-byte routing tag.
pub type MailboxTag = [u8; 8];

/// Tag rotation period: 6 h.
pub const TAG_EPOCH_MS: u64 = 6 * 60 * 60 * 1000;

/// `header_aad` TLV type for the routing tag (ASCII 'T').
pub const AAD_TLV_TAG: u8 = 0x54;
/// `header_aad` TLV type for recipient sub-tags (8·n bytes, §22.7(9)).
pub const AAD_TLV_SUBTAGS: u8 = 0x56;
/// Chats with at most this many members route on per-recipient sub-tags;
/// larger groups use the chat tag + Plumtree.
pub const SUBTAG_MAX_MEMBERS: usize = 8;

const LABEL: &[u8] = b"OMCP-tag";
const SUB_LABEL: &[u8] = b"OMCP-sub";
/// MLS exporter label for tag secrets (§22.7(5)).
pub const TAG_EXPORTER_LABEL: &str = "OMCP-tag";

/// Tag epoch for wall-clock `now_ms`.
pub fn tag_epoch(now_ms: u64) -> u64 {
    now_ms / TAG_EPOCH_MS
}

/// Derive the tag for `exporter_secret` (16..=64 bytes) in `epoch`.
pub fn mailbox_tag(exporter_secret: &[u8], epoch: u64) -> Result<MailboxTag> {
    if exporter_secret.len() < 16 {
        return Err(OmcpError::Sync("exporter secret too short"));
    }
    let mut mac = <Blake2bMac<U8> as Mac>::new_from_slice(exporter_secret)
        .map_err(|_| OmcpError::Sync("exporter secret too long"))?;
    mac.update(LABEL);
    mac.update(&epoch.to_be_bytes());
    let out = mac.finalize().into_bytes();
    let mut t = [0u8; 8];
    t.copy_from_slice(&out);
    Ok(t)
}

/// Per-recipient sub-tag (§22.7(9)):
/// `BLAKE2b-64(key = exporter_secret, "OMCP-sub" || leaf_index_u32_be || epoch_u64_be)`.
pub fn subtag(exporter_secret: &[u8], leaf_index: u32, epoch: u64) -> Result<MailboxTag> {
    if exporter_secret.len() < 16 {
        return Err(OmcpError::Sync("exporter secret too short"));
    }
    let mut mac = <Blake2bMac<U8> as Mac>::new_from_slice(exporter_secret)
        .map_err(|_| OmcpError::Sync("exporter secret too long"))?;
    mac.update(SUB_LABEL);
    mac.update(&leaf_index.to_be_bytes());
    mac.update(&epoch.to_be_bytes());
    let mut t = [0u8; 8];
    t.copy_from_slice(&mac.finalize().into_bytes());
    Ok(t)
}

/// Does a chat of `members` route on sub-tags (else chat tag + Plumtree)?
pub fn uses_subtags(members: usize) -> bool {
    members <= SUBTAG_MAX_MEMBERS
}

/// Tags a receiver should accept at `now_ms`: current and previous epoch
/// (clock skew / in-flight envelopes).
pub fn accepted_tags(exporter_secret: &[u8], now_ms: u64) -> Result<[MailboxTag; 2]> {
    let e = tag_epoch(now_ms);
    Ok([
        mailbox_tag(exporter_secret, e)?,
        mailbox_tag(exporter_secret, e.saturating_sub(1))?,
    ])
}

/// `header_aad` TLV registry class (PROTOCOL §21.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlvClass {
    /// `0x01`–`0x3F`: reserved for core.
    Core,
    /// `0x54`: mailbox tag (len 8).
    MailboxTag,
    /// `0x56`: recipient sub-tags (8·n).
    RecipientSubtags,
    /// `0x58`: `outer_expiry` (len 4, u32 unix seconds, §22.8(2)).
    OuterExpiry,
    /// `0x55`, `0x57`, `0x59`–`0x7F`: unassigned (extension process §16).
    Unassigned,
    /// `0x80`–`0xFF`: experimental / private use.
    Experimental,
    /// `0x00`, `0x40`–`0x53`: not in the registry.
    Invalid,
}

pub fn tlv_class(t: u8) -> TlvClass {
    match t {
        0x01..=0x3F => TlvClass::Core,
        AAD_TLV_TAG => TlvClass::MailboxTag,
        AAD_TLV_SUBTAGS => TlvClass::RecipientSubtags,
        0x58 => TlvClass::OuterExpiry,
        0x55 | 0x57 | 0x59..=0x7F => TlvClass::Unassigned,
        0x80..=0xFF => TlvClass::Experimental,
        _ => TlvClass::Invalid,
    }
}

/// Append the tag TLV to a `header_aad` buffer.
pub fn put_tag_in_aad(aad: &mut Vec<u8>, tag: &MailboxTag) {
    aad.push(AAD_TLV_TAG);
    aad.push(8);
    aad.extend_from_slice(tag);
}

/// Append recipient sub-tags (1..=8) as TLV `0x56`.
pub fn put_subtags_in_aad(aad: &mut Vec<u8>, subtags: &[MailboxTag]) -> Result<()> {
    if subtags.is_empty() || subtags.len() > SUBTAG_MAX_MEMBERS {
        return Err(OmcpError::Sync("subtag count"));
    }
    aad.push(AAD_TLV_SUBTAGS);
    aad.push((8 * subtags.len()) as u8);
    for t in subtags {
        aad.extend_from_slice(t);
    }
    Ok(())
}

/// Routing fields found in `header_aad`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutingAad {
    pub tag: Option<MailboxTag>,
    pub subtags: Vec<MailboxTag>,
}

impl RoutingAad {
    /// Tags to route on: sub-tags if present, else the chat tag.
    pub fn route_tags(&self) -> Vec<MailboxTag> {
        if self.subtags.is_empty() {
            self.tag.into_iter().collect()
        } else {
            self.subtags.clone()
        }
    }
}

/// Parse a `header_aad` of `type u8 | len u8 | value` items per the §21.8
/// registry. Rejects truncated items, types outside the registry, a `0x54`
/// whose length is not 8, a `0x56` whose length is not a non-zero multiple of
/// 8 up to 64, and duplicates of either. Core, unassigned and experimental
/// types are skipped (forward compatibility).
pub fn parse_routing_aad(aad: &[u8]) -> Result<RoutingAad> {
    let mut i = 0;
    let mut out = RoutingAad::default();
    let mut seen_sub = false;
    while i < aad.len() {
        if i + 2 > aad.len() {
            return Err(OmcpError::Sync("aad tlv truncated"));
        }
        let (t, l) = (aad[i], aad[i + 1] as usize);
        let v = aad
            .get(i + 2..i + 2 + l)
            .ok_or(OmcpError::Sync("aad tlv truncated"))?;
        match tlv_class(t) {
            TlvClass::Invalid => return Err(OmcpError::Sync("aad tlv type not in registry")),
            TlvClass::MailboxTag => {
                if l != 8 {
                    return Err(OmcpError::Sync("tag tlv length"));
                }
                if out.tag.is_some() {
                    return Err(OmcpError::Sync("duplicate tag tlv"));
                }
                out.tag = Some(v.try_into().expect("8"));
            }
            TlvClass::RecipientSubtags => {
                if l == 0 || l % 8 != 0 || l > 8 * SUBTAG_MAX_MEMBERS {
                    return Err(OmcpError::Sync("subtag tlv length"));
                }
                if seen_sub {
                    return Err(OmcpError::Sync("duplicate subtag tlv"));
                }
                seen_sub = true;
                out.subtags = v.chunks(8).map(|c| c.try_into().expect("8")).collect();
            }
            TlvClass::OuterExpiry if l != 4 => {
                return Err(OmcpError::Sync("outer_expiry tlv length"))
            }
            _ => {}
        }
        i += 2 + l;
    }
    Ok(out)
}

/// The chat tag in `header_aad`, if any (see [`parse_routing_aad`]).
pub fn tag_from_aad(aad: &[u8]) -> Result<Option<MailboxTag>> {
    Ok(parse_routing_aad(aad)?.tag)
}

/// 32-byte gateway rendezvous label (§22.7(3)):
/// `HKDF-SHA-384(ikm = tag_secret, info = "OMCP-rdv" || epoch_be, L = 32)`.
/// The existing `OpaqueEnvelope` / rendezvous server carry it unchanged.
pub fn rendezvous_label(tag_secret: &[u8], epoch: u64) -> Result<[u8; 32]> {
    let hk = hkdf::Hkdf::<sha2::Sha384>::new(None, tag_secret);
    let mut info = Vec::with_capacity(16);
    info.extend_from_slice(b"OMCP-rdv");
    info.extend_from_slice(&epoch.to_be_bytes());
    let mut out = [0u8; 32];
    hk.expand(&info, &mut out)
        .map_err(|_| OmcpError::Sync("hkdf expand"))?;
    Ok(out)
}

/// Maximum carry distance encoded in digests (= max adaptive TTL).
pub const MAX_CARRY_DISTANCE: u8 = 12;

/// Digest entry advertising "I can reach wanters of `tag` in `d` hops"
/// (§22.7(1)). Only nodes that saw `tag` can compute or test it.
pub fn carry_tag(tag: &MailboxTag, d: u8) -> MailboxTag {
    let mut m = [0u8; 9];
    m[..8].copy_from_slice(tag);
    m[8] = d;
    let h = blake3::derive_key("OMCP-carry-v1", &m);
    let mut t = [0u8; 8];
    t.copy_from_slice(&h[..8]);
    t
}

/// Source of per-epoch tag secrets (§22.7(5)): every chat — 1:1 included —
/// is an MLS group and the secret is `MLS-Exporter("OMCP-tag", epoch, 32)`.
/// Implemented by [`crate::mls::OmcpMlsMember`].
pub trait TagSecretSource {
    fn tag_secret(&self, tag_epoch: u64) -> Result<[u8; 32]>;
}

/// Sub-tag of member `leaf_index` at `tag_epoch` from the chat's secret source.
pub fn chat_subtag<S: TagSecretSource + ?Sized>(
    src: &S,
    leaf_index: u32,
    tag_epoch: u64,
) -> Result<MailboxTag> {
    subtag(&src.tag_secret(tag_epoch)?, leaf_index, tag_epoch)
}

/// Tag for a chat at `tag_epoch` from its secret source.
pub fn chat_tag<S: TagSecretSource + ?Sized>(src: &S, tag_epoch: u64) -> Result<MailboxTag> {
    mailbox_tag(&src.tag_secret(tag_epoch)?, tag_epoch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_rotates_and_is_keyed() {
        let s = [7u8; 32];
        let t = [8u8; 32];
        assert_eq!(mailbox_tag(&s, 1).unwrap(), mailbox_tag(&s, 1).unwrap());
        assert_ne!(mailbox_tag(&s, 1).unwrap(), mailbox_tag(&s, 2).unwrap());
        assert_ne!(mailbox_tag(&s, 1).unwrap(), mailbox_tag(&t, 1).unwrap());
        assert!(mailbox_tag(&[1u8; 8], 1).is_err());
        assert!(mailbox_tag(&[1u8; 65], 1).is_err());
        assert_eq!(tag_epoch(TAG_EPOCH_MS * 3 + 5), 3);
        let acc = accepted_tags(&s, TAG_EPOCH_MS * 3).unwrap();
        assert_eq!(acc[1], mailbox_tag(&s, 2).unwrap());
    }

    #[test]
    fn aad_roundtrip() {
        let mut aad = vec![0x01, 2, 9, 9];
        put_tag_in_aad(&mut aad, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(tag_from_aad(&aad).unwrap(), Some([1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(tag_from_aad(&[1, 1, 0]).unwrap(), None);
        assert!(tag_from_aad(&[0x54, 3, 1, 2, 3]).is_err());
        assert!(tag_from_aad(&[0x54]).is_err());
    }

    #[test]
    fn rendezvous_label_is_hkdf_sha384() {
        let a = rendezvous_label(&[1; 32], 1).unwrap();
        assert_ne!(a, rendezvous_label(&[1; 32], 2).unwrap());
        assert_ne!(a, rendezvous_label(&[2; 32], 1).unwrap());
        let hk = hkdf::Hkdf::<sha2::Sha384>::new(None, &[1; 32]);
        let mut want = [0u8; 32];
        hk.expand(b"OMCP-rdv\0\0\0\0\0\0\0\x01", &mut want).unwrap();
        assert_eq!(a, want);
    }

    #[test]
    fn tlv_registry() {
        assert_eq!(tlv_class(0x01), TlvClass::Core);
        assert_eq!(tlv_class(0x54), TlvClass::MailboxTag);
        assert_eq!(tlv_class(0x60), TlvClass::Unassigned);
        assert_eq!(tlv_class(0x55), TlvClass::Unassigned);
        assert_eq!(tlv_class(0x56), TlvClass::RecipientSubtags);
        assert_eq!(tlv_class(0x58), TlvClass::OuterExpiry);
        assert_eq!(tlv_class(0x57), TlvClass::Unassigned);
        assert!(parse_routing_aad(&[0x58, 8, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(parse_routing_aad(&[0x58, 4, 0, 0, 0, 1]).is_ok());
        assert_eq!(tlv_class(0x90), TlvClass::Experimental);
        assert_eq!(tlv_class(0x00), TlvClass::Invalid);
        assert_eq!(tlv_class(0x40), TlvClass::Invalid);
        // unassigned / experimental skipped, invalid rejected, dup rejected
        let mut aad = vec![0x60, 1, 0, 0x90, 0];
        put_tag_in_aad(&mut aad, &[4; 8]);
        assert_eq!(tag_from_aad(&aad).unwrap(), Some([4; 8]));
        put_tag_in_aad(&mut aad, &[4; 8]);
        assert!(tag_from_aad(&aad).is_err());
        assert!(tag_from_aad(&[0x45, 0]).is_err());
    }

    #[test]
    fn subtags_and_tlv_0x56() {
        let s = [3u8; 32];
        let a = subtag(&s, 0, 9).unwrap();
        assert_ne!(a, subtag(&s, 1, 9).unwrap());
        assert_ne!(a, subtag(&s, 0, 10).unwrap());
        assert_ne!(a, mailbox_tag(&s, 9).unwrap());
        assert!(uses_subtags(8) && !uses_subtags(9));
        let mut aad = vec![];
        put_tag_in_aad(&mut aad, &[1; 8]);
        put_subtags_in_aad(&mut aad, &[a, [2; 8]]).unwrap();
        let r = parse_routing_aad(&aad).unwrap();
        assert_eq!(r.tag, Some([1; 8]));
        assert_eq!(r.route_tags(), vec![a, [2; 8]]);
        assert!(put_subtags_in_aad(&mut vec![], &[[0; 8]; 9]).is_err());
        assert!(parse_routing_aad(&[0x56, 7, 0, 0, 0, 0, 0, 0, 0]).is_err());
        let mut dup = aad.clone();
        put_subtags_in_aad(&mut dup, &[a]).unwrap();
        assert!(parse_routing_aad(&dup).is_err());
        let only_tag = parse_routing_aad(&aad[..10]).unwrap();
        assert_eq!(only_tag.route_tags(), vec![[1; 8]]);
    }

    #[test]
    fn carry_tags_distinct_per_distance() {
        let t = [9u8; 8];
        assert_ne!(carry_tag(&t, 1), carry_tag(&t, 2));
        assert_ne!(carry_tag(&t, 1), t);
    }
}

//! Private-group admin list (§23.10.13): MLS GroupContext extension `0xF0A1`.

use openmls::extensions::{Extension, ExtensionType, UnknownExtension};
use openmls::prelude::Capabilities;

use crate::error::{OmcpError, Result};
use crate::moderation::peer_id_hash_bytes;

/// MLS private-use extension type for `omcp_group_admins`.
pub const OMCP_GROUP_ADMINS: u16 = 0xF0A1;
pub const MAX_ADMINS: usize = 16;

/// Encode the extension body: `count u8 (1–16) || member_id [32] × count`.
pub fn encode_admins(member_ids: &[[u8; 32]]) -> Result<Vec<u8>> {
    if !(1..=MAX_ADMINS).contains(&member_ids.len()) {
        return Err(OmcpError::Mls("admin list size"));
    }
    let mut b = Vec::with_capacity(1 + 32 * member_ids.len());
    b.push(member_ids.len() as u8);
    for id in member_ids {
        b.extend_from_slice(id);
    }
    Ok(b)
}

/// Decode the extension body. Empty / malformed → None (use leaf-0 fallback).
pub fn decode_admins(body: &[u8]) -> Option<Vec<[u8; 32]>> {
    if body.is_empty() {
        return None;
    }
    let n = body[0] as usize;
    if !(1..=MAX_ADMINS).contains(&n) || body.len() != 1 + 32 * n {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = 1 + 32 * i;
        out.push(body[s..s + 32].try_into().ok()?);
    }
    Some(out)
}

pub fn admins_extension(member_ids: &[[u8; 32]]) -> Result<Extension> {
    Ok(Extension::Unknown(
        OMCP_GROUP_ADMINS,
        UnknownExtension(encode_admins(member_ids)?),
    ))
}

/// Leaf capabilities that advertise support for `0xF0A1`.
pub fn admin_capable() -> Capabilities {
    Capabilities::builder()
        .extensions(vec![ExtensionType::Unknown(OMCP_GROUP_ADMINS)])
        .build()
}

/// `member_id` of an identity credential's bytes.
pub fn member_id_of(cred: &[u8]) -> [u8; 32] {
    peer_id_hash_bytes(cred)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_one_and_many() {
        let one = [[7u8; 32]];
        let b = encode_admins(&one).unwrap();
        assert_eq!(decode_admins(&b).unwrap(), one);
        let many: Vec<[u8; 32]> = (0..16).map(|i| [i; 32]).collect();
        let b = encode_admins(&many).unwrap();
        assert_eq!(decode_admins(&b).unwrap(), many);
        assert!(encode_admins(&[]).is_err());
        assert!(encode_admins(&[[0; 32]; 17]).is_err());
        assert!(decode_admins(&[0]).is_none());
        assert!(decode_admins(&[2, 1]).is_none());
    }
}

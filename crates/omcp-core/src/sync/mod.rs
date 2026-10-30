//! Scalable sync and data efficiency (PROTOCOL §22) — **experimental (v0.2)**.
//!
//! Sub-modules map one-to-one onto the spec:
//! - [`tag`]: rotating 8-byte mailbox tags (§22.1) and their `header_aad` TLV.
//! - [`cuckoo`]: padded cuckoo-filter interest digests and deltas (§22.1).
//! - [`hello`]: hello back-off and delta/full digest cadence (§22.7(2)).
//! - [`routing`]: forwarding rule (interest match / spray-and-wait), HyperLogLog
//!   mesh-size estimate, adaptive TTL, per-neighbour token bucket (§22.1, §22.4).
//! - [`recon`]: Negentropy-style range-based set reconciliation (§22.2).
//! - [`watermark`]: signed per-member Lamport high-water marks (§22.2).
//! - [`media`]: lazy content-addressed media refs + resumable chunk fetch (§22.3).
//! - [`compress`]: padding buckets and the compression MUST rules (§22.3).
//! - [`receipts`]: delta ops and 5 s receipt batching (§22.3).
//! - [`plumtree`]: eager/lazy tree push for communities > 256 members (§22.3).
//! - [`budget`]: relay budgets, eviction order, expiry, metered rules and
//!   gateway rendezvous offload (§22.4, §22.5).

pub mod budget;
pub mod compress;
pub mod cuckoo;
pub mod hello;
pub mod media;
pub mod plumtree;
pub mod receipts;
pub mod recon;
pub mod relay;
pub mod routing;
pub mod tag;
pub mod watermark;

/// Tiny deterministic PRNG (SplitMix64) used where the spec needs randomness
/// that callers may want to seed (decoys, spray targets, simulations).
#[derive(Debug, Clone)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    /// Next 64-bit output.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (n > 0).
    pub fn below(&mut self, n: u64) -> u64 {
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// Bernoulli(p).
    pub fn chance(&mut self, p: f64) -> bool {
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) < p
    }

    /// Seed from OS randomness.
    pub fn from_os() -> crate::Result<Self> {
        let mut b = [0u8; 8];
        getrandom::getrandom(&mut b).map_err(|_| crate::OmcpError::Crypto("getrandom"))?;
        Ok(Self(u64::from_le_bytes(b)))
    }
}

/// Unsigned LEB128 varint append.
pub(crate) fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// Unsigned LEB128 varint read; advances `buf`.
pub(crate) fn get_varint(buf: &mut &[u8]) -> crate::Result<u64> {
    let mut v = 0u64;
    for i in 0..10 {
        let (&b, rest) = buf
            .split_first()
            .ok_or(crate::OmcpError::Sync("varint truncated"))?;
        *buf = rest;
        v |= ((b & 0x7f) as u64) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(crate::OmcpError::Sync("varint too long"))
}

pub(crate) fn take<'a>(buf: &mut &'a [u8], n: usize) -> crate::Result<&'a [u8]> {
    if buf.len() < n {
        return Err(crate::OmcpError::Truncated {
            need: n,
            have: buf.len(),
        });
    }
    let (a, b) = buf.split_at(n);
    *buf = b;
    Ok(a)
}

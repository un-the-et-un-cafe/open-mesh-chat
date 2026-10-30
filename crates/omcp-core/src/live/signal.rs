//! WebRTC signalling (PROTOCOL §20.3: content type `0x50
//! application/omcp-rtc-signal`, E2EE). Offers, answers and ICE candidates
//! are sealed with the stream key and carried as OMCP messages, so no
//! signalling server exists and non-listeners learn nothing.

use serde::{Deserialize, Serialize};

use super::{err, StreamKey};
use crate::error::{OmcpError, Result};

pub const CONTENT_TYPE_RTC_SIGNAL: u8 = 0x50;
const MAX_BODY: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalKind {
    Offer,
    Answer,
    Ice,
    /// Tear down (re-parenting or leave).
    Bye,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RtcSignal {
    pub stream_id: [u8; 16],
    pub from: [u8; 32],
    pub to: [u8; 32],
    pub kind: SignalKind,
    /// SDP or ICE candidate line.
    pub body: String,
}

impl RtcSignal {
    pub fn seal(&self, key: &StreamKey) -> Result<Vec<u8>> {
        if self.body.len() > MAX_BODY || self.stream_id != key.stream_id {
            return err("rtc signal");
        }
        let mut plain = Vec::new();
        ciborium::into_writer(self, &mut plain).map_err(|_| OmcpError::Sync("rtc cbor"))?;
        let mut out = vec![CONTENT_TYPE_RTC_SIGNAL];
        out.extend_from_slice(&key.key_epoch.to_be_bytes());
        let aad = out.clone();
        out.extend_from_slice(&key.seal(&plain, &aad)?);
        Ok(out)
    }

    pub fn open(key: &StreamKey, b: &[u8]) -> Result<Self> {
        if b.len() < 5 || b[0] != CONTENT_TYPE_RTC_SIGNAL {
            return err("not an rtc signal");
        }
        if u32::from_be_bytes(b[1..5].try_into().expect("4")) != key.key_epoch {
            return err("rtc signal key epoch");
        }
        let plain = key.open(&b[5..], &b[..5])?;
        let s: Self =
            ciborium::from_reader(plain.as_slice()).map_err(|_| OmcpError::Sync("rtc cbor"))?;
        if s.stream_id != key.stream_id || s.body.len() > MAX_BODY {
            return err("rtc signal stream");
        }
        Ok(s)
    }
}

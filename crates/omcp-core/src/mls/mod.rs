//! MLS groups for OMCP (PROTOCOL §5.1, §6.2) plus OMCP-MLS-PQ helper.
//!
//! Built on [OpenMLS](https://github.com/openmls/openmls) 0.9 with ciphersuite
//! `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`. Not production-ready.

pub mod group;
pub mod mls_pq;

pub use group::{OmcpMlsMember, SignedAppMessage, MAX_SMALL_GROUP_MEMBERS, OMCP_MLS_CIPHERSUITE};
pub use mls_pq::{
    encapsulate_to_members, mix_exporter, EncapsEntry, MlsPqKemKeyPair, OmcpMlsPqExtensionV1,
    MLS_PQ_EXPORTER_LABEL, MLS_PQ_EXTENSION_TYPE, MLS_PQ_HKDF_INFO, MLS_PQ_MAGIC, MLS_PQ_VERSION,
};

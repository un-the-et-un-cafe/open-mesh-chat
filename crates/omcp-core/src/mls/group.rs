//! Thin OpenMLS facade for small OMCP groups (≤32) — PROTOCOL §5.1 / §6.2.
//!
//! Ciphersuite: `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`.
//! Application messages are encrypted under MLS epoch secrets via OpenMLS.
//! PQ hybrid mix is layered via [`super::mls_pq`] (provisional; see extensions/mls-pq.md).

use openmls::prelude::tls_codec::{Deserialize, Serialize as _};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;

use crate::error::{OmcpError, Result};
use crate::mls::mls_pq::{
    encapsulate_to_members, mix_exporter, MlsPqKemKeyPair, MLS_PQ_EXPORTER_LABEL,
};

/// RFC 9420 suite required by PROTOCOL §5.1.
pub const OMCP_MLS_CIPHERSUITE: Ciphersuite =
    Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// Soft cap for M3 small groups (PROTOCOL milestone text).
pub const MAX_SMALL_GROUP_MEMBERS: usize = 32;

/// Store-and-forward reorders heavily: keep this many skipped message keys
/// per sender (§22.8(1), mesh delivery is out of order).
pub const MESH_OUT_OF_ORDER_TOLERANCE: u32 = 1_000;
/// How far ahead of the last seen generation a message may be.
pub const MESH_MAX_FORWARD_DISTANCE: u32 = 10_000;
/// Past epochs whose message keys are kept (commit races).
pub const MESH_MAX_PAST_EPOCHS: usize = 4;
/// Application-message padding (relays only see bucketed sizes, §22.3).
pub const MESH_PADDING: usize = 64;

/// `(plaintext, aad, sender leaf, signed bytes)`.
pub type DecryptedApp = (Vec<u8>, Vec<u8>, u32, Option<SignedAppMessage>);

/// The bytes a receiver keeps with each private-group application message so
/// the sender's MLS signature can be re-checked later (PROTOCOL §23.10.1(2)).
/// Deleted with the message and on takedown.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SignedAppMessage {
    pub framed_content: Vec<u8>,
    pub wire_format: u16,
    pub group_context: Vec<u8>,
    pub signature: Vec<u8>,
    pub sender_leaf_node: Vec<u8>,
}

/// Serializable member state (everything needed to resume after a restart).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct MlsMemberState {
    pub identity: Vec<u8>,
    pub signer_public: Vec<u8>,
    pub group_id: Option<Vec<u8>>,
    /// Public half of the pending key package (joiner before the Welcome).
    pub key_package: Option<Vec<u8>>,
    pub storage: Vec<(Vec<u8>, Vec<u8>)>,
}

fn ratchet() -> SenderRatchetConfiguration {
    SenderRatchetConfiguration::new(MESH_OUT_OF_ORDER_TOLERANCE, MESH_MAX_FORWARD_DISTANCE)
}

/// One peer’s MLS + OMCP-MLS-PQ state.
/// `(extension_bytes, kem_shared_secrets, hybrid_exporter)` for one epoch.
pub type PqEpochSecrets = (Vec<u8>, Vec<[u8; 32]>, [u8; 32]);

pub struct OmcpMlsMember {
    pub identity: Vec<u8>,
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    credential_with_key: CredentialWithKey,
    /// Key package for others to add this member (refreshed after consume).
    key_package: Option<KeyPackageBundle>,
    /// Serialized public key package (survives a restart; the private part
    /// lives in the OpenMLS key store).
    kp_bytes: Option<Vec<u8>>,
    group: Option<MlsGroup>,
    /// ML-KEM key for OMCP-MLS-PQ (out-of-band with credential).
    pub pq_kem: MlsPqKemKeyPair,
    join_config: MlsGroupJoinConfig,
}

impl OmcpMlsMember {
    /// Create a member with a fresh credential, signature keys, key package, and PQ KEM.
    pub fn generate(identity: impl Into<Vec<u8>>) -> Result<Self> {
        let identity = identity.into();
        let provider = OpenMlsRustCrypto::default();
        let credential = BasicCredential::new(identity.clone());
        let signer = SignatureKeyPair::new(OMCP_MLS_CIPHERSUITE.signature_algorithm())
            .map_err(|_| OmcpError::Mls("signature keygen"))?;
        signer
            .store(provider.storage())
            .map_err(|_| OmcpError::Mls("store signer"))?;
        let credential_with_key = CredentialWithKey {
            credential: credential.into(),
            signature_key: signer.to_public_vec().into(),
        };
        let key_package = KeyPackage::builder()
            .build(
                OMCP_MLS_CIPHERSUITE,
                &provider,
                &signer,
                credential_with_key.clone(),
            )
            .map_err(|_| OmcpError::Mls("key package"))?;
        let join_config = Self::join_config();
        Ok(Self {
            identity,
            provider,
            signer,
            credential_with_key,
            key_package: Some(key_package),
            kp_bytes: None,
            group: None,
            pq_kem: MlsPqKemKeyPair::generate()?,
            join_config,
        })
    }

    fn join_config() -> MlsGroupJoinConfig {
        MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .sender_ratchet_configuration(ratchet())
            .max_past_epochs(MESH_MAX_PAST_EPOCHS)
            .padding_size(MESH_PADDING)
            .build()
    }

    /// Snapshot for persistence (the OpenMLS key store, group id, signer).
    pub fn export_state(&self) -> Result<MlsMemberState> {
        let values = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| OmcpError::Mls("storage lock"))?;
        let mut storage: Vec<(Vec<u8>, Vec<u8>)> =
            values.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        storage.sort();
        let key_package = match &self.key_package {
            Some(_) => Some(self.key_package_bytes()?),
            None => self.kp_bytes.clone(),
        };
        Ok(MlsMemberState {
            identity: self.identity.clone(),
            signer_public: self.signer.to_public_vec(),
            group_id: self
                .group
                .as_ref()
                .map(|g| g.group_id().as_slice().to_vec()),
            key_package,
            storage,
        })
    }

    /// Resume from [`Self::export_state`]. The OMCP-MLS-PQ KEM is regenerated
    /// (it is not used by 1:1 mesh chats).
    pub fn restore(st: &MlsMemberState) -> Result<Self> {
        let provider = OpenMlsRustCrypto::default();
        {
            let mut values = provider
                .storage()
                .values
                .write()
                .map_err(|_| OmcpError::Mls("storage lock"))?;
            for (k, v) in &st.storage {
                values.insert(k.clone(), v.clone());
            }
        }
        let signer = SignatureKeyPair::read(
            provider.storage(),
            &st.signer_public,
            OMCP_MLS_CIPHERSUITE.signature_algorithm(),
        )
        .ok_or(OmcpError::Mls("restore signer"))?;
        let credential_with_key = CredentialWithKey {
            credential: BasicCredential::new(st.identity.clone()).into(),
            signature_key: signer.to_public_vec().into(),
        };
        let group = match &st.group_id {
            Some(id) => Some(
                MlsGroup::load(provider.storage(), &GroupId::from_slice(id))
                    .map_err(|_| OmcpError::Mls("load group"))?
                    .ok_or(OmcpError::Mls("group missing"))?,
            ),
            None => None,
        };
        Ok(Self {
            identity: st.identity.clone(),
            provider,
            signer,
            credential_with_key,
            key_package: None,
            kp_bytes: st.key_package.clone(),
            group,
            pq_kem: MlsPqKemKeyPair::generate()?,
            join_config: Self::join_config(),
        })
    }

    /// Whether a group exists (created or joined).
    pub fn has_group(&self) -> bool {
        self.group.is_some()
    }

    /// Current key package (without consuming it), serialized.
    pub fn key_package_bytes(&self) -> Result<Vec<u8>> {
        match &self.key_package {
            Some(b) => b
                .key_package()
                .tls_serialize_detached()
                .map_err(|_| OmcpError::Mls("serialize key package")),
            None => self
                .kp_bytes
                .clone()
                .ok_or(OmcpError::Mls("no key package")),
        }
    }

    /// Validate a peer's serialized key package.
    pub fn parse_key_package(&self, bytes: &[u8]) -> Result<KeyPackage> {
        KeyPackageIn::tls_deserialize_exact(bytes)
            .map_err(|_| OmcpError::Mls("key package decode"))?
            .validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|_| OmcpError::Mls("key package validate"))
    }

    /// Validate a peer's key package and return its credential identity.
    pub fn key_package_identity(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let kp = self.parse_key_package(bytes)?;
        let c = BasicCredential::try_from(kp.leaf_node().credential().clone())
            .map_err(|_| OmcpError::Mls("key package credential"))?;
        Ok(c.identity().to_vec())
    }

    /// Encrypt with `aad` authenticated (not encrypted) in the MLS framing.
    pub fn encrypt_app_message_with_aad(
        &mut self,
        plaintext: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>> {
        let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
        group.set_aad(aad.to_vec());
        let msg = group
            .create_message(&self.provider, &self.signer, plaintext)
            .map_err(|_| OmcpError::Mls("create_message"))?;
        msg.to_bytes()
            .map_err(|_| OmcpError::Mls("serialize app message"))
    }

    /// Decrypt an application message; returns `(plaintext, aad, sender leaf)`.
    pub fn decrypt_app_message_with_aad(
        &mut self,
        bytes: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>, u32)> {
        let (p, a, l, _) = self.decrypt_app_message_full(bytes)?;
        Ok((p, a, l))
    }

    /// Decrypt an application message: `(plaintext, aad, sender leaf, signed
    /// bytes)`. The signed bytes come from the vendored openmls patch
    /// (vendor/README.md); `None` only for messages of an older epoch.
    pub fn decrypt_app_message_full(&mut self, bytes: &[u8]) -> Result<DecryptedApp> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|_| OmcpError::Mls("app deserialize"))?;
        let protocol = msg
            .try_into_protocol_message()
            .map_err(|_| OmcpError::Mls("not protocol message"))?;
        let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
        let processed = group
            .process_message(&self.provider, protocol)
            .map_err(|_| OmcpError::Mls("process app"))?;
        let aad = processed.aad().to_vec();
        let leaf = match processed.sender() {
            Sender::Member(l) => l.u32(),
            _ => return Err(OmcpError::Mls("sender not a member")),
        };
        let signed = processed.signed_content().map(|s| SignedAppMessage {
            framed_content: s.framed_content.clone(),
            wire_format: s.wire_format,
            group_context: s.group_context.clone(),
            signature: s.signature.clone(),
            sender_leaf_node: s.sender_leaf_node.clone(),
        });
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => {
                Ok((app.into_bytes(), aad, leaf, signed))
            }
            _ => Err(OmcpError::Mls("expected application message")),
        }
    }

    /// Create a new MLS group with this member as sole creator.
    pub fn create_group(&mut self) -> Result<()> {
        let config = MlsGroupCreateConfig::builder()
            .ciphersuite(OMCP_MLS_CIPHERSUITE)
            .use_ratchet_tree_extension(true)
            .sender_ratchet_configuration(ratchet())
            .max_past_epochs(MESH_MAX_PAST_EPOCHS)
            .padding_size(MESH_PADDING)
            .build();
        // §23.10.1(3): the group ID is 32 random bytes with no meaning, and
        // the GroupContext carries no readable metadata (names travel only
        // inside encrypted application messages).
        let mut gid = [0u8; 32];
        getrandom::getrandom(&mut gid).map_err(|_| OmcpError::Mls("rng"))?;
        let group = MlsGroup::new_with_group_id(
            &self.provider,
            &self.signer,
            &config,
            GroupId::from_slice(&gid),
            self.credential_with_key.clone(),
        )
        .map_err(|_| OmcpError::Mls("create group"))?;
        self.group = Some(group);
        Ok(())
    }

    fn group_ref(&self) -> Result<&MlsGroup> {
        self.group.as_ref().ok_or(OmcpError::Mls("no group"))
    }

    /// The MLS group ID (32 random bytes for groups created here).
    pub fn group_id(&self) -> Result<Vec<u8>> {
        Ok(self.group_ref()?.group_id().as_slice().to_vec())
    }

    /// `group_ref` of this group (PROTOCOL §23.10).
    pub fn moderation_group_ref(&self) -> Result<[u8; 32]> {
        Ok(crate::moderation::group_ref(&self.group_id()?))
    }

    /// Current epoch number.
    pub fn epoch(&self) -> Result<u64> {
        Ok(self.group_ref()?.epoch().as_u64())
    }

    /// Number of members in the local group state.
    pub fn member_count(&self) -> Result<usize> {
        Ok(self.group_ref()?.members().count())
    }

    /// Own leaf index.
    pub fn own_leaf_index(&self) -> Result<u32> {
        Ok(self.group_ref()?.own_leaf_index().u32())
    }

    /// Take (and clear) this member’s current key package for an Add.
    pub fn take_key_package(&mut self) -> Result<KeyPackage> {
        let bundle = self
            .key_package
            .take()
            .ok_or(OmcpError::Mls("key package already used"))?;
        Ok(bundle.key_package().clone())
    }

    /// Refresh key package after the previous one was consumed.
    pub fn refresh_key_package(&mut self) -> Result<()> {
        let key_package = KeyPackage::builder()
            .build(
                OMCP_MLS_CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential_with_key.clone(),
            )
            .map_err(|_| OmcpError::Mls("refresh key package"))?;
        self.key_package = Some(key_package);
        Ok(())
    }

    /// Add members by key package; returns serialized commit + welcome bytes.
    pub fn add_members(&mut self, key_packages: &[KeyPackage]) -> Result<(Vec<u8>, Vec<u8>)> {
        if key_packages.is_empty() {
            return Err(OmcpError::Mls("no key packages"));
        }
        let (commit, welcome) = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            if group.members().count() + key_packages.len() > MAX_SMALL_GROUP_MEMBERS {
                return Err(OmcpError::Mls("group would exceed 32"));
            }
            let (commit, welcome, _group_info) = group
                .add_members(&self.provider, &self.signer, key_packages)
                .map_err(|_| OmcpError::Mls("add_members"))?;
            (commit, welcome)
        };
        self.group
            .as_mut()
            .unwrap()
            .merge_pending_commit(&self.provider)
            .map_err(|_| OmcpError::Mls("merge add commit"))?;
        let commit_bytes = commit
            .to_bytes()
            .map_err(|_| OmcpError::Mls("serialize commit"))?;
        let welcome_bytes = welcome
            .to_bytes()
            .map_err(|_| OmcpError::Mls("serialize welcome"))?;
        Ok((commit_bytes, welcome_bytes))
    }

    /// Join from a Welcome message (serialized `MlsMessageOut` bytes).
    pub fn join_from_welcome(&mut self, welcome_bytes: &[u8]) -> Result<()> {
        let msg = MlsMessageIn::tls_deserialize_exact(welcome_bytes)
            .map_err(|_| OmcpError::Mls("welcome deserialize"))?;
        let welcome = match msg.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err(OmcpError::Mls("not a welcome")),
        };
        let staged =
            StagedWelcome::new_from_welcome(&self.provider, &self.join_config, welcome, None)
                .map_err(|_| OmcpError::Mls("staged welcome"))?;
        let group = staged
            .into_group(&self.provider)
            .map_err(|_| OmcpError::Mls("into group"))?;
        self.group = Some(group);
        Ok(())
    }

    /// Process a commit/proposal from another member (serialized MLS message).
    pub fn process_protocol_message(&mut self, bytes: &[u8]) -> Result<()> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|_| OmcpError::Mls("message deserialize"))?;
        let protocol = msg
            .try_into_protocol_message()
            .map_err(|_| OmcpError::Mls("not protocol message"))?;
        let processed = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            group
                .process_message(&self.provider, protocol)
                .map_err(|_| OmcpError::Mls("process message"))?
        };
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => {
                self.group
                    .as_mut()
                    .unwrap()
                    .merge_staged_commit(&self.provider, *staged)
                    .map_err(|_| OmcpError::Mls("merge staged"))?;
            }
            ProcessedMessageContent::ApplicationMessage(_) => {
                return Err(OmcpError::Mls("expected commit, got app"));
            }
            ProcessedMessageContent::ProposalMessage(_)
            | ProcessedMessageContent::ExternalJoinProposalMessage(_)
            | ProcessedMessageContent::OwnPendingCommit
            | ProcessedMessageContent::OwnPrivateMessage => {}
        }
        Ok(())
    }

    /// Remove members by leaf index; returns serialized commit (no welcome).
    pub fn remove_members(&mut self, leaf_indices: &[u32]) -> Result<Vec<u8>> {
        let indices: Vec<LeafNodeIndex> = leaf_indices
            .iter()
            .map(|&i| LeafNodeIndex::new(i))
            .collect();
        let commit = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            let (commit, _welcome, _info) = group
                .remove_members(&self.provider, &self.signer, &indices)
                .map_err(|_| OmcpError::Mls("remove_members"))?;
            commit
        };
        self.group
            .as_mut()
            .unwrap()
            .merge_pending_commit(&self.provider)
            .map_err(|_| OmcpError::Mls("merge remove"))?;
        commit
            .to_bytes()
            .map_err(|_| OmcpError::Mls("serialize remove commit"))
    }

    /// Encrypt an application plaintext under the current MLS epoch.
    pub fn encrypt_app_message(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
        let msg = group
            .create_message(&self.provider, &self.signer, plaintext)
            .map_err(|_| OmcpError::Mls("create_message"))?;
        msg.to_bytes()
            .map_err(|_| OmcpError::Mls("serialize app message"))
    }

    /// Decrypt an application message and keep its signed bytes.
    pub fn decrypt_app_message_signed(
        &mut self,
        bytes: &[u8],
    ) -> Result<(Vec<u8>, Option<SignedAppMessage>)> {
        let (p, _, _, s) = self.decrypt_app_message_full(bytes)?;
        Ok((p, s))
    }

    /// Decrypt an application message; returns plaintext bytes.
    pub fn decrypt_app_message(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        let msg = MlsMessageIn::tls_deserialize_exact(bytes)
            .map_err(|_| OmcpError::Mls("app deserialize"))?;
        let protocol = msg
            .try_into_protocol_message()
            .map_err(|_| OmcpError::Mls("not protocol message"))?;
        let processed = {
            let group = self.group.as_mut().ok_or(OmcpError::Mls("no group"))?;
            group
                .process_message(&self.provider, protocol)
                .map_err(|_| OmcpError::Mls("process app"))?
        };
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(app) => Ok(app.into_bytes()),
            _ => Err(OmcpError::Mls("expected application message")),
        }
    }

    /// Export MLS secret then mix OMCP-MLS-PQ shared secrets (PROTOCOL §6.2).
    pub fn hybrid_exporter(&self, kem_shared_secrets: &[[u8; 32]]) -> Result<[u8; 32]> {
        let group = self.group_ref()?;
        let epoch = group.epoch().as_u64();
        let context = epoch.to_be_bytes();
        let mls_exporter = group
            .export_secret(self.provider.crypto(), MLS_PQ_EXPORTER_LABEL, &context, 32)
            .map_err(|_| OmcpError::Mls("export_secret"))?;
        mix_exporter(&mls_exporter, kem_shared_secrets)
    }

    /// Tag secret for a §22 mailbox tag: `MLS-Exporter("OMCP-tag", tag_epoch, 32)`
    /// (§22.7(5)); 1:1 chats are 2-member groups. Context = `tag_epoch` as u64-be.
    ///
    /// TODO(§22.7(5)): the MLS exporter also changes with the *MLS* epoch, so
    /// members must be at the same MLS epoch to agree; receivers should also
    /// try the previous MLS epoch's exporter during a commit race. The
    /// OMCP-MLS-PQ hybrid mix is not applied here (spec says plain exporter).
    pub fn tag_exporter(&self, tag_epoch: u64) -> Result<[u8; 32]> {
        let group = self.group_ref()?;
        let out = group
            .export_secret(
                self.provider.crypto(),
                crate::sync::tag::TAG_EXPORTER_LABEL,
                &tag_epoch.to_be_bytes(),
                32,
            )
            .map_err(|_| OmcpError::Mls("export_secret"))?;
        let mut s = [0u8; 32];
        s.copy_from_slice(&out);
        Ok(s)
    }

    /// Group key fingerprint for in-person comparison (group-security):
    /// `MLS-Exporter("OMCP-group-fp-v1", "", 32)` at the current epoch. It
    /// changes with every key rotation; all members see the same value.
    pub fn key_fingerprint(&self) -> Result<[u8; 32]> {
        let out = self
            .group_ref()?
            .export_secret(self.provider.crypto(), "OMCP-group-fp-v1", &[], 32)
            .map_err(|_| OmcpError::Mls("export_secret"))?;
        let mut s = [0u8; 32];
        s.copy_from_slice(&out);
        Ok(s)
    }

    /// Community live secret input (PROTOCOL §20.3):
    /// `MLS-Exporter("OMCP-live-v1", community_id ‖ stream_id, 32)` at the
    /// current MLS epoch. Every member derives the same value; non-members can't.
    pub fn live_exporter(&self, community_id: &[u8; 32], stream_id: &[u8; 16]) -> Result<[u8; 32]> {
        let group = self.group_ref()?;
        let mut ctx = community_id.to_vec();
        ctx.extend_from_slice(stream_id);
        let out = group
            .export_secret(
                self.provider.crypto(),
                crate::live::community::LIVE_EXPORTER_LABEL,
                &ctx,
                32,
            )
            .map_err(|_| OmcpError::Mls("export_secret"))?;
        let mut s = [0u8; 32];
        s.copy_from_slice(&out);
        Ok(s)
    }

    /// Build PQ encapsulations to `others` for the current epoch.
    ///
    /// Returns `(extension_bytes, kem_shared_secrets, hybrid_exporter)`.
    pub fn build_pq_epoch_secrets(
        &self,
        others: &[(u32, [u8; crate::handshake::MLKEM768_EK_LEN])],
    ) -> Result<PqEpochSecrets> {
        let epoch = self.epoch()?;
        let (ext, secrets) = encapsulate_to_members(epoch, others)?;
        let hybrid = self.hybrid_exporter(&secrets)?;
        Ok((ext.encode()?, secrets, hybrid))
    }

    /// Hard rule (§23.10.5(1), MR17): add members only if no KeyPackage
    /// carries a whitelisted team identity.
    pub fn add_members_excluding_team(
        &mut self,
        policy: &crate::moderation::TeamPolicy,
        key_packages: &[KeyPackage],
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        policy
            .reject_team_identities(
                key_packages
                    .iter()
                    .map(|k| k.leaf_node().credential().serialized_content()),
            )
            .map_err(|_| OmcpError::Mls("team key may not join a group"))?;
        self.add_members(key_packages)
    }

    /// Hard rule (MR17): join only if no member of the welcomed group is a
    /// whitelisted team identity; otherwise the group state is dropped.
    pub fn join_from_welcome_excluding_team(
        &mut self,
        policy: &crate::moderation::TeamPolicy,
        welcome_bytes: &[u8],
    ) -> Result<()> {
        self.join_from_welcome(welcome_bytes)?;
        let ids = self.member_identities()?;
        if policy
            .reject_team_identities(ids.iter().map(|(_, i)| i.as_slice()))
            .is_err()
        {
            self.erase_group();
            return Err(OmcpError::Mls("team key may not be a group member"));
        }
        Ok(())
    }

    /// Erase the group's MLS state from this member (takedown, §23.10.2(3b)).
    pub fn erase_group(&mut self) {
        if let Some(mut g) = self.group.take() {
            let _ = g.delete(self.provider.storage());
        }
    }

    /// Collect (leaf_index, identity) for debugging/tests.
    pub fn member_identities(&self) -> Result<Vec<(u32, Vec<u8>)>> {
        let g = self.group_ref()?;
        Ok(g.members()
            .map(|m| (m.index.u32(), m.credential.serialized_content().to_vec()))
            .collect())
    }
}

impl crate::sync::tag::TagSecretSource for OmcpMlsMember {
    fn tag_secret(&self, tag_epoch: u64) -> Result<[u8; 32]> {
        self.tag_exporter(tag_epoch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_to_one_chat_tags_from_mls_exporter() {
        use crate::sync::tag::{chat_subtag, chat_tag, rendezvous_label, TagSecretSource};
        let mut alice = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
        let mut bob = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
        let mut eve = OmcpMlsMember::generate(b"eve".to_vec()).unwrap();
        alice.create_group().unwrap();
        let (_c, w) = alice
            .add_members(&[bob.take_key_package().unwrap()])
            .unwrap();
        bob.join_from_welcome(&w).unwrap();
        assert_eq!(alice.member_count().unwrap(), 2);
        let (ta, tb) = (chat_tag(&alice, 77).unwrap(), chat_tag(&bob, 77).unwrap());
        assert_eq!(ta, tb);
        assert_ne!(ta, chat_tag(&alice, 78).unwrap());
        eve.create_group().unwrap();
        assert_ne!(ta, chat_tag(&eve, 77).unwrap());
        let bob_leaf = bob.own_leaf_index().unwrap();
        assert_eq!(
            chat_subtag(&alice, bob_leaf, 77).unwrap(),
            chat_subtag(&bob, bob_leaf, 77).unwrap()
        );
        assert_ne!(
            chat_subtag(&alice, alice.own_leaf_index().unwrap(), 77).unwrap(),
            chat_subtag(&bob, bob_leaf, 77).unwrap()
        );
        let sa = alice.tag_secret(77).unwrap();
        assert_eq!(
            rendezvous_label(&sa, 77).unwrap(),
            rendezvous_label(&bob.tag_secret(77).unwrap(), 77).unwrap()
        );
    }

    #[test]
    fn create_add_remove_encrypt_three_members() {
        let mut alice = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
        let mut bob = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
        let mut charlie = OmcpMlsMember::generate(b"charlie".to_vec()).unwrap();

        alice.create_group().unwrap();
        assert_eq!(alice.member_count().unwrap(), 1);

        let bob_kp = bob.take_key_package().unwrap();
        let (commit1, welcome_bob) = alice.add_members(&[bob_kp]).unwrap();
        bob.join_from_welcome(&welcome_bob).unwrap();
        assert_eq!(alice.member_count().unwrap(), 2);
        assert_eq!(bob.member_count().unwrap(), 2);

        let charlie_kp = charlie.take_key_package().unwrap();
        let (commit2, welcome_charlie) = alice.add_members(&[charlie_kp]).unwrap();
        bob.process_protocol_message(&commit2).unwrap();
        charlie.join_from_welcome(&welcome_charlie).unwrap();
        assert_eq!(alice.member_count().unwrap(), 3);
        assert_eq!(bob.member_count().unwrap(), 3);
        assert_eq!(charlie.member_count().unwrap(), 3);

        let ct = alice.encrypt_app_message(b"hello MLS group").unwrap();
        let p_b = bob.decrypt_app_message(&ct).unwrap();
        let p_c = charlie.decrypt_app_message(&ct).unwrap();
        assert_eq!(p_b, b"hello MLS group");
        assert_eq!(p_c, b"hello MLS group");

        let others = [
            (bob.own_leaf_index().unwrap(), *bob.pq_kem.public_bytes()),
            (
                charlie.own_leaf_index().unwrap(),
                *charlie.pq_kem.public_bytes(),
            ),
        ];
        let (_ext, secrets, hybrid) = alice.build_pq_epoch_secrets(&others).unwrap();
        let classical = alice.hybrid_exporter(&[]).unwrap();
        assert_ne!(hybrid, classical);
        assert_eq!(secrets.len(), 2);

        let charlie_leaf = charlie.own_leaf_index().unwrap();
        let rem = alice.remove_members(&[charlie_leaf]).unwrap();
        bob.process_protocol_message(&rem).unwrap();
        assert_eq!(alice.member_count().unwrap(), 2);
        assert_eq!(bob.member_count().unwrap(), 2);

        let _ = commit1;
    }

    #[test]
    fn one_to_one_restore_out_of_order_and_aad() {
        let mut alice = OmcpMlsMember::generate(b"alice".to_vec()).unwrap();
        let bob0 = OmcpMlsMember::generate(b"bob".to_vec()).unwrap();
        // Bob restarts between sending his key package and the Welcome.
        let kp_bytes = bob0.key_package_bytes().unwrap();
        let mut bob = OmcpMlsMember::restore(&bob0.export_state().unwrap()).unwrap();
        assert_eq!(bob.key_package_bytes().unwrap(), kp_bytes);
        alice.create_group().unwrap();
        let kp = alice.parse_key_package(&kp_bytes).unwrap();
        let (_c, w) = alice.add_members(&[kp]).unwrap();
        bob.join_from_welcome(&w).unwrap();
        let cts: Vec<Vec<u8>> = (0..5u8)
            .map(|i| alice.encrypt_app_message_with_aad(&[i], &[9, i]).unwrap())
            .collect();
        let mut alice = OmcpMlsMember::restore(&alice.export_state().unwrap()).unwrap();
        let mut bob = OmcpMlsMember::restore(&bob.export_state().unwrap()).unwrap();
        for i in [4usize, 0, 2, 1, 3] {
            let (p, aad, leaf) = bob.decrypt_app_message_with_aad(&cts[i]).unwrap();
            assert_eq!((p, aad), (vec![i as u8], vec![9, i as u8]));
            assert_eq!(leaf, alice.own_leaf_index().unwrap());
        }
        assert!(bob.decrypt_app_message_with_aad(&cts[1]).is_err(), "replay");
        let back = bob.encrypt_app_message_with_aad(b"ok", b"").unwrap();
        assert_eq!(alice.decrypt_app_message_with_aad(&back).unwrap().0, b"ok");
        assert_eq!(alice.tag_exporter(5).unwrap(), bob.tag_exporter(5).unwrap());
    }
}

//! Hopla team moderation over the FFI (PROTOCOL §23.9, REQUIREMENTS TM1–TM4).
//!
//! The team policy is compiled in from `config/team_moderation.conf` (public
//! keys only; it changes only with an app release). Moderation envelopes ride
//! the mesh on one public tag every node wants; each node verifies, applies and
//! relays them. Private chats are never read or touched (TM4).

use omcp_core::moderation::{
    hex32, is_team_mod_envelope, open_team_action, seal_team_action, ActionKind, Rejected,
    TeamAction, TeamPolicy,
};
use omcp_core::EnvelopeV1;

use crate::{invalid, now_ms, FfiError, NodeState, OmcpNode};

/// 0.1.23 state, discarded on upgrade (tombstones kept, MR32).
pub(crate) const LEGACY_MOD_KEY: &[u8] = b"team-mod-state";

/// Re-encodes any stored CBOR value as raw CBOR bytes.
mod serde_bytes_any {
    pub struct Any(pub Vec<u8>);
    impl<'de> serde::Deserialize<'de> for Any {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let v = ciborium::Value::deserialize(d)?;
            let mut b = Vec::new();
            ciborium::into_writer(&v, &mut b).map_err(serde::de::Error::custom)?;
            Ok(Any(b))
        }
    }
}

/// v2: per-(target, item) state with tombstones (§23.10).
pub(crate) const MOD_KEY: &[u8] = b"team-mod-state-v2";
const SHIPPED_POLICY: &str = include_str!("../../../config/team_moderation.conf");

/// DEV/TEST ONLY. An Ed25519 *public* key compiled in from the build
/// environment `OMCP_DEV_MOD_KEY`, only with the cargo feature `dev-mod-key`
/// (scripts enable it only for debug-flagged builds, never for release). The
/// production key line in `config/team_moderation.conf` stays empty until v1.0.
/// Also requires `debug_assertions`, so an optimised release build stays
/// keyless even if the feature were turned on by mistake.
#[cfg(all(feature = "dev-mod-key", debug_assertions))]
const DEV_MOD_KEY: Option<&str> = option_env!("OMCP_DEV_MOD_KEY");
#[cfg(not(all(feature = "dev-mod-key", debug_assertions)))]
const DEV_MOD_KEY: Option<&str> = None;

/// Policy from a config plus an optional dev public key. The dev key is used
/// only when the config has no team key (never mixed with production keys).
/// Returns `(policy, dev_key_in_use)`.
pub(crate) fn policy_for(config: &str, dev: Option<&str>) -> (TeamPolicy, bool) {
    let p = TeamPolicy::from_config(config).unwrap_or_default();
    if !p.members.is_empty() {
        return (p, false);
    }
    let dev = dev
        .and_then(|h| hex32(h.trim()))
        .filter(|k| ed25519_dalek::VerifyingKey::from_bytes(k).is_ok());
    match dev {
        Some(k) => (TeamPolicy::from_public_keys(&[k], 1), true),
        None => (p, false),
    }
}

/// The policy shipped with this build (production config, plus the dev key
/// in dev-flagged builds).
pub(crate) fn shipped_policy() -> TeamPolicy {
    policy_for(SHIPPED_POLICY, DEV_MOD_KEY).0
}

/// Whether this build's policy comes from the dev key.
pub(crate) fn shipped_policy_is_dev() -> bool {
    policy_for(SHIPPED_POLICY, DEV_MOD_KEY).1
}

/// One line of the public takedown log (§23.9(5), §23.10.4, MR9, MR21).
/// Holds only the evidence fingerprint and a reason category.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TeamLogEntry {
    /// `takedown`, `restore`, `group_takedown`, `group_restore`,
    /// `post_takedown`, `topic_takedown`, `live_takedown`, `item_restore`.
    pub kind: String,
    /// community_id, or group_ref for group actions (never a name).
    pub community_id_hex: String,
    /// post / topic / stream id; empty when none.
    pub item_id_hex: String,
    /// BLAKE2b-256 of the evidence file; empty when the decision had none.
    pub evidence_hash_hex: String,
    /// Reason category 1–9 (§23.10.4(3)); 0 for restores.
    pub reason_code: u8,
    pub note: String,
    pub epoch: u64,
    /// Short ids of the team keys that signed (`omcp1:…`).
    pub signed_by: Vec<String>,
    pub applied_ms: u64,
}

/// Result of handing a signed action to the node.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TeamActionResult {
    Applied,
    NoTeam,
    Malformed,
    BadSignature,
    NotWhitelisted,
    BelowThreshold,
    Replay,
}

impl From<Rejected> for TeamActionResult {
    fn from(r: Rejected) -> Self {
        match r {
            Rejected::NoTeam => Self::NoTeam,
            Rejected::Malformed => Self::Malformed,
            Rejected::BadSignature => Self::BadSignature,
            Rejected::NotWhitelisted => Self::NotWhitelisted,
            Rejected::BelowThreshold => Self::BelowThreshold,
            Rejected::Replay => Self::Replay,
        }
    }
}

impl NodeState {
    pub(crate) fn load_moderation(&mut self) -> Result<(), FfiError> {
        let v2: Option<Option<omcp_core::moderation::ModerationState>> =
            self.db.get_meta(MOD_KEY)?;
        self.mesh.moderation = match v2.flatten() {
            Some(m) => m,
            None => {
                // MR32: 0.1.23 state (kinds 3/4 were Live) is discarded; its
                // community tombstones are kept; actions re-sync from the mesh.
                let legacy: Option<serde_bytes_any::Any> =
                    self.db.get_meta(LEGACY_MOD_KEY).ok().flatten();
                let m = legacy
                    .and_then(|l| omcp_core::moderation::ModerationState::from_legacy_v1(&l.0))
                    .unwrap_or_default();
                self.db.put_meta(LEGACY_MOD_KEY, &())?;
                self.db.put_meta(MOD_KEY, &m)?;
                m
            }
        };
        self.mesh.engine.set_community_blocks(
            omcp_core::community::CommunityBlocks::from_moderation(&self.mesh.moderation),
        );
        Ok(())
    }

    fn save_moderation(&self) -> Result<(), FfiError> {
        self.db.put_meta(MOD_KEY, &self.mesh.moderation)?;
        Ok(())
    }

    fn apply_team_action(&mut self, a: &TeamAction) -> Result<TeamActionResult, FfiError> {
        let policy = self.mesh.team.clone();
        match self.mesh.moderation.apply(&policy, a, now_ms()) {
            Ok(_) => {
                if a.kind == ActionKind::LiveTakedown {
                    self.block_live_host_for(a);
                }
                self.save_moderation()?;
                self.mesh.engine.set_community_blocks(
                    omcp_core::community::CommunityBlocks::from_moderation(&self.mesh.moderation),
                );
                // Full erasure on this device (§23.10.2(3), §23.10.6).
                self.enforce_group_moderation()?;
                self.enforce_live_moderation();
                self.enforce_open_communities()?;
                Ok(TeamActionResult::Applied)
            }
            Err(r) => Ok(r.into()),
        }
    }

    /// MR29: the stream's host may not start Live in that community for 24 h.
    fn block_live_host_for(&mut self, a: &TeamAction) {
        let sid: [u8; 16] = a.item_id[16..].try_into().expect("16");
        let lv = &self.mesh.live;
        let host = if let Some(h) = lv.host.as_ref().filter(|h| h.stream_id() == sid) {
            Some(h.invite(true).host)
        } else {
            lv.known.get(&sid).map(|(_, i)| i.host).or_else(|| {
                lv.listening
                    .as_ref()
                    .filter(|l| l.invite.key.stream_id == sid)
                    .map(|l| l.invite.host)
            })
        };
        if let Some(h) = host {
            self.mesh
                .moderation
                .block_live_host(a.target_id, h, now_ms());
        }
    }

    /// A moderation envelope arrived from the mesh. `true` if it was one
    /// (whether or not it was valid): it is never treated as a chat.
    pub(crate) fn on_team_mod_envelope(&mut self, env: &EnvelopeV1) -> Result<bool, FfiError> {
        if !is_team_mod_envelope(env) {
            return Ok(false);
        }
        if let Ok(a) = open_team_action(env) {
            self.apply_team_action(&a)?;
        }
        Ok(true)
    }

    #[cfg(test)]
    pub(crate) fn set_team_policy(&mut self, p: TeamPolicy) {
        self.mesh.team = p;
    }
}

fn nz(h: &[u8; 32]) -> String {
    if *h == [0u8; 32] {
        String::new()
    } else {
        hex::encode(h)
    }
}

/// One message the reporter picked, with the MLS signed bytes the app keeps
/// for it (§23.10.1(2)).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EvidenceMessage {
    pub sender_id_hex: String,
    pub framed_content: Vec<u8>,
    pub group_context: Vec<u8>,
    pub signature: Vec<u8>,
    pub leaf_node: Vec<u8>,
    /// MLS wire format: 2 = PrivateMessage (verifiable).
    pub wire_format: u16,
    pub body: Vec<u8>,
}

/// Result of checking an `.omcp-evidence` file.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EvidenceCheck {
    pub evidence_hash_hex: String,
    pub group_ref_hex: String,
    pub reporter_sig_ok: bool,
    pub verified: u32,
    pub total: u32,
}

fn short(h: &[u8; 32]) -> String {
    format!("omcp1:{}…", hex::encode(&h[..4]))
}

#[uniffi::export]
impl OmcpNode {
    /// Whether this build ships a moderation team (TM4: other apps may not).
    pub fn moderation_has_team(&self) -> bool {
        !self.inner.lock().mesh.team.members.is_empty()
    }

    /// For About / debug: `production` (key in config), `dev` (DEV/TEST key,
    /// debug builds only) or `disabled` (no key: takedowns are not enforced).
    pub fn moderation_status(&self) -> String {
        let st = self.inner.lock();
        if st.mesh.team.members.is_empty() {
            "disabled".into()
        } else if st.mesh.team_dev {
            "dev".into()
        } else {
            "production".into()
        }
    }

    /// Release apps call this at start: a dev key never enforces in release,
    /// even if the native library was built with one by mistake.
    pub fn moderation_drop_dev_key(&self) {
        let mut st = self.inner.lock();
        if st.mesh.team_dev {
            st.mesh.team = TeamPolicy::from_config(SHIPPED_POLICY).unwrap_or_default();
            st.mesh.team_dev = false;
        }
    }

    /// Verify a signed TAKEDOWN / RESTORE (hex wire form from the offline
    /// `team_mod` tool), apply it here and gossip it at high priority.
    /// Rejected actions are not gossiped.
    pub fn moderation_publish(&self, action_hex: String) -> Result<TeamActionResult, FfiError> {
        let bytes =
            hex::decode(action_hex.trim()).map_err(|_| invalid("team action is not hex"))?;
        let Ok(a) = TeamAction::decode(&bytes) else {
            return Ok(TeamActionResult::Malformed);
        };
        let mut st = self.inner.lock();
        let r = st.apply_team_action(&a)?;
        if r == TeamActionResult::Applied {
            let now = now_ms();
            let ttl = st.mesh.engine.new_ttl();
            let env = seal_team_action(&a, now, ttl).map_err(|_| invalid("team action"))?;
            let ev = st.mesh.engine.submit_own(env, now)?;
            st.route(ev)?;
        }
        Ok(r)
    }

    /// Transparency log, newest first.
    pub fn moderation_log(&self) -> Vec<TeamLogEntry> {
        self.inner
            .lock()
            .mesh
            .moderation
            .log()
            .into_iter()
            .map(|e| TeamLogEntry {
                kind: match e.kind {
                    ActionKind::Takedown => "takedown",
                    ActionKind::Restore => "restore",
                    ActionKind::GroupTakedown => "group_takedown",
                    ActionKind::GroupRestore => "group_restore",
                    ActionKind::PostTakedown => "post_takedown",
                    ActionKind::TopicTakedown => "topic_takedown",
                    ActionKind::LiveTakedown => "live_takedown",
                    ActionKind::ItemRestore => "item_restore",
                }
                .into(),
                community_id_hex: hex::encode(e.target_id),
                item_id_hex: nz(&e.item_id),
                evidence_hash_hex: nz(&e.evidence_hash),
                reason_code: e.reason_code,
                note: e.note,
                epoch: e.epoch,
                signed_by: e.signers.iter().map(short).collect(),
                applied_ms: e.applied_ms,
            })
            .collect()
    }

    /// Reason code if this live stream was taken down by the team.
    pub fn moderation_live_removed_reason(&self, stream_id_hex: String) -> Option<u8> {
        let id: [u8; 16] = hex::decode(stream_id_hex.trim()).ok()?.try_into().ok()?;
        self.inner
            .lock()
            .mesh
            .moderation
            .live_removal(&id)
            .map(|r| r.reason.code())
    }

    /// Reason code if the community was removed by the team, else `None`.
    /// Clients hide it, refuse joins and posts, and relays drop its envelopes.
    pub fn moderation_removed_reason(&self, community_id_hex: String) -> Option<u8> {
        let id = hex32(community_id_hex.trim())?;
        self.inner
            .lock()
            .mesh
            .moderation
            .removal(&id)
            .map(|r| r.reason.code())
    }

    /// Reason code if a private group (by `group_ref` hex) was taken down.
    pub fn moderation_group_removed_reason(&self, group_ref_hex: String) -> Option<u8> {
        let r = hex32(group_ref_hex.trim())?;
        self.inner
            .lock()
            .mesh
            .moderation
            .group_removal(&r)
            .map(|r| r.reason.code())
    }

    /// Reason code if a post / topic / live item was taken down by the team.
    pub fn moderation_item_removed_reason(
        &self,
        community_id_hex: String,
        item_id_hex: String,
    ) -> Option<u8> {
        let c = hex32(community_id_hex.trim())?;
        let i = omcp_core::moderation::pad_id(&hex::decode(item_id_hex.trim()).ok()?);
        self.inner
            .lock()
            .mesh
            .moderation
            .item_removal(&c, &i)
            .map(|r| r.reason.code())
    }

    /// Received private-group messages kept with their MLS signed bytes,
    /// oldest first, ready to pass to [`Self::report_export_evidence`].
    pub fn report_kept_messages(
        &self,
        group_id_hex: String,
    ) -> Result<Vec<EvidenceMessage>, FfiError> {
        let id = hex32(group_id_hex.trim()).ok_or_else(|| invalid("group id"))?;
        let st = self.inner.lock();
        let c = st
            .mesh
            .community
            .list
            .get(&id)
            .ok_or_else(|| invalid("reports exist only for private groups"))?;
        Ok(c.signed
            .iter()
            .filter_map(|(_, sg, body)| {
                let r =
                    omcp_core::moderation::evidence::ReportedMessage::from_signed(sg, body.clone())
                        .ok()?;
                Some(EvidenceMessage {
                    sender_id_hex: hex::encode(r.sender_id),
                    framed_content: r.framed_content,
                    group_context: r.group_context,
                    signature: r.signature,
                    leaf_node: r.leaf_node,
                    wire_format: r.wire_format,
                    body: r.body,
                })
            })
            .collect())
    }

    /// Export an `.omcp-evidence` file (§23.10.3) from messages the reporter
    /// picked in a private group. Never for 1:1 chats. Reports are anonymous:
    /// the file is signed with a fresh one-off key. Contains hashes and the
    /// selected messages only, never the group name or a member list.
    pub fn report_export_evidence(
        &self,
        group_id_hex: String,
        messages: Vec<EvidenceMessage>,
        reason_code: u8,
        note: Option<String>,
    ) -> Result<Vec<u8>, FfiError> {
        use omcp_core::moderation::evidence::{EvidenceFile, ReportedMessage};
        let id = hex32(group_id_hex.trim()).ok_or_else(|| invalid("group id"))?;
        let gid = {
            let st = self.inner.lock();
            let c = st
                .mesh
                .community
                .list
                .get(&id)
                .ok_or_else(|| invalid("reports exist only for private groups"))?;
            c.member.group_id().map_err(|_| invalid("group"))?
        };
        let msgs = messages
            .into_iter()
            .map(|m| {
                Ok(ReportedMessage {
                    sender_id: hex32(&m.sender_id_hex).ok_or_else(|| invalid("sender id"))?,
                    group_context_hash: omcp_core::moderation::peer_id_hash_bytes(&m.group_context),
                    framed_content: m.framed_content,
                    group_context: m.group_context,
                    signature: m.signature,
                    leaf_node: m.leaf_node,
                    wire_format: m.wire_format,
                    body: m.body,
                    media: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, FfiError>>()?;
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).map_err(|_| invalid("rng"))?;
        let one_off = ed25519_dalek::SigningKey::from_bytes(&seed);
        EvidenceFile::export(
            &gid,
            msgs,
            reason_code,
            note,
            None,
            vec![],
            &one_off,
            now_ms(),
        )
        .map_err(|_| invalid("evidence export"))
    }

    /// Check an evidence file: reporter signature and each message's MLS
    /// sender signature (§23.10.1(4)).
    pub fn evidence_verify(&self, file: Vec<u8>) -> Result<EvidenceCheck, FfiError> {
        use omcp_core::moderation::evidence::{verify, EvidenceFile};
        let v = verify(&file).map_err(|_| invalid("not an evidence file"))?;
        let f = EvidenceFile::decode(&file).map_err(|_| invalid("not an evidence file"))?;
        Ok(EvidenceCheck {
            evidence_hash_hex: hex::encode(v.evidence_hash),
            group_ref_hex: hex::encode(f.group_ref),
            reporter_sig_ok: v.reporter_sig_ok,
            verified: v.verified() as u32,
            total: v.messages.len() as u32,
        })
    }
}

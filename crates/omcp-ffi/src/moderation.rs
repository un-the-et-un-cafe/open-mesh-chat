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

const MOD_KEY: &[u8] = b"team-mod-state";
const SHIPPED_POLICY: &str = include_str!("../../../config/team_moderation.conf");

/// The policy shipped with this build.
pub(crate) fn shipped_policy() -> TeamPolicy {
    TeamPolicy::from_config(SHIPPED_POLICY).unwrap_or_default()
}

/// One line of the public "Removed communities" log (§23.9(5), MR9).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TeamLogEntry {
    /// `takedown` or `restore`.
    pub kind: String,
    pub community_id_hex: String,
    /// 1 hate, 2 illegal, 3 terror, 4 harassment, 5 spam, 9 other; 0 for restore.
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
        self.mesh.moderation = self.db.get_meta(MOD_KEY)?.unwrap_or_default();
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
                self.save_moderation()?;
                self.enforce_live_moderation();
                Ok(TeamActionResult::Applied)
            }
            Err(r) => Ok(r.into()),
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

fn short(h: &[u8; 32]) -> String {
    format!("omcp1:{}…", hex::encode(&h[..4]))
}

#[uniffi::export]
impl OmcpNode {
    /// Whether this build ships a moderation team (TM4: other apps may not).
    pub fn moderation_has_team(&self) -> bool {
        !self.inner.lock().mesh.team.members.is_empty()
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
                    ActionKind::Takedown => "takedown".into(),
                    ActionKind::Restore => "restore".into(),
                    ActionKind::TakedownLive => "takedown_live".into(),
                    ActionKind::RestoreLive => "restore_live".into(),
                },
                community_id_hex: hex::encode(e.community_id),
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
}

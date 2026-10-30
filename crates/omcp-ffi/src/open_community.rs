//! Open communities over the FFI (PROTOCOL §20.1, §23.10.6; TM10, TM12).
//!
//! Forum-style topics and threads; posts are signed by their author but NOT
//! end-to-end encrypted, and are relayed over the mesh on the community's
//! public tag. The app shows the "posts are public" notice when
//! [`OmcpNode::open_community_posts_are_public`] is true. History is kept 30
//! days or 50 MB per community in its own store, separate from the relay cap.
//! Team takedowns erase the community / topic / post on this device.

use ed25519_dalek::SigningKey;
use omcp_core::community::{self as oc, Control, ControlKind, Post, Wire};
use omcp_core::EnvelopeV1;

use crate::{invalid, now_ms, parse_id, FfiError, NodeState, OmcpNode};

const STATE_KEY: &[u8] = b"open-communities-v1";

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenCommunityInfo {
    pub community_id_hex: String,
    pub name: String,
    pub members: u32,
    pub is_founder: bool,
    pub is_moderator: bool,
    /// Always true: show "Posts in communities are public…" (MR27).
    pub posts_are_public: bool,
    /// Founder-signed genesis frame (hex) to share as an invite link / QR.
    pub invite_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenCommunityTopic {
    pub topic_id_hex: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenCommunityPost {
    pub post_id_hex: String,
    pub parent_id_hex: Option<String>,
    pub author_hex: String,
    pub created_ms: u64,
    /// Empty when removed.
    pub body: String,
    /// Moderator's reason when removed (shown to everyone, moderators too).
    pub removed_reason: Option<String>,
}

impl NodeState {
    pub(crate) fn load_open_communities(&mut self) -> Result<(), FfiError> {
        let (open, genesis): (oc::CommunityStore, _) =
            self.db.get_meta(STATE_KEY)?.unwrap_or_default();
        self.mesh.open = open;
        self.mesh.open_genesis = genesis;
        self.mesh.open.enforce(&self.mesh.moderation);
        Ok(())
    }

    fn save_open_communities(&mut self) -> Result<(), FfiError> {
        self.mesh
            .open_genesis
            .retain(|id, _| self.mesh.open.communities.contains_key(id));
        self.db
            .put_meta(STATE_KEY, &(&self.mesh.open, &self.mesh.open_genesis))?;
        Ok(())
    }

    /// Erase everything a team takedown names (§23.10.6).
    pub(crate) fn enforce_open_communities(&mut self) -> Result<(), FfiError> {
        let before = self.mesh.open.clone();
        self.mesh.open.enforce(&self.mesh.moderation);
        if self.mesh.open != before {
            self.save_open_communities()?;
        }
        Ok(())
    }

    /// A public envelope on one of our communities' tags.
    pub(crate) fn on_open_community_envelope(
        &mut self,
        env: &EnvelopeV1,
    ) -> Result<bool, FfiError> {
        let ids: Vec<[u8; 32]> = self.mesh.open.communities.keys().copied().collect();
        for id in ids {
            let Ok(w) = oc::open(env, &id) else {
                continue;
            };
            let now = now_ms();
            let m = self.mesh.moderation.clone();
            let ok = match &w {
                Wire::Post(p) if p.community_id == id => {
                    self.mesh.open.add_post(p, &m, now).is_ok()
                }
                Wire::Control(c) if c.community_id == id => {
                    self.mesh.open.apply_control(c, &m).is_ok()
                }
                _ => false,
            };
            if ok {
                self.save_open_communities()?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn publish_open(&mut self, w: &Wire, id: &[u8; 32]) -> Result<(), FfiError> {
        let now = now_ms();
        let ttl = self.mesh.engine.new_ttl();
        let env = oc::seal(w, id, now, ttl);
        let ev = self.mesh.engine.submit_own(env, now)?;
        self.route(ev)?;
        Ok(())
    }

    fn open_info(&self, id: &[u8; 32], me: &[u8; 32]) -> Option<OpenCommunityInfo> {
        let c = self.mesh.open.get(id)?;
        let genesis = self.mesh.open_genesis.get(id).cloned().unwrap_or_default();
        Some(OpenCommunityInfo {
            community_id_hex: hex::encode(id),
            name: c.name.clone(),
            members: c.members.len() as u32,
            is_founder: c.founder == *me,
            is_moderator: c.is_moderator(me),
            posts_are_public: oc::posts_are_public(),
            invite_hex: hex::encode(genesis),
        })
    }
}

fn cbor<T: serde::Serialize>(v: &T) -> Vec<u8> {
    let mut b = Vec::new();
    ciborium::into_writer(v, &mut b).expect("cbor to vec");
    b
}

impl OmcpNode {
    fn key_and_me(&self) -> Result<(SigningKey, [u8; 32]), FfiError> {
        let k = self.signing_key()?;
        let me = k.verifying_key().to_bytes();
        Ok((k, me))
    }

    fn control(&self, id: [u8; 32], kind: ControlKind) -> Result<(), FfiError> {
        let (k, _) = self.key_and_me()?;
        let c = Control::new(&k, id, kind, now_ms());
        let mut st = self.inner.lock();
        let m = st.mesh.moderation.clone();
        st.mesh
            .open
            .apply_control(&c, &m)
            .map_err(|_| invalid("community action refused"))?;
        st.save_open_communities()?;
        st.publish_open(&Wire::Control(c), &id)
    }
}

#[uniffi::export]
impl OmcpNode {
    /// Community posts are public, not end-to-end encrypted (TM12).
    pub fn open_community_posts_are_public(&self) -> bool {
        oc::posts_are_public()
    }

    /// Start an open community (you are the founder).
    pub fn open_community_create(&self, name: String) -> Result<OpenCommunityInfo, FfiError> {
        let (k, me) = self.key_and_me()?;
        let mut st = self.inner.lock();
        let g = st
            .mesh
            .open
            .create(&k, name.trim(), now_ms())
            .map_err(|_| invalid("community name"))?;
        let id = g.community_id;
        st.mesh.open_genesis.insert(id, cbor(&g));
        st.save_open_communities()?;
        st.publish_open(&Wire::Control(g), &id)?;
        st.open_info(&id, &me).ok_or_else(|| invalid("community"))
    }

    /// Join from an invite (the founder-signed genesis frame, hex).
    pub fn open_community_join(&self, invite_hex: String) -> Result<OpenCommunityInfo, FfiError> {
        let bytes = hex::decode(invite_hex.trim()).map_err(|_| invalid("invite"))?;
        let g: Control = ciborium::from_reader(bytes.as_slice()).map_err(|_| invalid("invite"))?;
        if !matches!(g.kind, ControlKind::Create { .. }) {
            return Err(invalid("invite"));
        }
        let id = g.community_id;
        let (_, me) = self.key_and_me()?;
        {
            let mut st = self.inner.lock();
            let m = st.mesh.moderation.clone();
            st.mesh
                .open
                .apply_control(&g, &m)
                .map_err(|_| invalid("community removed or invalid"))?;
            st.mesh.open_genesis.insert(id, bytes);
        }
        self.control(id, ControlKind::Join)?;
        let st = self.inner.lock();
        st.open_info(&id, &me).ok_or_else(|| invalid("community"))
    }

    pub fn open_communities(&self) -> Vec<OpenCommunityInfo> {
        let Ok((_, me)) = self.key_and_me() else {
            return Vec::new();
        };
        let st = self.inner.lock();
        st.mesh
            .open
            .communities
            .keys()
            .filter_map(|id| st.open_info(id, &me))
            .collect()
    }

    pub fn open_community_new_topic(
        &self,
        community_id_hex: String,
        title: String,
    ) -> Result<String, FfiError> {
        let id = parse_id(&community_id_hex)?;
        let mut t = [0u8; 32];
        getrandom::getrandom(&mut t).map_err(|_| invalid("rng"))?;
        self.control(
            id,
            ControlKind::NewTopic {
                topic_id: t,
                title: title.trim().into(),
            },
        )?;
        Ok(hex::encode(t))
    }

    pub fn open_community_topics(&self, community_id_hex: String) -> Vec<OpenCommunityTopic> {
        let Ok(id) = parse_id(&community_id_hex) else {
            return Vec::new();
        };
        let st = self.inner.lock();
        st.mesh
            .open
            .get(&id)
            .map(|c| {
                c.topics
                    .iter()
                    .map(|(t, x)| OpenCommunityTopic {
                        topic_id_hex: hex::encode(t),
                        title: x.title.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Post (≤ 4 KB) to a topic, optionally as a reply. Returns the post id.
    pub fn open_community_post(
        &self,
        community_id_hex: String,
        topic_id_hex: String,
        parent_id_hex: Option<String>,
        body: String,
    ) -> Result<String, FfiError> {
        let id = parse_id(&community_id_hex)?;
        let topic = parse_id(&topic_id_hex)?;
        let parent = parent_id_hex.map(|p| parse_id(&p)).transpose()?;
        let (k, _) = self.key_and_me()?;
        let now = now_ms();
        let p =
            Post::new(&k, id, topic, parent, &body, now).map_err(|_| invalid("post over 4 KB"))?;
        let mut st = self.inner.lock();
        let m = st.mesh.moderation.clone();
        let pid = st
            .mesh
            .open
            .add_post(&p, &m, now)
            .map_err(|_| invalid("post refused"))?;
        st.save_open_communities()?;
        st.publish_open(&Wire::Post(p), &id)?;
        Ok(hex::encode(pid))
    }

    pub fn open_community_posts(
        &self,
        community_id_hex: String,
        topic_id_hex: String,
    ) -> Vec<OpenCommunityPost> {
        let (Ok(id), Ok(t)) = (parse_id(&community_id_hex), parse_id(&topic_id_hex)) else {
            return Vec::new();
        };
        let st = self.inner.lock();
        let Some(c) = st.mesh.open.get(&id) else {
            return Vec::new();
        };
        c.topic_posts(&t)
            .into_iter()
            .map(|(pid, sp)| OpenCommunityPost {
                post_id_hex: hex::encode(pid),
                parent_id_hex: sp.parent.map(hex::encode),
                author_hex: sp
                    .post
                    .as_ref()
                    .map(|p| hex::encode(p.author))
                    .unwrap_or_default(),
                created_ms: sp.created_ms,
                body: sp.post.as_ref().map(|p| p.body.clone()).unwrap_or_default(),
                removed_reason: sp.removed_reason.clone(),
            })
            .collect()
    }

    /// Founder only: make a member a moderator (member = Ed25519 key hex).
    pub fn open_community_appoint_moderator(
        &self,
        community_id_hex: String,
        member_key_hex: String,
    ) -> Result<(), FfiError> {
        let id = parse_id(&community_id_hex)?;
        let member = parse_id(&member_key_hex)?;
        self.control(id, ControlKind::AppointModerator { member })
    }

    /// Moderator: remove a post. A reason is required; the post is hidden
    /// from everyone, moderators included.
    pub fn open_community_remove_post(
        &self,
        community_id_hex: String,
        post_id_hex: String,
        reason: String,
    ) -> Result<(), FfiError> {
        let id = parse_id(&community_id_hex)?;
        let post_id = parse_id(&post_id_hex)?;
        self.control(id, ControlKind::RemovePost { post_id, reason })
    }

    /// Community Live is members-only.
    pub fn open_community_can_join_live(&self, community_id_hex: String) -> bool {
        let (Ok(id), Ok((_, me))) = (parse_id(&community_id_hex), self.key_and_me()) else {
            return false;
        };
        let st = self.inner.lock();
        st.mesh
            .open
            .get(&id)
            .map(|c| c.can_join_live(&me))
            .unwrap_or(false)
    }

    /// Members with their role ("founder", "moderator", "member").
    pub fn open_community_members(&self, community_id_hex: String) -> Vec<OpenCommunityMember> {
        let (Ok(id), Ok((_, me))) = (parse_id(&community_id_hex), self.key_and_me()) else {
            return Vec::new();
        };
        let st = self.inner.lock();
        let Some(c) = st.mesh.open.get(&id) else {
            return Vec::new();
        };
        let mut v: Vec<OpenCommunityMember> = c
            .members
            .iter()
            .map(|k| OpenCommunityMember {
                key_hex: hex::encode(k),
                role: if *k == c.founder {
                    "founder"
                } else if c.moderators.contains(k) {
                    "moderator"
                } else {
                    "member"
                }
                .into(),
                is_me: *k == me,
            })
            .collect();
        v.sort_by_key(|m| match m.role.as_str() {
            "founder" => 0,
            "moderator" => 1,
            _ => 2,
        });
        v
    }

    /// Anonymous report of a community post: the author-signed post plus a
    /// reason and optional note. No reporter key or identity (Q40).
    pub fn open_community_report_post(
        &self,
        community_id_hex: String,
        post_id_hex: String,
        reason_code: u8,
        note: Option<String>,
    ) -> Result<Vec<u8>, FfiError> {
        let id = parse_id(&community_id_hex)?;
        let pid = parse_id(&post_id_hex)?;
        let st = self.inner.lock();
        let p = st
            .mesh
            .open
            .get(&id)
            .and_then(|c| c.posts.get(&pid))
            .and_then(|sp| sp.post.clone())
            .ok_or_else(|| invalid("post not available"))?;
        let note: String = note.unwrap_or_default().chars().take(500).collect();
        Ok(cbor(&("OMCP-community-report-v1", p, reason_code, note)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OpenCommunityMember {
    pub key_hex: String,
    pub role: String,
    pub is_me: bool,
}

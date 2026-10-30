//! Open communities (PROTOCOL §20.1, §23.10.6; REQUIREMENTS TM10, TM12).
//!
//! A community is an open, forum-style space with topics and threaded posts.
//! Content is **signed but not end-to-end encrypted**: every post carries its
//! author's Ed25519 signature and is relayed over the mesh in public
//! envelopes on a per-topic tag. Apps must show [`POSTS_ARE_PUBLIC_NOTICE`]
//! on the join screen and in the composer ([`posts_are_public`]).
//!
//! Limits: [`MAX_MEMBERS`] members, [`MAX_POST_BYTES`] per post, history kept
//! [`HISTORY_MAX_AGE_MS`] or [`HISTORY_MAX_BYTES`] per community (oldest
//! dropped first). This history budget is separate from the daily relay cap.
//!
//! Roles: only the founder appoints moderators. A moderator removal needs a
//! reason; the post body is erased and the post is hidden from everyone,
//! moderators included. Community Live is members-only ([`Community::can_join_live`]).
//!
//! Team takedowns (§23.10.6) fully erase the community, topic or post and
//! leave a tombstone that is applied before any backup restore
//! ([`CommunityStore::enforce`], [`CommunityStore::restore_backup`]).

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{OmcpError, Result};
use crate::framing::{flags, EnvelopeV1};
use crate::moderation::{h256, ActionKind, ModerationState, Tombstone};
use crate::suite::{ENVELOPE_VERSION_V1, SUITE_OMCP_2026_A};
use crate::sync::relay::put_expires_in_aad;
use crate::sync::tag::{parse_routing_aad, put_tag_in_aad, MailboxTag};

#[cfg(test)]
mod tests;

pub const MAX_MEMBERS: usize = 5_000;
pub const MAX_POST_BYTES: usize = 4 * 1024;
pub const HISTORY_MAX_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const HISTORY_MAX_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_NAME: usize = 60;
pub const MAX_REASON: usize = 500;
/// Community posts are public (TM12, MR27). The app shows the notice when true.
pub const POSTS_ARE_PUBLIC: bool = true;
pub const POSTS_ARE_PUBLIC_NOTICE: &str = "Posts in communities are public. Anyone in the community can read them, and they are not end-to-end encrypted.";
const POST_DOMAIN: &[u8] = b"OMCP-community-post-v1\x00";
const CTRL_DOMAIN: &[u8] = b"OMCP-community-ctrl-v1\x00";
const TAG_DOMAIN: &[u8] = b"OMCP-community-tag-v1\x00";
const ENVELOPE_FLAGS: u16 = flags::RELAY_OK | flags::STORE_OK;

/// Flag for the app's "community posts are public" notice.
pub fn posts_are_public() -> bool {
    POSTS_ARE_PUBLIC
}

fn err<T>(m: &'static str) -> Result<T> {
    Err(OmcpError::Sync(m))
}

fn verify(key: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> bool {
    VerifyingKey::from_bytes(key)
        .and_then(|k| k.verify_strict(msg, &Signature::from_bytes(sig)))
        .is_ok()
}

/// Public mesh tag of one community (all its topics).
pub fn community_tag(community_id: &[u8; 32]) -> MailboxTag {
    let h = h256(&[TAG_DOMAIN, community_id]);
    h[..8].try_into().expect("8")
}

mod sig64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8; 64], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 64], D::Error> {
        let v: serde_bytes_buf::Buf = Deserialize::deserialize(d)?;
        v.0.try_into()
            .map_err(|_| serde::de::Error::custom("signature must be 64 bytes"))
    }
    mod serde_bytes_buf {
        pub struct Buf(pub Vec<u8>);
        impl<'de> serde::Deserialize<'de> for Buf {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                struct V;
                impl<'de> serde::de::Visitor<'de> for V {
                    type Value = Buf;
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        f.write_str("bytes")
                    }
                    fn visit_bytes<E>(self, v: &[u8]) -> Result<Buf, E> {
                        Ok(Buf(v.to_vec()))
                    }
                    fn visit_seq<A: serde::de::SeqAccess<'de>>(
                        self,
                        mut a: A,
                    ) -> Result<Buf, A::Error> {
                        let mut o = Vec::new();
                        while let Some(b) = a.next_element::<u8>()? {
                            o.push(b);
                        }
                        Ok(Buf(o))
                    }
                }
                d.deserialize_bytes(V)
            }
        }
    }
}

/// A signed post (a topic opener when `parent` is `None` and `topic_title` set).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Post {
    pub community_id: [u8; 32],
    pub topic_id: [u8; 32],
    /// Reply target (thread), `None` for a top-level post in the topic.
    pub parent: Option<[u8; 32]>,
    pub author: [u8; 32],
    pub created_ms: u64,
    /// UTF-8 text plus inline metadata, ≤ 4 KB.
    pub body: String,
    #[serde(with = "sig64")]
    pub sig: [u8; 64],
}

impl Post {
    fn signed_bytes(&self) -> Vec<u8> {
        let mut b = POST_DOMAIN.to_vec();
        b.extend_from_slice(&self.community_id);
        b.extend_from_slice(&self.topic_id);
        b.extend_from_slice(&self.parent.unwrap_or([0u8; 32]));
        b.push(self.parent.is_some() as u8);
        b.extend_from_slice(&self.author);
        b.extend_from_slice(&self.created_ms.to_be_bytes());
        b.extend_from_slice(self.body.as_bytes());
        b
    }
    pub fn new(
        key: &SigningKey,
        community_id: [u8; 32],
        topic_id: [u8; 32],
        parent: Option<[u8; 32]>,
        body: &str,
        created_ms: u64,
    ) -> Result<Self> {
        if body.len() > MAX_POST_BYTES {
            return err("post over 4 KB");
        }
        let mut p = Self {
            community_id,
            topic_id,
            parent,
            author: key.verifying_key().to_bytes(),
            created_ms,
            body: body.to_string(),
            sig: [0u8; 64],
        };
        p.sig = key.sign(&p.signed_bytes()).to_bytes();
        Ok(p)
    }
    /// 32-byte post id = BLAKE2b-256 of the signed bytes and signature.
    pub fn id(&self) -> [u8; 32] {
        h256(&[&self.signed_bytes(), &self.sig])
    }
    pub fn verify(&self) -> bool {
        self.body.len() <= MAX_POST_BYTES && verify(&self.author, &self.signed_bytes(), &self.sig)
    }
}

/// Signed community control frames (§20.1: no moderation API on relays).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlKind {
    /// Founder-signed genesis.
    Create {
        name: String,
    },
    Join,
    Leave,
    /// Founder only.
    AppointModerator {
        member: [u8; 32],
    },
    /// Founder only.
    DismissModerator {
        member: [u8; 32],
    },
    NewTopic {
        topic_id: [u8; 32],
        title: String,
    },
    /// Moderator or founder; reason required.
    RemovePost {
        post_id: [u8; 32],
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Control {
    pub community_id: [u8; 32],
    pub actor: [u8; 32],
    pub created_ms: u64,
    pub kind: ControlKind,
    #[serde(with = "sig64")]
    pub sig: [u8; 64],
}

impl Control {
    fn signed_bytes(&self) -> Vec<u8> {
        let mut b = CTRL_DOMAIN.to_vec();
        b.extend_from_slice(&self.community_id);
        b.extend_from_slice(&self.actor);
        b.extend_from_slice(&self.created_ms.to_be_bytes());
        ciborium::into_writer(&self.kind, &mut b).expect("cbor to vec");
        b
    }
    pub fn new(
        key: &SigningKey,
        community_id: [u8; 32],
        kind: ControlKind,
        created_ms: u64,
    ) -> Self {
        let mut c = Self {
            community_id,
            actor: key.verifying_key().to_bytes(),
            created_ms,
            kind,
            sig: [0u8; 64],
        };
        c.sig = key.sign(&c.signed_bytes()).to_bytes();
        c
    }
    pub fn verify(&self) -> bool {
        verify(&self.actor, &self.signed_bytes(), &self.sig)
    }
}

/// What travels in a community envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wire {
    Post(Post),
    Control(Control),
}

/// A post as shown to everyone. Removed posts keep only the marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredPost {
    pub post: Option<Post>,
    pub topic_id: [u8; 32],
    pub parent: Option<[u8; 32]>,
    pub created_ms: u64,
    /// Set when a moderator removed it (reason shown to everyone).
    pub removed_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub title: String,
    pub author: [u8; 32],
    pub created_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Community {
    pub id: [u8; 32],
    pub name: String,
    pub founder: [u8; 32],
    pub moderators: BTreeSet<[u8; 32]>,
    pub members: BTreeSet<[u8; 32]>,
    pub topics: BTreeMap<[u8; 32], Topic>,
    pub posts: BTreeMap<[u8; 32], StoredPost>,
}

impl Community {
    pub fn is_moderator(&self, who: &[u8; 32]) -> bool {
        *who == self.founder || self.moderators.contains(who)
    }
    pub fn is_member(&self, who: &[u8; 32]) -> bool {
        self.members.contains(who)
    }
    /// Community Live is members-only.
    pub fn can_join_live(&self, who: &[u8; 32]) -> bool {
        self.is_member(who)
    }
    /// History bytes counted against [`HISTORY_MAX_BYTES`].
    pub fn history_bytes(&self) -> u64 {
        self.posts
            .values()
            .map(|p| {
                p.post
                    .as_ref()
                    .map(|p| p.body.len() as u64 + 200)
                    .unwrap_or(64)
            })
            .sum()
    }
    /// Visible posts of a topic, oldest first. Removed ones carry no content.
    pub fn topic_posts(&self, topic_id: &[u8; 32]) -> Vec<([u8; 32], &StoredPost)> {
        let mut v: Vec<_> = self
            .posts
            .iter()
            .filter(|(_, p)| p.topic_id == *topic_id)
            .map(|(id, p)| (*id, p))
            .collect();
        v.sort_by_key(|(id, p)| (p.created_ms, *id));
        v
    }

    fn erase_post_tree(&mut self, post_id: &[u8; 32]) {
        let mut stack = vec![*post_id];
        while let Some(id) = stack.pop() {
            self.posts.remove(&id);
            stack.extend(
                self.posts
                    .iter()
                    .filter(|(_, p)| p.parent == Some(id))
                    .map(|(k, _)| *k),
            );
        }
    }

    fn erase_topic(&mut self, topic_id: &[u8; 32]) {
        self.topics.remove(topic_id);
        self.posts.retain(|_, p| p.topic_id != *topic_id);
    }

    /// Drop history older than 30 days, then oldest first until ≤ 50 MB.
    pub fn evict(&mut self, now_ms: u64) {
        let cutoff = now_ms.saturating_sub(HISTORY_MAX_AGE_MS);
        self.posts.retain(|_, p| p.created_ms >= cutoff);
        let mut bytes = self.history_bytes();
        if bytes <= HISTORY_MAX_BYTES {
            return;
        }
        let mut order: Vec<_> = self
            .posts
            .iter()
            .map(|(id, p)| (p.created_ms, *id))
            .collect();
        order.sort();
        for (_, id) in order {
            if bytes <= HISTORY_MAX_BYTES {
                break;
            }
            if let Some(p) = self.posts.remove(&id) {
                bytes -= p.post.map(|p| p.body.len() as u64 + 200).unwrap_or(64);
            }
        }
    }
}

/// All communities on this device plus their moderation-aware rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityStore {
    pub communities: BTreeMap<[u8; 32], Community>,
}

/// Encrypted-backup portion for communities: never contains taken-down
/// items, always contains the tombstones (§23.10.2(4), §23.10.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityBackup {
    pub store: CommunityStore,
    pub tombstones: Vec<Tombstone>,
}

impl CommunityStore {
    /// Start a community. The id is 32 random bytes.
    pub fn create(&mut self, founder: &SigningKey, name: &str, now_ms: u64) -> Result<Control> {
        let mut id = [0u8; 32];
        getrandom::getrandom(&mut id).map_err(|_| OmcpError::Sync("rng"))?;
        let c = Control::new(
            founder,
            id,
            ControlKind::Create { name: name.into() },
            now_ms,
        );
        self.apply_control(&c, &ModerationState::default())?;
        Ok(c)
    }

    pub fn get(&self, id: &[u8; 32]) -> Option<&Community> {
        self.communities.get(id)
    }

    /// Apply a signed control frame (local or from the mesh).
    pub fn apply_control(&mut self, c: &Control, m: &ModerationState) -> Result<()> {
        if !c.verify() {
            return err("community control signature");
        }
        if m.is_removed(&c.community_id) {
            return err("community removed by the team");
        }
        if let ControlKind::Create { name } = &c.kind {
            if name.trim().is_empty() || name.len() > MAX_NAME {
                return err("community name");
            }
            if c.community_id != [0u8; 32] && !self.communities.contains_key(&c.community_id) {
                let mut members = BTreeSet::new();
                members.insert(c.actor);
                self.communities.insert(
                    c.community_id,
                    Community {
                        id: c.community_id,
                        name: name.clone(),
                        founder: c.actor,
                        moderators: BTreeSet::new(),
                        members,
                        topics: BTreeMap::new(),
                        posts: BTreeMap::new(),
                    },
                );
            }
            return Ok(());
        }
        let Some(com) = self.communities.get_mut(&c.community_id) else {
            return err("unknown community");
        };
        match &c.kind {
            ControlKind::Create { .. } => unreachable!(),
            ControlKind::Join => {
                if !com.members.contains(&c.actor) && com.members.len() >= MAX_MEMBERS {
                    return err("community is full (5,000 members)");
                }
                com.members.insert(c.actor);
            }
            ControlKind::Leave => {
                if c.actor == com.founder {
                    return err("founder cannot leave");
                }
                com.members.remove(&c.actor);
                com.moderators.remove(&c.actor);
            }
            ControlKind::AppointModerator { member } => {
                if c.actor != com.founder {
                    return err("only the founder appoints moderators");
                }
                if !com.members.contains(member) {
                    return err("moderator must be a member");
                }
                com.moderators.insert(*member);
            }
            ControlKind::DismissModerator { member } => {
                if c.actor != com.founder {
                    return err("only the founder dismisses moderators");
                }
                com.moderators.remove(member);
            }
            ControlKind::NewTopic { topic_id, title } => {
                if !com.is_member(&c.actor) || title.trim().is_empty() || title.len() > MAX_NAME {
                    return err("new topic");
                }
                if m.is_item_removed(&c.community_id, topic_id) {
                    return err("topic removed by the team");
                }
                com.topics.entry(*topic_id).or_insert(Topic {
                    title: title.clone(),
                    author: c.actor,
                    created_ms: c.created_ms,
                });
            }
            ControlKind::RemovePost { post_id, reason } => {
                if !com.is_moderator(&c.actor) {
                    return err("only moderators remove posts");
                }
                if reason.trim().is_empty() || reason.len() > MAX_REASON {
                    return err("a removal needs a reason");
                }
                let Some(p) = com.posts.get_mut(post_id) else {
                    return err("unknown post");
                };
                // Erase the content; everyone (moderators included) sees
                // only "removed" with the reason.
                p.post = None;
                p.removed_reason = Some(reason.clone());
            }
        }
        Ok(())
    }

    /// Accept a signed post (local or from the mesh).
    pub fn add_post(&mut self, p: &Post, m: &ModerationState, now_ms: u64) -> Result<[u8; 32]> {
        if p.body.len() > MAX_POST_BYTES {
            return err("post over 4 KB");
        }
        if !p.verify() {
            return err("post signature");
        }
        if m.is_removed(&p.community_id) || m.is_item_removed(&p.community_id, &p.topic_id) {
            return err("removed by the team");
        }
        let id = p.id();
        if m.is_item_removed(&p.community_id, &id)
            || p.parent
                .map(|x| m.is_item_removed(&p.community_id, &x))
                .unwrap_or(false)
        {
            return err("removed by the team");
        }
        if p.created_ms + HISTORY_MAX_AGE_MS < now_ms {
            return err("post older than the history window");
        }
        let Some(com) = self.communities.get_mut(&p.community_id) else {
            return err("unknown community");
        };
        if !com.is_member(&p.author) {
            return err("only members post");
        }
        if !com.topics.contains_key(&p.topic_id) {
            return err("unknown topic");
        }
        if let Some(par) = p.parent {
            match com.posts.get(&par) {
                Some(pp) if pp.topic_id == p.topic_id => {}
                _ => return err("unknown parent post"),
            }
        }
        com.posts.entry(id).or_insert(StoredPost {
            post: Some(p.clone()),
            topic_id: p.topic_id,
            parent: p.parent,
            created_ms: p.created_ms,
            removed_reason: None,
        });
        com.evict(now_ms);
        Ok(id)
    }

    /// Apply every team takedown in force: full erasure of communities,
    /// topics, posts (with replies). Call after each applied team action and
    /// before writing anything restored from a backup.
    pub fn enforce(&mut self, m: &ModerationState) {
        for t in m.tombstones() {
            match t.kind {
                ActionKind::Takedown => {
                    self.communities.remove(&t.target_id);
                }
                ActionKind::TopicTakedown => {
                    if let Some(c) = self.communities.get_mut(&t.target_id) {
                        c.erase_topic(&t.item_id);
                    }
                }
                ActionKind::PostTakedown => {
                    if let Some(c) = self.communities.get_mut(&t.target_id) {
                        c.erase_post_tree(&t.item_id);
                    }
                }
                _ => {}
            }
        }
    }

    /// Backup portion: enforced store plus tombstones.
    pub fn backup(&self, m: &ModerationState) -> CommunityBackup {
        let mut store = self.clone();
        store.enforce(m);
        CommunityBackup {
            store,
            tombstones: m.tombstones(),
        }
    }

    /// Restore: merge tombstones first (device + backup), erase, then write.
    pub fn restore_backup(b: CommunityBackup, m: &mut ModerationState) -> Self {
        m.merge_tombstones(&b.tombstones);
        let mut s = b.store;
        s.enforce(m);
        s
    }
}

fn envelope_id(body: &[u8]) -> [u8; 16] {
    h256(&[b"OMCP-community-env-v1\x00", body])[..16]
        .try_into()
        .expect("16")
}

/// Wrap a post or control frame in a public mesh envelope (signed, not
/// encrypted: the ciphertext field carries the CBOR frame).
pub fn seal(w: &Wire, community_id: &[u8; 32], now_ms: u64, ttl_hops: u8) -> EnvelopeV1 {
    let mut body = Vec::new();
    ciborium::into_writer(w, &mut body).expect("cbor to vec");
    let mut header_aad = Vec::new();
    put_tag_in_aad(&mut header_aad, &community_tag(community_id));
    put_expires_in_aad(&mut header_aad, now_ms.saturating_add(HISTORY_MAX_AGE_MS));
    EnvelopeV1 {
        version: ENVELOPE_VERSION_V1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: envelope_id(&body),
        flags: ENVELOPE_FLAGS,
        ttl_hops,
        created_ms: now_ms,
        header_aad,
        ciphertext: body,
    }
}

/// Parse a community envelope on `community_id`'s tag.
pub fn open(env: &EnvelopeV1, community_id: &[u8; 32]) -> Result<Wire> {
    if parse_routing_aad(&env.header_aad)?.tag != Some(community_tag(community_id)) {
        return err("not this community's tag");
    }
    if env.envelope_id != envelope_id(&env.ciphertext) || env.ciphertext.len() > MAX_POST_BYTES * 2
    {
        return err("community envelope");
    }
    ciborium::from_reader(env.ciphertext.as_slice()).map_err(|_| OmcpError::Sync("community frame"))
}

//! Community discovery (PROTOCOL §20.8; REQUIREMENTS TM25–TM31).
//!
//! Listed is **off by default** and only the founder can turn it on, after
//! the app shows [`OmcpNode::discovery_listed_notice`]. Search and browse run
//! only on the local index inside the relay engine; nothing about a search
//! is ever sent. Private groups are never announced.

use std::collections::BTreeMap;

use omcp_core::community::discovery::{
    self as d, Announcement, Delist, DiscoveryContext, Listing, ListingFields,
};
use omcp_core::community::{self as oc, Control, ControlKind, Wire};
use serde::{Deserialize, Serialize};

use crate::{invalid, now_ms, parse_id, FfiError, NodeState, OmcpNode};

pub(crate) const LISTED_KEY: &[u8] = b"discovery-listed-v1";
pub(crate) const INDEX_KEY: &[u8] = b"discovery-index-v1";

/// Owner's listing settings for one community (persisted).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ListedRec {
    pub on: bool,
    pub description: String,
    pub category: u8,
    pub tags: Vec<String>,
    pub langs: Vec<String>,
    pub seq: u64,
    pub announced_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DiscoveryState {
    pub listed: BTreeMap<[u8; 32], ListedRec>,
    /// Joins started from discovery: community_id → (community_pk, owner_pk),
    /// waiting for the founder-signed genesis on the community tag.
    pub pending: BTreeMap<[u8; 32], ([u8; 32], [u8; 32])>,
    pub saved_index: Option<d::DiscoveryIndex>,
    /// Genesis arrived: send our Join on the next tick.
    pub ready: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DiscoveryListing {
    pub community_id_hex: String,
    pub community_ref_hex: String,
    pub name: String,
    pub description: String,
    /// Display category 1–8 (unknown codes are 8, Other).
    pub category: u8,
    pub tags: Vec<String>,
    pub langs: Vec<String>,
    /// Member range from the owner's bucket, never a count (TM26).
    pub size_min: u32,
    pub size_max: u32,
    /// Local only: heard directly over Bluetooth/Wi-Fi, ≤ 2 hops, ≤ 7 days.
    pub near_you: bool,
    pub issued_ms: u64,
    pub approval_needed: bool,
    pub joined: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CommunityListingSettings {
    pub listed: bool,
    pub description: String,
    pub category: u8,
    pub tags: Vec<String>,
    pub langs: Vec<String>,
}

impl NodeState {
    pub(crate) fn save_listed(&mut self) -> Result<(), FfiError> {
        self.db.put_meta(LISTED_KEY, &self.mesh.discovery.listed)?;
        Ok(())
    }

    /// Persist the index when it changed; refresh moderation / roster facts.
    pub(crate) fn discovery_tick(&mut self, now: u64) -> Result<(), FfiError> {
        let owners: Vec<([u8; 32], [u8; 32])> = self
            .mesh
            .engine
            .discovery()
            .entries()
            .map(|e| (e.ann.community_id, e.ann.owner_pk))
            .collect();
        let mut ctx = DiscoveryContext::from_moderation(&self.mesh.moderation, owners, now);
        for (id, c) in &self.mesh.open.communities {
            ctx.roster_owner.insert(oc::community_ref(id), c.founder);
        }
        ctx.reported = self.mesh.engine.discovery().context().reported.clone();
        if *self.mesh.engine.discovery().context() != ctx {
            let ev = self.mesh.engine.set_discovery_context(ctx);
            self.route(ev)?;
        }
        let idx = self.mesh.engine.discovery();
        if self.mesh.discovery.saved_index.as_ref() != Some(idx) {
            let idx = idx.clone();
            self.db.put_meta(INDEX_KEY, &idx)?;
            self.mesh.discovery.saved_index = Some(idx);
        }
        Ok(())
    }

    fn publish_listing(&mut self, l: &Listing) -> Result<(), FfiError> {
        let env = d::seal(l, d::OWN_TTL);
        let ev = self
            .mesh
            .engine
            .submit_discovery_own(env, now_ms())
            .map_err(|_| invalid("listing refused"))?;
        self.route(ev)?;
        Ok(())
    }

    /// Pending discovery joins whose genesis this relay already carries.
    pub(crate) fn pending_from_store(&mut self) -> Result<(), FfiError> {
        if self.mesh.discovery.pending.is_empty() {
            return Ok(());
        }
        let ids: Vec<[u8; 32]> = self.mesh.discovery.pending.keys().copied().collect();
        let held: Vec<omcp_core::EnvelopeV1> = self
            .mesh
            .engine
            .item_ids()
            .iter()
            .filter_map(|i| self.mesh.engine.item(i).map(|it| it.envelope.clone()))
            .collect();
        for env in held {
            for id in &ids {
                if let Ok(Wire::Control(c)) = oc::open(&env, id) {
                    if c.community_id == *id && self.on_discovery_genesis(&c) == Some(true) {
                        self.save_open_communities()?;
                        self.mesh.tags_for = None;
                    }
                }
            }
        }
        Ok(())
    }

    /// A Create frame for a community we asked to join from discovery.
    pub(crate) fn on_discovery_genesis(&mut self, c: &Control) -> Option<bool> {
        let (cpk, opk) = *self.mesh.discovery.pending.get(&c.community_id)?;
        if !matches!(c.kind, ControlKind::Create { .. }) || !c.verify() {
            return None;
        }
        self.mesh.discovery.pending.remove(&c.community_id);
        if c.actor != cpk || c.actor != opk {
            // MR62 / item 6: keys don't match the roster: refuse and stop relaying.
            let ev = self
                .mesh
                .engine
                .discovery_distrust(oc::community_ref(&c.community_id));
            let _ = self.route(ev);
            return Some(false);
        }
        let m = self.mesh.moderation.clone();
        if self.mesh.open.apply_control(c, &m).is_err() {
            return Some(false);
        }
        let mut b = Vec::new();
        ciborium::into_writer(c, &mut b).expect("cbor");
        self.mesh.open_genesis.insert(c.community_id, b);
        self.mesh.discovery.ready.push(c.community_id);
        Some(true)
    }
}

fn listing_of(a: &Announcement, near_you: bool, joined: bool) -> DiscoveryListing {
    let (size_min, size_max) = d::bucket_range(a.member_bucket);
    DiscoveryListing {
        community_id_hex: hex::encode(a.community_id),
        community_ref_hex: hex::encode(a.community_ref()),
        name: a.name.clone(),
        description: a.description.clone(),
        category: d::display_category(a.categories.first().copied().unwrap_or(d::CAT_OTHER)),
        tags: a.tags.clone(),
        langs: a.langs.clone(),
        size_min,
        size_max,
        near_you,
        issued_ms: a.issued_ms,
        approval_needed: a.join_policy == 2,
        joined,
    }
}

impl OmcpNode {
    fn announce(&self, id: [u8; 32], now: u64) -> Result<(), FfiError> {
        let (k, me) = self.key_and_me()?;
        let mut st = self.inner.lock();
        let c = st.mesh.open.get(&id).ok_or_else(|| invalid("community"))?;
        if c.founder != me {
            return Err(invalid("only the founder can list"));
        }
        let (name, members) = (c.name.clone(), c.members.len());
        let mut rec = st
            .mesh
            .discovery
            .listed
            .get(&id)
            .cloned()
            .unwrap_or_default();
        rec.seq = rec.seq.max(now) + 1; // strictly increasing across restores
        let a = Announcement::sign(
            id,
            &k,
            &k,
            rec.seq,
            now,
            ListingFields {
                name: name.chars().take(64).collect::<String>(),
                description: rec.description.clone(),
                langs: rec.langs.clone(),
                categories: vec![rec.category.clamp(1, 8)],
                tags: rec.tags.clone(),
                member_bucket: d::bucket_for(members),
                join_policy: 1,
                icon_hash: None,
            },
        )
        .map_err(|_| invalid("listing fields"))?;
        st.publish_listing(&Listing::Announce(a))?;
        rec.announced_ms = now;
        st.mesh.discovery.listed.insert(id, rec);
        st.save_listed()?;
        // Keep the genesis frame on the mesh so discovery joins can complete.
        if let Some(g) = st.mesh.open_genesis.get(&id).cloned() {
            if let Ok(g) = ciborium::from_reader::<Control, _>(g.as_slice()) {
                st.publish_open(&Wire::Control(g), &id)?;
            }
        }
        Ok(())
    }

    /// Item 4(c): re-announce every 7 days while Listed is on.
    pub(crate) fn discovery_maintain(&self, now: u64) -> Result<(), FfiError> {
        let due: Vec<[u8; 32]> = {
            let mut st = self.inner.lock();
            st.discovery_tick(now)?;
            st.pending_from_store()?;
            st.mesh
                .discovery
                .listed
                .iter()
                .filter(|(_, r)| r.on && now.saturating_sub(r.announced_ms) >= d::REFRESH_MS)
                .map(|(id, _)| *id)
                .collect()
        };
        for id in due {
            let _ = self.announce(id, now);
        }
        let ready = std::mem::take(&mut self.inner.lock().mesh.discovery.ready);
        for id in ready {
            self.control(id, ControlKind::Join)?;
        }
        Ok(())
    }
}

#[uniffi::export]
impl OmcpNode {
    /// TM27 notice, shown before Listed is turned on (`comm-listed-notice`).
    pub fn discovery_listed_notice(&self) -> String {
        d::LISTED_NOTICE.to_string()
    }

    /// Listed is off unless the founder turned it on (MR53).
    pub fn open_community_listing(&self, community_id_hex: String) -> CommunityListingSettings {
        let r = parse_id(&community_id_hex)
            .ok()
            .and_then(|id| self.inner.lock().mesh.discovery.listed.get(&id).cloned())
            .unwrap_or_default();
        CommunityListingSettings {
            listed: r.on,
            description: r.description,
            category: if r.category == 0 {
                d::CAT_OTHER
            } else {
                r.category
            },
            tags: r.tags,
            langs: r.langs,
        }
    }

    /// Founder only. Turning on announces; turning off sends a delist.
    pub fn open_community_set_listed(
        &self,
        community_id_hex: String,
        listed: bool,
        description: String,
        category: u8,
        tags: Vec<String>,
        langs: Vec<String>,
    ) -> Result<(), FfiError> {
        let id = parse_id(&community_id_hex)?;
        let (k, me) = self.key_and_me()?;
        let now = now_ms();
        {
            let mut st = self.inner.lock();
            let c = st.mesh.open.get(&id).ok_or_else(|| invalid("community"))?;
            if c.founder != me {
                return Err(invalid("only the founder can list"));
            }
            let description = description.trim().to_string();
            if description.chars().count() > d::MAX_DESC_CHARS {
                return Err(invalid("description over 140 characters"));
            }
            let was_on = st.mesh.discovery.listed.get(&id).is_some_and(|r| r.on);
            let rec = st.mesh.discovery.listed.entry(id).or_default();
            rec.on = listed;
            rec.description = description;
            rec.category = d::display_category(category);
            rec.tags = tags
                .into_iter()
                .map(|t| t.trim().to_lowercase())
                .filter(|t| !t.is_empty())
                .take(d::MAX_TAGS)
                .collect();
            rec.langs = langs.into_iter().take(d::MAX_LANGS).collect();
            if !listed {
                let send = was_on;
                rec.seq = rec.seq.max(now) + 1;
                let seq = rec.seq;
                st.save_listed()?;
                if send {
                    let dl = Delist::sign(&id, &k, &k, seq, now);
                    st.publish_listing(&Listing::Delist(dl))?;
                }
                return Ok(());
            }
            st.save_listed()?;
        }
        self.announce(id, now)
    }

    /// Local search / browse (MR55: nothing here is ever sent).
    pub fn discovery_search(
        &self,
        query: String,
        category: Option<u8>,
        near_only: bool,
    ) -> Vec<DiscoveryListing> {
        let now = now_ms();
        let st = self.inner.lock();
        st.mesh
            .engine
            .discovery()
            .search(&query, category, None, now)
            .into_iter()
            .filter(|f| !near_only || f.near_you)
            .map(|f| {
                let joined = st.mesh.open.communities.contains_key(&f.ann.community_id);
                listing_of(f.ann, f.near_you, joined)
            })
            .collect()
    }

    /// Hide a listing on this phone only.
    pub fn discovery_hide(&self, community_ref_hex: String) -> Result<(), FfiError> {
        let r = parse_id(&community_ref_hex)?;
        self.inner.lock().mesh.engine.discovery_hide(r, true);
        Ok(())
    }

    /// MR62: "Report community" from a card: only the signed announcement
    /// bytes, for reason codes 1–7, 9 and 10. Also hides it here.
    pub fn discovery_report(
        &self,
        community_ref_hex: String,
        reason_code: u8,
    ) -> Result<Vec<u8>, FfiError> {
        if !matches!(reason_code, 1..=7 | 9 | 10) {
            return Err(invalid("reason"));
        }
        let r = parse_id(&community_ref_hex)?;
        let mut st = self.inner.lock();
        let bytes = st
            .mesh
            .engine
            .discovery()
            .get(&r)
            .map(|e| e.ann.encode())
            .ok_or_else(|| invalid("listing"))?;
        st.mesh.engine.discovery_hide(r, true);
        Ok(bytes)
    }

    /// Join a listed community. Returns true when joined now, false while
    /// waiting for the community's genesis frame from the mesh.
    pub fn discovery_join(&self, community_ref_hex: String) -> Result<bool, FfiError> {
        let r = parse_id(&community_ref_hex)?;
        {
            let mut st = self.inner.lock();
            let e = st
                .mesh
                .engine
                .discovery()
                .get(&r)
                .ok_or_else(|| invalid("listing"))?;
            let (id, keys) = (e.ann.community_id, (e.ann.community_pk, e.ann.owner_pk));
            if st.mesh.open.communities.contains_key(&id) {
                return Ok(true);
            }
            st.mesh.discovery.pending.insert(id, keys);
            st.mesh.tags_for = None; // want the community tag now
            st.pending_from_store()?;
        }
        let ready = std::mem::take(&mut self.inner.lock().mesh.discovery.ready);
        let joined = !ready.is_empty();
        for id in ready {
            self.control(id, ControlKind::Join)?;
        }
        Ok(joined)
    }
}

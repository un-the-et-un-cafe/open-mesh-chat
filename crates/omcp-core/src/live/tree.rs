//! Viewer tree for online video (no media server).
//!
//! The host sends to at most [`HOST_FANOUT`] direct viewers; each viewer
//! forwards to at most [`VIEWER_FANOUT`]. A viewer on a weak link joins as a
//! leaf (`can_forward = false`). New viewers attach to the shallowest parent
//! with a free slot (fewest children first, then lowest id, so every peer
//! computes the same tree). When a viewer leaves, each of its children is
//! re-parented with its own subtree intact; a child that finds no slot is
//! parked (audio over the mesh) and retried on the next change.

use std::collections::{BTreeMap, BTreeSet};

use super::{HOST_FANOUT, VIEWER_FANOUT};

pub type PeerId = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    parent: Option<PeerId>,
    children: BTreeSet<PeerId>,
    can_forward: bool,
}

/// One re-parenting instruction for the apps (tear down / open WebRTC links).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reparent {
    pub viewer: PeerId,
    /// `None` = parked: no slot, audio-only until one frees up.
    pub new_parent: Option<PeerId>,
}

#[derive(Debug, Clone)]
pub struct ViewerTree {
    host: PeerId,
    host_fanout: usize,
    viewer_fanout: usize,
    nodes: BTreeMap<PeerId, Node>,
    parked: BTreeSet<PeerId>,
}

impl ViewerTree {
    pub fn new(host: PeerId) -> Self {
        Self::with_fanout(host, HOST_FANOUT, VIEWER_FANOUT)
    }

    pub fn with_fanout(host: PeerId, host_fanout: usize, viewer_fanout: usize) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            host,
            Node {
                parent: None,
                children: BTreeSet::new(),
                can_forward: true,
            },
        );
        Self {
            host,
            host_fanout,
            viewer_fanout,
            nodes,
            parked: BTreeSet::new(),
        }
    }

    pub fn host(&self) -> PeerId {
        self.host
    }

    fn cap(&self, id: &PeerId) -> usize {
        match self.nodes.get(id) {
            Some(_) if *id == self.host => self.host_fanout,
            Some(n) if n.can_forward => self.viewer_fanout,
            _ => 0,
        }
    }

    pub fn depth(&self, id: &PeerId) -> Option<usize> {
        let mut d = 0;
        let mut cur = *id;
        loop {
            let n = self.nodes.get(&cur)?;
            match n.parent {
                None => return (cur == self.host).then_some(d),
                Some(p) => {
                    d += 1;
                    cur = p;
                    if d > self.nodes.len() {
                        return None;
                    }
                }
            }
        }
    }

    fn subtree(&self, id: &PeerId) -> BTreeSet<PeerId> {
        let mut s = BTreeSet::new();
        let mut stack = vec![*id];
        while let Some(x) = stack.pop() {
            if s.insert(x) {
                if let Some(n) = self.nodes.get(&x) {
                    stack.extend(n.children.iter().copied());
                }
            }
        }
        s
    }

    fn best_parent(&self, exclude: &BTreeSet<PeerId>) -> Option<PeerId> {
        self.nodes
            .iter()
            .filter(|(id, n)| {
                !exclude.contains(*id)
                    && !self.parked.contains(*id)
                    && n.children.len() < self.cap(id)
                    && self.depth(id).is_some()
            })
            .min_by_key(|(id, n)| (self.depth(id).unwrap_or(usize::MAX), n.children.len(), **id))
            .map(|(id, _)| *id)
    }

    fn attach(&mut self, id: PeerId, parent: PeerId) {
        if let Some(n) = self.nodes.get_mut(&id) {
            n.parent = Some(parent);
        }
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.children.insert(id);
        }
        self.parked.remove(&id);
    }

    /// Add a viewer; returns its parent (`None` = parked, no free slot).
    pub fn join(&mut self, id: PeerId, can_forward: bool) -> Option<PeerId> {
        if id == self.host {
            return None;
        }
        if let Some(p) = self.parent(&id) {
            return Some(p);
        }
        self.nodes.insert(
            id,
            Node {
                parent: None,
                children: BTreeSet::new(),
                can_forward,
            },
        );
        let excl = BTreeSet::from([id]);
        match self.best_parent(&excl) {
            Some(p) => {
                self.attach(id, p);
                Some(p)
            }
            None => {
                self.parked.insert(id);
                None
            }
        }
    }

    /// Remove a viewer; its children (with their subtrees) are re-parented.
    /// Also retries parked viewers. The host leaving ends the tree: use
    /// [`Self::host_left`].
    pub fn leave(&mut self, id: &PeerId) -> Vec<Reparent> {
        if *id == self.host {
            return Vec::new();
        }
        let Some(n) = self.nodes.remove(id) else {
            self.parked.remove(id);
            return Vec::new();
        };
        self.parked.remove(id);
        if let Some(p) = n.parent {
            if let Some(pn) = self.nodes.get_mut(&p) {
                pn.children.remove(id);
            }
        }
        let mut out = Vec::new();
        let mut orphans: Vec<PeerId> = n.children.into_iter().collect();
        for o in &orphans {
            if let Some(on) = self.nodes.get_mut(o) {
                on.parent = None;
            }
        }
        // Retry parked viewers too (a slot may have opened).
        let parked: Vec<PeerId> = self
            .parked
            .iter()
            .copied()
            .filter(|p| !orphans.contains(p))
            .collect();
        orphans.extend(parked);
        for o in orphans {
            let excl = self.subtree(&o);
            match self.best_parent(&excl) {
                Some(p) => {
                    self.attach(o, p);
                    out.push(Reparent {
                        viewer: o,
                        new_parent: Some(p),
                    });
                }
                None => {
                    if self.parked.insert(o) {
                        out.push(Reparent {
                            viewer: o,
                            new_parent: None,
                        });
                    }
                }
            }
        }
        out
    }

    /// Host lost internet: every viewer falls back to mesh audio (LV3).
    pub fn host_left(&mut self) -> Vec<PeerId> {
        let v: Vec<PeerId> = self
            .nodes
            .keys()
            .copied()
            .filter(|k| *k != self.host)
            .collect();
        self.nodes.retain(|k, _| *k == self.host);
        if let Some(h) = self.nodes.get_mut(&self.host) {
            h.children.clear();
        }
        self.parked.clear();
        v
    }

    pub fn parent(&self, id: &PeerId) -> Option<PeerId> {
        self.nodes.get(id)?.parent
    }

    pub fn children(&self, id: &PeerId) -> Vec<PeerId> {
        self.nodes
            .get(id)
            .map(|n| n.children.iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn is_parked(&self, id: &PeerId) -> bool {
        self.parked.contains(id)
    }

    pub fn viewers(&self) -> usize {
        self.nodes.len() - 1
    }

    /// Invariants: fan-out limits, acyclic, every placed viewer reaches the host.
    pub fn check(&self) -> bool {
        self.nodes.iter().all(|(id, n)| {
            n.children.len() <= self.cap(id)
                && n.children
                    .iter()
                    .all(|c| self.nodes.get(c).and_then(|x| x.parent) == Some(*id))
                && (self.parked.contains(id) || self.depth(id).is_some())
        })
    }
}

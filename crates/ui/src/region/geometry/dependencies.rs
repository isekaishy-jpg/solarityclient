//! Retained reverse parent/anchor links for local geometry invalidation.

use crate::UiLayoutError;
use crate::script::UiRuntimeObjectPlan;

use super::resolution_error;

/// No link in the indexed adjacency lists or free-slot chain.
const NONE: usize = usize::MAX;

/// One dependency-to-dependent link; backlinks allow constant-time retirement.
#[derive(Clone)]
struct Edge {
    dependency: usize,
    dependent: usize,
    previous: usize,
    next: usize,
}

/// Geometry generations retain topology; queries touch only reachable nodes.
#[derive(Clone)]
pub(super) struct RegionDependencies {
    heads: Vec<usize>,
    owned: Vec<Vec<usize>>,
    edges: Vec<Edge>,
    free: Vec<usize>,
    marked: Vec<bool>,
    work: Vec<usize>,
}

/// Validated replacements are published only after geometry resolution succeeds.
pub(super) struct DependencyChanges(Vec<(usize, Vec<usize>)>);

impl RegionDependencies {
    /// Builds the index once with the initial complete geometry generation.
    pub(super) fn new(live: &UiRuntimeObjectPlan) -> Result<Self, UiLayoutError> {
        let count = live.objects().len();
        let mut index = Self {
            heads: vec![NONE; count],
            owned: vec![Vec::new(); count],
            edges: Vec::new(),
            free: Vec::new(),
            marked: vec![false; count],
            work: Vec::new(),
        };
        let changes = index.stage(live, &(0..count).collect::<Vec<_>>())?;
        index.publish(changes);
        Ok(index)
    }

    /// Validates every changed dependency owner before touching retained links.
    pub(super) fn stage(
        &self,
        live: &UiRuntimeObjectPlan,
        roots: &[usize],
    ) -> Result<DependencyChanges, UiLayoutError> {
        let mut changes = Vec::new();
        for &root in roots {
            let object = live.objects().get(root).ok_or_else(|| {
                resolution_error(format!("geometry refresh root {root} is outside the arena"))
            })?;
            let mut parents = object
                .parent
                .into_iter()
                .chain(
                    live.anchors_for(object)
                        .iter()
                        .filter_map(|anchor| anchor.target),
                )
                .collect::<Vec<_>>();
            if parents.iter().any(|&parent| parent >= self.heads.len()) {
                return Err(resolution_error("geometry dependency is outside the arena"));
            }
            parents.sort_unstable();
            parents.dedup();
            // Owned edge order is kept sorted by dependency at publication.
            if !parents.iter().copied().eq(self.owned[root]
                .iter()
                .map(|&edge| self.edges[edge].dependency))
            {
                changes.push((root, parents));
            }
        }
        Ok(DependencyChanges(changes))
    }

    /// Uses the preceding graph: every owner whose outgoing dependencies changed
    /// is itself a root, so changed edges cannot add an unvisited root. This
    /// keeps graph and geometry publication atomic even on a cycle/error.
    pub(super) fn affected(&mut self, roots: &[usize]) -> Vec<usize> {
        for &index in &self.work {
            self.marked[index] = false;
        }
        self.work.clear();
        for &root in roots {
            self.enqueue(root);
        }
        let mut cursor = 0;
        while cursor < self.work.len() {
            let mut edge = self.heads[self.work[cursor]];
            while edge != NONE {
                self.enqueue(self.edges[edge].dependent);
                edge = self.edges[edge].next;
            }
            cursor += 1;
        }
        self.work.sort_unstable();
        self.work.clone()
    }

    /// Deduplicates visitation without clearing an arena-sized mask per query.
    fn enqueue(&mut self, index: usize) {
        if !self.marked[index] {
            self.marked[index] = true;
            self.work.push(index);
        }
    }

    /// Replaces only changed root links, reusing slots and per-owner capacity.
    pub(super) fn publish(&mut self, changes: DependencyChanges) {
        for (dependent, parents) in changes.0 {
            for edge in self.owned[dependent].drain(..) {
                let link = &self.edges[edge];
                let (dependency, previous, next) = (link.dependency, link.previous, link.next);
                if previous == NONE {
                    self.heads[dependency] = next;
                } else {
                    self.edges[previous].next = next;
                }
                if next != NONE {
                    self.edges[next].previous = previous;
                }
                self.free.push(edge);
            }
            for dependency in parents {
                let next = self.heads[dependency];
                let link = Edge {
                    dependency,
                    dependent,
                    previous: NONE,
                    next,
                };
                let edge = if let Some(edge) = self.free.pop() {
                    self.edges[edge] = link;
                    edge
                } else {
                    self.edges.push(link);
                    self.edges.len() - 1
                };
                if next != NONE {
                    self.edges[next].previous = edge;
                }
                self.heads[dependency] = edge;
                self.owned[dependent].push(edge);
            }
        }
    }
}

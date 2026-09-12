//! Conservative root candidates with native append-order publication.

use super::{MovementRootReference, ResidentTerrainMap, RuntimeMovementRegistrationError};
use glam::Vec3;

/// A balanced box hierarchy is rebuilt only when a root or terrain generation changes.
#[derive(Default)]
pub(super) struct RootIndex {
    revision: Option<u64>,
    nodes: Vec<Node>,
}

/// Internal nodes contain two descendants; leaves preserve the native root ordinal.
struct Node {
    bounds: [Vec3; 2],
    kind: NodeKind,
}
enum NodeKind {
    Root(usize),
    Branch([usize; 2]),
}

impl RootIndex {
    /// Median partition bounds construction depth independently of placement order.
    fn build(entries: &mut [(usize, [Vec3; 2])], nodes: &mut Vec<Node>) -> usize {
        let bounds = entries.iter().fold(
            [Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)],
            |[minimum, maximum], (_, bounds)| [minimum.min(bounds[0]), maximum.max(bounds[1])],
        );
        let index = nodes.len();
        nodes.push(Node {
            bounds,
            kind: NodeKind::Root(entries[0].0),
        });
        if entries.len() > 1 {
            let extent = bounds[1] - bounds[0];
            let axis = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let middle = entries.len() / 2;
            entries.select_nth_unstable_by(middle, |left, right| {
                (f64::from(left.1[0][axis]) + f64::from(left.1[1][axis]))
                    .total_cmp(&(f64::from(right.1[0][axis]) + f64::from(right.1[1][axis])))
            });
            let (left, right) = entries.split_at_mut(middle);
            nodes[index].kind =
                NodeKind::Branch([Self::build(left, nodes), Self::build(right, nodes)]);
        }
        index
    }

    /// Traversal order is deliberately erased; callers receive native ordinals.
    fn collect(&self, index: usize, bounds: [Vec3; 2], output: &mut Vec<usize>) {
        let node = &self.nodes[index];
        if (0..3).any(|axis| {
            bounds[1][axis] < node.bounds[0][axis] || bounds[0][axis] > node.bounds[1][axis]
        }) {
            return;
        }
        match node.kind {
            NodeKind::Root(root) => output.push(root),
            NodeKind::Branch(children) => {
                for child in children {
                    self.collect(child, bounds, output);
                }
            }
        }
    }
}

impl ResidentTerrainMap {
    /// Both 7C25D0's segment gate and 7A09D0's point gate require root-box overlap.
    /// The exact native narrow phase still owns tolerances and root precedence.
    pub(super) fn root_candidates(
        &mut self,
        start: Vec3,
        end: Vec3,
    ) -> Result<Vec<MovementRootReference>, RuntimeMovementRegistrationError> {
        if self.movement.roots.is_empty() {
            return Ok(Vec::new());
        }
        if !start.is_finite() || !end.is_finite() {
            return Err(solarity_systems::WorldModelCollisionError::NonFiniteSegment.into());
        }
        if self.movement.root_index.revision != Some(self.movement.lighting_revision) {
            let mut entries = Vec::with_capacity(self.movement.roots.len());
            for index in 0..self.movement.roots.len() {
                let root = self.movement.roots[index];
                entries.push((index, self.registration_root_mut(root)?.root_bounds()));
            }
            let mut nodes = Vec::with_capacity(entries.len() * 2 - 1);
            RootIndex::build(&mut entries, &mut nodes);
            self.movement.root_index = RootIndex {
                revision: Some(self.movement.lighting_revision),
                nodes,
            };
        }
        let mut candidates = Vec::new();
        self.movement
            .root_index
            .collect(0, [start.min(end), start.max(end)], &mut candidates);
        candidates.sort_unstable();
        Ok(candidates
            .into_iter()
            .map(|index| self.movement.roots[index])
            .collect())
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/root_index.rs"]
mod tests;

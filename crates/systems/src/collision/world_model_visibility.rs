//! Native 7AC060 traversal after each portal's polygon has been projected.

use glam::Vec3;
use solarity_asset::DecodedWorldModel;
use thiserror::Error;

/// Invalid input to the retained WMO portal visibility traversal.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum WorldModelVisibilityError {
    /// A camera group is outside the admitted WMO generation.
    #[error("world-model visibility group is outside the admitted model")]
    InvalidGroup,
    /// The projection array must contain one entry per authored portal.
    #[error("world-model visibility portal projection count does not match the model")]
    PortalCount,
    /// Camera coordinates or projected screen coordinates are not finite.
    #[error("world-model visibility coordinates are not finite")]
    NonFiniteCoordinates,
}

/// One ordered native group visit, including the inherited fog bank.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelVisibilityVisit {
    /// Authored group index.
    pub group: usize,
    /// True selects the indoor model fog bank.
    pub indoor_fog: bool,
    /// Number of traversed portals from this query's initial group.
    pub depth: u32,
    /// Native min-Y, min-X, max-Y, max-X projection bounds for this visit.
    pub screen_window: [f32; 4],
}

#[derive(Clone, Copy)]
struct Pending {
    visit: WorldModelVisibilityVisit,
    parent: Option<usize>,
}

/// Reusable ordered portal traversal storage, independent of GPU resources.
#[derive(Default)]
pub struct WorldModelVisibilityQuery {
    pending: Vec<Pending>,
    visits: Vec<WorldModelVisibilityVisit>,
}

impl WorldModelVisibilityQuery {
    /// Traverses one initial group using already projected portal windows.
    ///
    /// Positions are in root-local coordinates. A missing window denotes a
    /// rejected portal; a near-plane spanning portal supplies the full window.
    /// Groups can occur more than once through different portal paths. The
    /// caller chooses the native recursion limit and merges all initial groups.
    ///
    /// # Errors
    /// Rejects invalid group indices, projection counts or nonfinite values.
    pub fn query(
        &mut self,
        model: &DecodedWorldModel,
        position: Vec3,
        group: usize,
        maximum_depth: u32,
        indoor_fog: bool,
        projected_portals: &[Option<[f32; 4]>],
    ) -> Result<&[WorldModelVisibilityVisit], WorldModelVisibilityError> {
        self.pending.clear();
        self.visits.clear();
        if group >= model.groups().len() {
            return Err(WorldModelVisibilityError::InvalidGroup);
        }
        if projected_portals.len() != model.portals().len() {
            return Err(WorldModelVisibilityError::PortalCount);
        }
        if !position.is_finite()
            || projected_portals
                .iter()
                .flatten()
                .flatten()
                .any(|v| !v.is_finite())
        {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        self.pending.push(Pending {
            visit: WorldModelVisibilityVisit {
                group,
                indoor_fog,
                depth: 0,
                screen_window: [-1., -1., 1., 1.],
            },
            parent: None,
        });
        while let Some(Pending { mut visit, parent }) = self.pending.pop() {
            let group = &model.groups()[visit.group];
            if group.flags() & 0x10000 != 0 {
                continue;
            }
            visit.indoor_fog &= group.flags() & 0x48 == 0;
            self.visits.push(visit);
            if visit.depth == maximum_depth {
                continue;
            }
            let first = usize::from(group.portal_reference_start());
            let count = usize::from(group.portal_reference_count());
            // Reverse pushes reproduce recursive authored-reference order.
            for reference in model.portal_references()[first..first + count].iter().rev() {
                let adjacent = usize::from(reference.group_index());
                if Some(adjacent) == parent || model.group_info()[adjacent].flags() & 0x10008 != 0 {
                    continue;
                }
                let portal_index = usize::from(reference.portal_index());
                let portal = model.portals()[portal_index];
                let Some(bounds) = projected_portals[portal_index] else {
                    continue;
                };
                let [x, y, z] = portal.normal().map(f64::from);
                let p = position.as_dvec3();
                let mut side = ((y * p.y + z * p.z) + x * p.x) + f64::from(portal.distance());
                if reference.side() < 0 {
                    side = -side;
                }
                let parent_bounds = visit.screen_window;
                if side < 0.
                    || bounds[1] > parent_bounds[3]
                    || parent_bounds[1] > bounds[3]
                    || bounds[0] > parent_bounds[2]
                    || parent_bounds[0] > bounds[2]
                {
                    continue;
                }
                // The pinned code clamps both minima and maximum X, and
                // retains the child's maximum Y even beyond the parent edge.
                let window = [
                    bounds[0].max(parent_bounds[0]),
                    bounds[1].max(parent_bounds[1]),
                    bounds[2],
                    bounds[3].min(parent_bounds[3]),
                ];
                let narrow =
                    |a: f32, b: f32| (f64::from(a) - f64::from(b)).abs() < f64::from(0.001_f32);
                if narrow(window[0], window[2]) || narrow(window[1], window[3]) {
                    continue;
                }
                self.pending.push(Pending {
                    visit: WorldModelVisibilityVisit {
                        group: adjacent,
                        indoor_fog: visit.indoor_fog,
                        depth: visit.depth + 1,
                        screen_window: window,
                    },
                    parent: Some(visit.group),
                });
            }
        }
        Ok(&self.visits)
    }
}

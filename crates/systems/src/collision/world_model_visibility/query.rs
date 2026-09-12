//! Retained recursive group visits and once-per-root exterior portal encounters.

use super::{
    WorldModelPortalProjectionFrame, WorldModelPortalProjector, WorldModelSceneVisibilityEvent,
    WorldModelVisibilityError, WorldModelVisibilityVisit,
};
use glam::Vec3;
use solarity_asset::DecodedWorldModel;

/// Deferred recursive calls preserve authored order without the native stack.
#[derive(Clone, Copy)]
enum Pending {
    Group {
        visit: WorldModelVisibilityVisit,
        parent: Option<usize>,
    },
    ExteriorPortal {
        reference: usize,
    },
}

/// Selects whether the consumer needs the exterior callback stream as well.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TraversalOutput {
    Groups,
    Scene,
}

/// Reusable ordered portal traversal storage, independent of GPU resources.
#[derive(Default)]
pub struct WorldModelVisibilityQuery {
    pending: Vec<Pending>,
    visits: Vec<WorldModelVisibilityVisit>,
    scene_events: Vec<WorldModelSceneVisibilityEvent>,
    exterior_encountered: Vec<bool>,
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
        self.clear();
        if group >= model.groups().len() {
            return Err(WorldModelVisibilityError::InvalidGroup);
        }
        Self::validate(model, position, projected_portals)?;
        self.push_initial(group, indoor_fog, [-1., -1., 1., 1.]);
        self.traverse(
            model,
            position,
            maximum_depth,
            |index| Ok(projected_portals[index]),
            TraversalOutput::Groups,
        )?;
        Ok(&self.visits)
    }

    /// Traverses 7AD350's outdoor entry with the inherited screen window.
    ///
    /// The initial window uses clip-space min-Y, min-X, max-Y, max-X order.
    /// Fog starts in the outdoor bank, and CFBEC0 is zero: this traversal visits
    /// groups without generating camera-root exterior portal events. Optional
    /// occluder construction is disabled at this boundary. The caller first
    /// applies 7B3A10's group-info flag and cropped world-bounds checks.
    ///
    /// # Errors
    /// Rejects invalid groups, projection counts, nonfinite values or an
    /// unordered initial screen window.
    pub fn query_outdoor(
        &mut self,
        model: &DecodedWorldModel,
        position: Vec3,
        group: usize,
        maximum_depth: u32,
        screen_window: [f32; 4],
        projected_portals: &[Option<[f32; 4]>],
    ) -> Result<&[WorldModelVisibilityVisit], WorldModelVisibilityError> {
        self.clear();
        if group >= model.groups().len() {
            return Err(WorldModelVisibilityError::InvalidGroup);
        }
        Self::validate(model, position, projected_portals)?;
        if !screen_window.into_iter().all(f32::is_finite) {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        if screen_window[0] >= screen_window[2] || screen_window[1] >= screen_window[3] {
            return Err(WorldModelVisibilityError::DegenerateFrustum);
        }
        self.push_initial(group, false, screen_window);
        self.traverse(
            model,
            position,
            maximum_depth,
            |index| Ok(projected_portals[index]),
            TraversalOutput::Groups,
        )?;
        Ok(&self.visits)
    }

    /// Traverses 7AD1F0's ordered camera groups, retaining 7AC060's scene events.
    ///
    /// Each root begins with indoor fog and the full screen window. The native
    /// portal cache spans all initial groups in that root. This method covers
    /// the recursive portal pass; the caller separately tests flag-0x10000 group
    /// bounds for 7AD1F0's final direct callback pass. The pinned default maximum
    /// depth is 10, copied by 9CE7E0 from ADFE40 into D1BEE4.
    ///
    /// # Errors
    /// Rejects invalid initial groups, projection counts or nonfinite values.
    pub fn query_scene(
        &mut self,
        model: &DecodedWorldModel,
        position: Vec3,
        initial_groups: &[usize],
        maximum_depth: u32,
        projected_portals: &[Option<[f32; 4]>],
    ) -> Result<&[WorldModelSceneVisibilityEvent], WorldModelVisibilityError> {
        self.clear();
        if initial_groups
            .iter()
            .any(|&group| group >= model.groups().len())
        {
            return Err(WorldModelVisibilityError::InvalidGroup);
        }
        Self::validate(model, position, projected_portals)?;
        self.exterior_encountered
            .resize(model.portals().len(), false);
        for &group in initial_groups.iter().rev() {
            self.push_initial(group, true, [-1., -1., 1., 1.]);
        }
        self.traverse(
            model,
            position,
            maximum_depth,
            |index| Ok(projected_portals[index]),
            TraversalOutput::Scene,
        )?;
        Ok(&self.scene_events)
    }

    /// Runtime camera roots project only portals reached by native recursion.
    pub(super) fn query_scene_projecting(
        &mut self,
        model: &DecodedWorldModel,
        frame: WorldModelPortalProjectionFrame,
        initial_groups: &[usize],
        maximum_depth: u32,
        projector: &mut WorldModelPortalProjector,
    ) -> Result<&[WorldModelSceneVisibilityEvent], WorldModelVisibilityError> {
        self.clear();
        if initial_groups
            .iter()
            .any(|&group| group >= model.groups().len())
        {
            return Err(WorldModelVisibilityError::InvalidGroup);
        }
        projector.begin_scene(model, frame)?;
        self.exterior_encountered
            .resize(model.portals().len(), false);
        for &group in initial_groups.iter().rev() {
            self.push_initial(group, true, [-1., -1., 1., 1.]);
        }
        self.traverse(
            model,
            frame.local_camera,
            maximum_depth,
            |index| projector.scene_portal(model, frame, index),
            TraversalOutput::Scene,
        )?;
        Ok(&self.scene_events)
    }

    /// Outdoor entries keep independent windows and visitation while sharing
    /// projection only across repeated references within this one query.
    pub(super) fn query_outdoor_projecting(
        &mut self,
        model: &DecodedWorldModel,
        frame: WorldModelPortalProjectionFrame,
        group: usize,
        maximum_depth: u32,
        screen_window: [f32; 4],
        projector: &mut WorldModelPortalProjector,
    ) -> Result<&[WorldModelVisibilityVisit], WorldModelVisibilityError> {
        self.clear();
        if group >= model.groups().len() {
            return Err(WorldModelVisibilityError::InvalidGroup);
        }
        projector.begin_scene(model, frame)?;
        if !screen_window.into_iter().all(f32::is_finite) {
            return Err(WorldModelVisibilityError::NonFiniteCoordinates);
        }
        if screen_window[0] >= screen_window[2] || screen_window[1] >= screen_window[3] {
            return Err(WorldModelVisibilityError::DegenerateFrustum);
        }
        self.push_initial(group, false, screen_window);
        self.traverse(
            model,
            frame.local_camera,
            maximum_depth,
            |index| projector.scene_portal(model, frame, index),
            TraversalOutput::Groups,
        )?;
        Ok(&self.visits)
    }

    /// Resets per-root results while retaining their allocations.
    fn clear(&mut self) {
        self.pending.clear();
        self.visits.clear();
        self.scene_events.clear();
        self.exterior_encountered.clear();
    }

    /// Rejects malformed shared query inputs before exposing any visits.
    fn validate(
        model: &DecodedWorldModel,
        position: Vec3,
        projected_portals: &[Option<[f32; 4]>],
    ) -> Result<(), WorldModelVisibilityError> {
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
        Ok(())
    }

    /// Starts a camera group or outdoor entry with its caller-selected window.
    fn push_initial(&mut self, group: usize, indoor_fog: bool, screen_window: [f32; 4]) {
        self.pending.push(Pending::Group {
            visit: WorldModelVisibilityVisit {
                group,
                indoor_fog,
                depth: 0,
                screen_window,
            },
            parent: None,
        });
    }

    /// Runs the common group recursion and optional first-portal scene events.
    fn traverse(
        &mut self,
        model: &DecodedWorldModel,
        position: Vec3,
        maximum_depth: u32,
        mut project: impl FnMut(usize) -> Result<Option<[f32; 4]>, WorldModelVisibilityError>,
        output: TraversalOutput,
    ) -> Result<(), WorldModelVisibilityError> {
        let scene = output == TraversalOutput::Scene;
        while let Some(pending) = self.pending.pop() {
            let (mut visit, parent) = match pending {
                Pending::Group { visit, parent } => (visit, parent),
                Pending::ExteriorPortal { reference } => {
                    let portal = usize::from(model.portal_references()[reference].portal_index());
                    if !self.exterior_encountered[portal] {
                        self.exterior_encountered[portal] = true;
                        self.scene_events
                            .push(WorldModelSceneVisibilityEvent::ExteriorPortal { reference });
                    }
                    continue;
                }
            };
            let group = &model.groups()[visit.group];
            if group.flags() & 0x10000 != 0 {
                continue;
            }
            visit.indoor_fog &= group.flags() & 0x48 == 0;
            self.visits.push(visit);
            if scene {
                self.scene_events
                    .push(WorldModelSceneVisibilityEvent::Group(visit));
            }
            if visit.depth == maximum_depth && !scene {
                continue;
            }
            let first = usize::from(group.portal_reference_start());
            let count = usize::from(group.portal_reference_count());
            // Reverse pushes reproduce recursive authored-reference order.
            for reference_index in (first..first + count).rev() {
                let reference = &model.portal_references()[reference_index];
                let adjacent = usize::from(reference.group_index());
                let adjacent_flags = model.group_info()[adjacent].flags();
                if Some(adjacent) == parent {
                    continue;
                }
                let portal_index = usize::from(reference.portal_index());
                let portal = model.portals()[portal_index];
                let Some(bounds) = project(portal_index)? else {
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
                if adjacent_flags & 0x10008 == 0 && visit.depth < maximum_depth {
                    self.pending.push(Pending::Group {
                        visit: WorldModelVisibilityVisit {
                            group: adjacent,
                            indoor_fog: visit.indoor_fog,
                            depth: visit.depth + 1,
                            screen_window: window,
                        },
                        parent: Some(visit.group),
                    });
                }
                // The exterior callback precedes recursion, including at the
                // maximum group depth, where the child call cannot visit.
                if scene && adjacent_flags & 0x50148 != 0 {
                    self.pending.push(Pending::ExteriorPortal {
                        reference: reference_index,
                    });
                }
            }
        }
        Ok(())
    }
}

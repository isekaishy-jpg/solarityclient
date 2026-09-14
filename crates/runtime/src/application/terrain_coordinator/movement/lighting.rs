//! Dependency revisions for retained native floor and liquid registrations.

use std::collections::VecDeque;

use glam::{Mat4, Vec3};
use solarity_systems::MovementCollisionBounds;

/// Unit probes use world bounds; native box registration uses local AABBs.
struct RootMotion {
    bounds: [Vec3; 2],
    registration: Option<RootRegistrationMotion>,
}

/// Retains both root placements so departures invalidate old destinations.
struct RootRegistrationMotion {
    inverse_transforms: [Mat4; 2],
    local_bounds: [Vec3; 2],
}

/// Motion invalidates only unit probes crossing the old or new root box. Root
/// membership and terrain publication invalidate every registration. The bounded
/// history is an optimization: older consumers simply run the original query.
#[derive(Default)]
pub(super) struct LightingChanges {
    revision: u64,
    topology_revision: u64,
    motions: VecDeque<RootMotion>,
}

impl LightingChanges {
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn topology_revision(&self) -> u64 {
        self.topology_revision
    }

    /// A source, destination list, or terrain generation can change any result.
    pub(super) fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.topology_revision = self.topology_revision.wrapping_add(1);
        self.motions.clear();
    }

    /// Includes departure and arrival, so leaving a cached floor invalidates it
    /// just as entering a previously empty registration does.
    pub(super) fn moved(&mut self, previous: [Vec3; 2], current: [Vec3; 2]) {
        self.revision = self.revision.wrapping_add(1);
        if self.motions.len() == 64 {
            self.motions.pop_front();
        }
        self.motions.push_back(RootMotion {
            bounds: [previous[0].min(current[0]), previous[1].max(current[1])],
            registration: None,
        });
    }

    /// Saves the exact matrices used by 7C2BF0's inverse-transformed box test.
    pub(super) fn moved_map_root(
        &mut self,
        previous: [Vec3; 2],
        current: [Vec3; 2],
        inverse_transforms: [Mat4; 2],
        local_bounds: [Vec3; 2],
    ) {
        self.moved(previous, current);
        if let Some(motion) = self.motions.back_mut() {
            motion.registration = Some(RootRegistrationMotion {
                inverse_transforms,
                local_bounds,
            });
        }
    }

    /// Unit_C's 7C2A70 registration probes vertical segments at the unit XY,
    /// including the upward probe. Both directions first test the world root box
    /// (7C25D0). Ignoring Z here also covers every native segment endpoint.
    /// MapObject box registration is deliberately not covered by this predicate.
    pub(super) fn affects_unit(&self, previous: u64, position: Vec3) -> bool {
        let count = self.revision.wrapping_sub(previous);
        if count > self.motions.len() as u64 {
            return true;
        }
        self.motions
            .iter()
            .rev()
            .take(count as usize)
            .any(|motion| {
                let bounds = motion.bounds;
                position.x >= bounds[0].x
                    && position.x <= bounds[1].x
                    && position.y >= bounds[0].y
                    && position.y <= bounds[1].y
            })
    }

    /// 7C2E70 uses a vertical collision-center probe and a render-box group query.
    /// Probe dependencies cover the entire vertical column. Box dependencies use
    /// stock's local AABB test, which is more permissive than world-AABB overlap
    /// for rotated roots. Rejecting only by world bounds would lose destinations.
    pub(super) fn affects_map_object(
        &self,
        previous: u64,
        center: Vec3,
        render_bounds: MovementCollisionBounds,
    ) -> bool {
        let count = self.revision.wrapping_sub(previous);
        if count > self.motions.len() as u64 {
            return true;
        }
        self.motions
            .iter()
            .rev()
            .take(count as usize)
            .any(|motion| {
                let bounds = motion.bounds;
                if center.x >= bounds[0].x
                    && center.x <= bounds[1].x
                    && center.y >= bounds[0].y
                    && center.y <= bounds[1].y
                {
                    return true;
                }
                let Some(registration) = &motion.registration else {
                    return true;
                };
                // If a dependency cannot be evaluated, run the original query
                // so its validation/error behavior is preserved.
                let Ok(root) = MovementCollisionBounds::new(
                    registration.local_bounds[0],
                    registration.local_bounds[1],
                ) else {
                    return true;
                };
                registration.inverse_transforms.iter().any(|&inverse| {
                    match render_bounds.transformed(inverse) {
                        Ok(local) => local.intersects(root),
                        Err(_) => true,
                    }
                })
            })
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/lighting_changes.rs"]
mod tests;

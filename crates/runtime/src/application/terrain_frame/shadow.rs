//! One frame's native scenery admission and nearer-map exclusion volumes.

use glam::Vec3;
use solarity_rendering::{
    WorldCameraFrame, WorldEnvironmentShadowFrame, WorldEnvironmentShadowMap,
    WorldShadowProjection, WorldShadowQuality,
};
use solarity_systems::MovementCollisionBounds;
use std::collections::HashMap;

use super::RuntimeTerrainFrameError;
use crate::application::terrain_coordinator::RuntimeWorldModelMovementOwner;

#[cfg(test)]
#[path = "../../../tests/application/environment_shadow_admission.rs"]
mod tests;

/// Group-derived membership, accumulated across every resident exterior MODR.
pub(super) type WorldModelShadowDoodads = HashMap<(RuntimeWorldModelMovementOwner, usize), u8>;

/// Borrowed spatial and WMO-group queries used while preparing M2 poses.
#[derive(Clone, Copy)]
pub(super) struct SceneryShadowQueries<'a> {
    pub admission: &'a WorldShadowAdmission,
    pub doodads: &'a WorldModelShadowDoodads,
}

/// Collector registration determines both caster masks and radius restrictions.
#[derive(Clone, Copy)]
pub(super) enum ModelShadowKind {
    Unit,
    StaticScenery,
    AnimatedScenery,
    StaticGameObject,
    AnimatedGameObject,
    /// 7BC890 includes every doodad of a moving WMO in its animated bank.
    MovingWorldModelDoodad,
}

/// Immutable queries shared by WMO groups, M2 roots, and their attachments.
pub(super) struct WorldShadowAdmission {
    quality: WorldShadowQuality,
    active_maps: u8,
    primary: WorldShadowProjection,
    environment: [Option<WorldShadowProjection>; 3],
    exclusions: [Option<WorldShadowProjection>; 3],
}

impl WorldShadowAdmission {
    pub(super) fn active_maps(&self) -> u8 {
        self.active_maps
    }

    pub(super) fn is_cascaded(&self) -> bool {
        self.quality == WorldShadowQuality::Cascaded
    }

    /// The primary mask's animated bit also limits unit radii at qualities 3/4.
    pub(super) fn admits_primary_unit(&self, bounds: MovementCollisionBounds, radius: f32) -> bool {
        let maximum = if self.quality == WorldShadowQuality::Cascaded {
            10_000.
        } else {
            25.
        };
        (0.25..=maximum).contains(&radius)
            && self
                .primary
                .admits_bounds(bounds.minimum(), bounds.maximum())
    }

    /// Builds the second admission volume and full preceding-volume exclusion
    /// once per map. Empty cached updates neither collect nor exclude scenery.
    pub(super) fn new(
        primary: WorldShadowProjection,
        frame: WorldEnvironmentShadowFrame<'_>,
        camera: WorldCameraFrame,
        ray: Vec3,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let quality = frame.quality();
        let mut environment = [None; 3];
        let mut exclusions = [None; 3];
        let mut previous = (quality == WorldShadowQuality::Cascaded).then_some(primary);
        for (index, pass) in frame.passes().into_iter().enumerate() {
            let Some(pass) = pass else {
                previous = None;
                continue;
            };
            environment[index] = Some(pass.projection().with_camera_culling(camera));
            exclusions[index] = previous;
            // Cached regions exclude a complete pending extent in the next
            // map, even while only one region is being rasterized this frame.
            previous = Some(WorldShadowProjection::environment(
                quality,
                WorldEnvironmentShadowMap::ALL[index],
                pass.update().center,
                primary.origin(),
                ray,
            )?);
        }
        Ok(Self {
            quality,
            active_maps: environment
                .iter()
                .enumerate()
                .fold(8, |maps, (index, projection)| {
                    maps | (u8::from(projection.is_some()) << index)
                }),
            primary,
            environment,
            exclusions,
        })
    }

    /// Returns spatial membership before registration-specific mask filtering.
    /// The low three bits are environment maps; bit three is primary scenery.
    pub(super) fn admitted_maps(&self, bounds: MovementCollisionBounds) -> u8 {
        let minimum = bounds.minimum();
        let maximum = bounds.maximum();
        let mut maps = u8::from(self.primary.admits_bounds(minimum, maximum)) << 3;
        for (index, projection) in self.environment.iter().enumerate() {
            if projection.is_some_and(|projection| projection.admits_bounds(minimum, maximum))
                && self.exclusions[index]
                    .is_none_or(|projection| !projection.contains_bounds(minimum, maximum))
            {
                maps |= 1 << index;
            }
        }
        maps
    }

    /// Applies 7BB9D0/7BA8F0's class and radius tests after spatial admission.
    /// Unit primary packets stay in their existing queue and are not duplicated.
    pub(super) fn model_maps(&self, kind: ModelShadowKind, radius: f32, maps: u8) -> u8 {
        let cascaded = self.quality == WorldShadowQuality::Cascaded;
        let mut result = maps;
        let (animated, dynamic, unlimited) = match kind {
            ModelShadowKind::Unit => {
                result &= 7;
                (true, true, false)
            }
            ModelShadowKind::StaticScenery => (false, false, false),
            ModelShadowKind::AnimatedScenery => (true, false, false),
            ModelShadowKind::StaticGameObject => (false, true, false),
            ModelShadowKind::AnimatedGameObject => (true, true, false),
            ModelShadowKind::MovingWorldModelDoodad => (true, false, true),
        };
        if !radius.is_finite() || radius < 0. {
            return 0;
        }
        if !unlimited && radius > 10_000. {
            if cascaded && !dynamic {
                // 7BCC00/7BC490 collect scenery into cascades without the
                // ordinary primary collector's maximum-radius restriction.
                result &= 7;
            } else {
                return 0;
            }
        }
        if !cascaded {
            // Cached environment maps have mask 8; primary scenery has mask 4.
            result &= if animated { 8 } else { 7 };
            if animated && !unlimited && radius > 25. {
                result &= !8;
            }
        }
        if dynamic {
            for (index, minimum) in [0.25, 2., 10., 0.25].into_iter().enumerate() {
                if radius < minimum {
                    result &= !(1 << index);
                }
            }
        }
        result
    }

    /// Static WMO geometry uses bit 8; transformed moving roots use bit 4.
    pub(super) fn world_model_maps(&self, maps: u8, moving: bool) -> u8 {
        if self.quality == WorldShadowQuality::Cascaded {
            maps
        } else {
            maps & if moving { 8 } else { 7 }
        }
    }
}

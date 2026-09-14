//! Resolve lighting only after geometry demand has selected receivers and parents.

use super::super::super::{M2Frame, RuntimeTerrainFrameError};
use crate::application::terrain_coordinator::RuntimeTerrainCoordinator;
use solarity_systems::WorldEntityLightEnvironment;

impl M2Frame {
    /// 821A20 drains registered render roots through 831AF0 after pose/effects.
    /// A shadow-only model has no lit-color receiver; a visible child still
    /// requires the parent's callback and center. Preserve traversal order for
    /// selected callbacks and compact logical indices before renderer validation.
    pub(in crate::application::terrain_frame::m2) fn prepare_visible_receivers(
        &mut self,
        animation_time_ms: f32,
        mut spatial: Option<(&mut RuntimeTerrainCoordinator, WorldEntityLightEnvironment)>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _profile_scope = solarity_profiling::profile!(
            "runtime.application.terrain_frame.m2.preparation.receivers.frame.prepare_visible_receivers"
        );
        for index in self
            .visible_draws
            .iter()
            .filter_map(|draw| draw.scene_index())
            .chain(
                self.particle_draws
                    .iter()
                    .filter_map(|draw| draw.scene_index()),
            )
            .chain(
                self.ribbon_draws
                    .iter()
                    .filter_map(|draw| draw.scene_index()),
            )
        {
            self.receiver_frame.require(index);
        }
        for &index in &self.receiver_frame.touched {
            if !self.receiver_frame.needed[index] {
                continue;
            }
            let Some(request) = self.receiver_frame.requests[index] else {
                continue;
            };
            let placement = &mut self.placements[index];
            let callback = if request.parent.is_none()
                && let Some((terrain, environment)) = spatial.as_mut()
                && let Some(source) = &self.sources[placement.source_index]
            {
                placement.entity_lighting.sample(
                    placement
                        .retirement
                        .as_ref()
                        .map_or(placement.owner, |retired| retired.original_owner),
                    &source.model,
                    placement.transform,
                    placement.color,
                    animation_time_ms,
                    terrain,
                    *environment,
                )?
            } else {
                None
            };
            self.receiver_frame.remap[index] = Some(self.scene_lighting.receiver_with_light(
                index,
                request.parent,
                request.center,
                callback,
                request.fog,
            )?);
        }
        for draw in &mut self.visible_draws {
            if let Some(index) = draw.scene_index() {
                *draw = draw.with_scene_index(Some(self.receiver_frame.resolved(index)?));
            }
        }
        for draw in &mut self.particle_draws {
            if let Some(index) = draw.scene_index() {
                *draw = draw.with_scene_index(Some(self.receiver_frame.resolved(index)?));
            }
        }
        for draw in &mut self.ribbon_draws {
            if let Some(index) = draw.scene_index() {
                *draw = draw.with_scene_index(Some(self.receiver_frame.resolved(index)?));
            }
        }
        Ok(())
    }
}

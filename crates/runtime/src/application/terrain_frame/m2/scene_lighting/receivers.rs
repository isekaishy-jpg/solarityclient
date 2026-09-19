//! Frozen receiver facts shared across independent uniform batches.

use super::{RuntimeTerrainFrameError, SceneLightSources};
use glam::Vec3;
use solarity_rendering::{
    M2DirectionalLight, M2LocalLightState, M2SceneUniform, merge_wotlk_directional_lights,
};
use std::sync::Arc;

/// Main publishes callbacks in stock order; workers only read their completed facts.
#[derive(Default)]
pub(super) struct ReceiverInputs {
    centers: Vec<Vec3>,
    placement_centers: Vec<Option<Vec3>>,
    receiver_lights: Vec<Option<M2DirectionalLight>>,
    receiver_fog: Vec<Option<Vec3>>,
    placement_lights: Vec<Option<M2DirectionalLight>>,
    placement_parents: Vec<Option<usize>>,
    receiver_placements: Vec<usize>,
    /// Current frame's indexed extent, independent of retained storage capacity.
    placement_end: usize,
}

impl ReceiverInputs {
    /// Reclamation returns all readers before mutation; no in-flight copy is allowed.
    pub(super) fn exclusive(owner: &mut Arc<Self>) -> &mut Self {
        Arc::get_mut(owner)
            .unwrap_or_else(|| unreachable!("receiver readers return before frame mutation"))
    }

    pub(super) fn len(&self) -> usize {
        self.centers.len()
    }

    /// Reset only last frame's published placements, not all resident scenery slots.
    pub(super) fn clear(&mut self) {
        self.centers.clear();
        for &placement in &self.receiver_placements {
            self.placement_centers[placement] = None;
            self.placement_lights[placement] = None;
            self.placement_parents[placement] = None;
        }
        self.receiver_lights.clear();
        self.receiver_fog.clear();
        self.receiver_placements.clear();
        self.placement_end = 0;
    }

    /// The returned index retains callback order regardless of worker completion order.
    pub(super) fn push(
        &mut self,
        placement_index: usize,
        parent: Option<usize>,
        center: Vec3,
        light: Option<M2DirectionalLight>,
        fog_color: Option<Vec3>,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        let index = u32::try_from(self.centers.len())
            .map_err(|_| solarity_rendering::VulkanError::WorldFrameCapacity)?;
        let size = self.placement_centers.len().max(placement_index + 1);
        self.placement_end = self.placement_end.max(placement_index + 1);
        self.placement_centers.resize(size, None);
        self.placement_centers[placement_index] = Some(center);
        self.placement_lights.resize(size, None);
        self.placement_lights[placement_index] = light;
        self.placement_parents.resize(size, None);
        self.placement_parents[placement_index] = parent;
        self.receiver_placements.push(placement_index);
        self.receiver_lights.push(light);
        self.receiver_fog.push(fog_color);
        self.centers.push(center);
        Ok(index)
    }

    /// Samples one receiver from complete source and ancestry banks. The arithmetic
    /// and parent/source selection order are the original serial receiver operation.
    pub(super) fn evaluate(
        &self,
        index: usize,
        sources: &SceneLightSources,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<M2SceneUniform, RuntimeTerrainFrameError> {
        let mut center = self.centers[index];
        let mut light = self.receiver_lights[index];
        let placement = self.receiver_placements[index];
        let mut current = placement;
        // A vehicle may be published after its passenger. All callbacks have
        // joined before this bank is shared, including parents in another batch.
        for _ in 0..self.placement_end {
            let Some(parent) = self.placement_parents.get(current).copied().flatten() else {
                break;
            };
            if parent == placement {
                break;
            }
            if let Some(value) = self.placement_centers.get(parent).copied().flatten() {
                center = value;
            }
            light = self
                .placement_lights
                .get(parent)
                .copied()
                .flatten()
                .or(light);
            current = parent;
        }
        let exterior = light.unwrap_or(exterior);
        let sunlight = merge_wotlk_directional_lights(
            sources
                .directionals
                .iter()
                .chain(std::iter::once(&exterior)),
        );
        let mut lights = [M2LocalLightState::disabled(); 4];
        if let Some(sunlight) = sunlight {
            lights[0] = sunlight.local_light_state();
        }
        for (slot, index) in sources
            .points
            .query(center, 0.0)?
            .indices()
            .into_iter()
            .flatten()
            .take(3)
            .enumerate()
        {
            lights[slot + 1] = sources.points.points()[index].local_light_state();
        }
        let scene = base.with_local_lights(lights);
        Ok(self.receiver_fog[index].map_or(scene, |color| scene.with_fog_color(color)))
    }
}

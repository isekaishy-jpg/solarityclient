//! Complete WMO resource staging before terrain collision and placement publication.

use super::{
    RuntimeTerrainFrameError, WorldModelFrame, WorldModelGpuPlacementOwner, prepare_gpu_source,
};
use crate::application::terrain_coordinator::world_model_residency::ResidentWorldModelSource;
use solarity_rendering::VulkanRenderer;
use std::sync::Arc;

impl WorldModelFrame {
    /// Shared root/group generations already uploaded need no admission slot.
    pub(in crate::application::terrain_frame) fn static_source_is_prepared(
        &self,
        source: &ResidentWorldModelSource,
    ) -> bool {
        self.placements
            .iter()
            .filter(|placement| {
                matches!(placement.owner, WorldModelGpuPlacementOwner::Static { .. })
            })
            .filter_map(|placement| self.sources[placement.source_index].as_ref())
            .chain(&self.prepared_static)
            .any(|gpu| Arc::ptr_eq(&gpu.model, source.model()))
    }

    /// Prepares one source without creating any MODF placement or collision owner.
    pub(in crate::application::terrain_frame) fn prepare_static_source(
        &mut self,
        renderer: &mut VulkanRenderer,
        source: &ResidentWorldModelSource,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.prepared_static.push(prepare_gpu_source(
            renderer,
            source,
            self.filtering,
            self.base_mip,
            &mut self.liquid_materials,
        )?);
        Ok(())
    }

    /// Cancels unpublished WMO liquids with the renderer's ordinary fence ownership.
    pub(in crate::application::terrain_frame) fn clear_prepared_static(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let handles = self
            .prepared_static
            .iter()
            .flat_map(|source| &source.liquids)
            .map(|batch| batch.mesh())
            .collect::<Vec<_>>();
        renderer.retire_liquid_meshes(&handles)?;
        self.prepared_static.clear();
        Ok(())
    }
}

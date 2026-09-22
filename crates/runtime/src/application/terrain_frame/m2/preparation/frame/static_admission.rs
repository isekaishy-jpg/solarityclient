//! Captures compact static facts after native WMO visibility has been resolved.

use super::super::spatial::SpatialView;
use super::input::FrameView;
use crate::application::terrain_frame::m2::{M2Frame, RuntimeTerrainFrameError};
use crate::application::terrain_frame::shadow::SceneryShadowQueries;
use solarity_cpu::CpuExecutor;

impl M2Frame {
    /// No simulation records or source leases enter this phase. Immutable
    /// metadata survives topology relocation; WMO decisions belong to this frame.
    pub(super) fn begin_static_admission(
        &mut self,
        cpu: &CpuExecutor,
        view: FrameView,
        spatial_lighting: bool,
        shadows: Option<SceneryShadowQueries<'_>>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let _profile = solarity_profiling::profile!("m2.static_admission.capture");
        self.spatial_batch
            .prepare(cpu, self.frame_work.remaining_count())?;
        let spatial_view = SpatialView {
            camera: view.camera.camera().position(),
            detail: self.environment_detail,
            frustum: view.frustum,
            shadows: shadows.map(|queries| *queries.admission),
        };
        for &index in self.frame_work.selected_indices() {
            let Some(mut input) = self
                .placement_visibility
                .static_admission_input(index, shadows)
            else {
                continue;
            };
            input.publishes_lights &= view.world_lighting.is_some();
            input.doodad_active &= spatial_lighting;
            input.doodad_visible =
                !input.doodad_active || self.doodad_scene.fog_bank(index).is_some();
            input.doodad_opacity = self.doodad_scene.opacity(index);
            self.spatial_batch.push(index, input, spatial_view)?;
        }
        self.spatial_batch.start()
    }
}

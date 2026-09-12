//! Neighbor-tile resources prepared without activating placements or animation clocks.

use super::{M2Frame, M2GluePipelineWarmup, M2GpuSource, RuntimeTerrainFrameError, prepare_source};
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use solarity_asset::DecodedM2Model;
use solarity_rendering::{M2ModelOrientation, VulkanRenderer};
use std::sync::Arc;

/// Retains uploaded resources until the exact tile generation commits membership.
pub(super) struct PreparedStaticM2 {
    pub(super) model: Arc<DecodedM2Model>,
    pub(super) source: Option<M2GpuSource>,
}

/// Pipeline and upload work occupy separate admission services.
pub(in crate::application::terrain_frame) enum M2SourceAdmission {
    Pipelines(M2GluePipelineWarmup),
    Upload,
}

impl M2Frame {
    /// Existing sources and completed staging need no further GPU work.
    pub(in crate::application::terrain_frame) fn static_source_is_prepared(
        &self,
        source: &ResidentM2Source,
    ) -> bool {
        self.static_residency
            .contains_source(&self.sources, source.model())
            || self
                .prepared_static
                .iter()
                .any(|gpu| Arc::ptr_eq(&gpu.model, source.model()))
    }

    /// Advances one cold source while leaving random consumption and visible owners untouched.
    pub(in crate::application::terrain_frame) fn prepare_static_source(
        &mut self,
        renderer: &mut VulkanRenderer,
        source: &ResidentM2Source,
        admission: &mut Option<M2SourceAdmission>,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let stage = admission.get_or_insert_with(|| {
            M2SourceAdmission::Pipelines(M2GluePipelineWarmup::new(
                source.cpu_source(),
                M2ModelOrientation::Authored,
            ))
        });
        match stage {
            M2SourceAdmission::Pipelines(pipelines) => {
                if pipelines.service_one(renderer)? {
                    *stage = M2SourceAdmission::Upload;
                }
                Ok(false)
            }
            M2SourceAdmission::Upload => {
                self.prepared_static.push(PreparedStaticM2 {
                    model: Arc::clone(source.model()),
                    source: prepare_source(renderer, source)?,
                });
                *admission = None;
                Ok(true)
            }
        }
    }

    /// Cancels unpublished resource ownership when the requested ADT generation changes.
    pub(in crate::application::terrain_frame) fn clear_prepared_static(&mut self) {
        self.prepared_static.clear();
    }
}

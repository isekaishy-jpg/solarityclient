//! Renderer-local resources for the shared resident placed-M2 scene.

use std::sync::Arc;

use glam::Mat4;
use solarity_rendering::{M2MeshHandle, M2MeshPlan, VulkanRenderer};

use crate::application::terrain_coordinator::m2_residency::{ResidentM2Owner, ResidentM2Scene};

use super::RuntimeTerrainFrameError;

/// Build 12340's highest-capability external SKIN selection.
///
/// The stock client chooses one `%02d.skin` companion when the shared model is
/// loaded. Its world-distance policy culls and fades whole placements; it does
/// not swap geometry profiles per placement. Vulkan 1.3 exceeds the original
/// hardware capability gate, so this renderer selects the authored `00.skin`.
const STOCK_HIGH_CAPABILITY_PROFILE: usize = 0;

/// One selected M2/SKIN generation uploaded once for all of its placements.
struct M2GpuSource {
    plan: Arc<M2MeshPlan>,
    mesh: M2MeshHandle,
}

/// Exact per-instance state required by later animation and material assembly.
struct M2GpuPlacement {
    source_index: usize,
    transform: Mat4,
    owner: ResidentM2Owner,
    flags: u16,
    color: [u8; 4],
}

/// All resident M2 geometry and transforms owned by one terrain generation.
pub(super) struct M2Frame {
    sources: Vec<M2GpuSource>,
    placements: Vec<M2GpuPlacement>,
}

impl M2Frame {
    /// Uploads every authored SKIN profile without selecting an unproved LOD.
    pub(super) fn prepare(
        renderer: &mut VulkanRenderer,
        scene: &ResidentM2Scene,
    ) -> Result<Self, RuntimeTerrainFrameError> {
        let mut sources = Vec::with_capacity(scene.sources().len());
        for source in scene.sources() {
            let plan = Arc::new(M2MeshPlan::prepare(
                source.model(),
                STOCK_HIGH_CAPABILITY_PROFILE,
            )?);
            let mesh = renderer.upload_m2_mesh(&plan)?;
            sources.push(M2GpuSource { plan, mesh });
        }

        let mut placements = Vec::with_capacity(scene.placements().len());
        for placement in scene.placements() {
            if placement.source_index() >= sources.len() {
                return Err(RuntimeTerrainFrameError::M2SourceIndex {
                    source_index: placement.source_index(),
                    source_count: sources.len(),
                });
            }
            placements.push(M2GpuPlacement {
                source_index: placement.source_index(),
                transform: placement.transform(),
                owner: placement.owner(),
                flags: placement.flags(),
                color: placement.color(),
            });
        }
        Ok(Self {
            sources,
            placements,
        })
    }

    /// Returns the number of selected shared M2 GPU generations.
    pub(super) fn mesh_count(&self) -> usize {
        for source in &self.sources {
            debug_assert_eq!(source.plan.profile_index(), STOCK_HIGH_CAPABILITY_PROFILE);
            tracing::trace!(
                path = %source.plan.path(),
                mesh = ?source.mesh,
                profile_index = source.plan.profile_index(),
                "shared M2 entered renderer generation"
            );
        }
        self.sources.len()
    }

    /// Returns independently transformed MDDF and MODD instance count.
    pub(super) fn placement_count(&self) -> usize {
        for placement in &self.placements {
            debug_assert!(placement.source_index < self.sources.len());
            debug_assert!(placement.transform.is_finite());
            tracing::trace!(
                owner = ?placement.owner,
                flags = placement.flags,
                color = ?placement.color,
                "placed M2 retained by renderer generation"
            );
        }
        self.placements.len()
    }
}

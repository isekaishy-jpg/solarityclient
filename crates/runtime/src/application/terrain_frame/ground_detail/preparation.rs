//! Finite worker ownership of exact native ground-detail chunk preparation.

use super::RuntimeTerrainFrameError;
use crate::application::terrain_coordinator::ResidentGroundDetailTile;
use solarity_asset::{DecodedTerrainTile, GroundEffectCatalog, TerrainChunkIndex};
use solarity_cpu::{CpuTask, CpuTaskPermit};
use solarity_rendering::{
    GroundDetailDensity, GroundDetailMeshPlan, TerrainDetailChunk, TerrainTileMeshPlan,
};
use std::sync::Arc;

/// Shared immutable providers pin the requested ADT and density generation.
pub(super) struct DetailRequest {
    pub(super) plan: Arc<TerrainTileMeshPlan>,
    pub(super) decoded: Arc<DecodedTerrainTile>,
    pub(super) assets: Arc<ResidentGroundDetailTile>,
    pub(super) catalog: Arc<GroundEffectCatalog>,
    pub(super) chunk: TerrainChunkIndex,
    pub(super) index: usize,
    pub(super) density: GroundDetailDensity,
}

/// A completed mesh has no renderer resources until main-thread admission.
pub(super) struct PreparedDetail {
    pub(super) _plan: Arc<TerrainTileMeshPlan>,
    pub(super) assets: Arc<ResidentGroundDetailTile>,
    pub(super) mesh: Arc<GroundDetailMeshPlan>,
    pub(super) index: usize,
}

/// One finite executor-owned job; dropping it never waits on worker execution.
pub(super) struct PendingDetail {
    pub(super) plan: Arc<TerrainTileMeshPlan>,
    pub(super) density: GroundDetailDensity,
    task: CpuTask<Result<PreparedDetail, RuntimeTerrainFrameError>>,
}

impl PendingDetail {
    /// A reserved slot transfers ownership only after admission can no longer fail.
    pub(super) fn submit(permit: CpuTaskPermit<'_>, request: DetailRequest) -> Self {
        Self {
            plan: Arc::clone(&request.plan),
            density: request.density,
            task: permit.submit(move || {
                let scatter = TerrainDetailChunk::prepare(
                    &request.decoded,
                    request.chunk,
                    &request.catalog,
                    request.density,
                )?;
                let mesh = Arc::new(GroundDetailMeshPlan::prepare(
                    &scatter,
                    &request.catalog,
                    request.density,
                    |id| request.assets.models.get(&id).map(Arc::as_ref),
                )?);
                Ok(PreparedDetail {
                    _plan: request.plan,
                    assets: request.assets,
                    mesh,
                    index: request.index,
                })
            }),
        }
    }

    pub(super) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    /// The caller must observe completion before joining on the presentation owner.
    pub(super) fn finish(
        self,
    ) -> Result<Result<PreparedDetail, RuntimeTerrainFrameError>, solarity_cpu::CpuError> {
        self.task.join()
    }
}

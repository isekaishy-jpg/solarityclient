//! Nearby MCNK detail generations retained until density or ADT ownership changes.

use std::collections::VecDeque;
use std::sync::Arc;

use glam::Vec3;
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, GroundDetailDensity, GroundDetailDraw,
    GroundDetailFrame, TerrainTileMeshPlan, VulkanRenderer, WorldCameraFrame, WorldFrustum,
    WorldModelBaseMip, WorldModelTextureFiltering,
};

mod preparation;

use preparation::{DetailRequest, PendingDetail, PreparedDetail};

use crate::application::cpu_retirement::CpuRetirementQueue;

use super::RuntimeTerrainFrameError;
use crate::application::terrain_coordinator::ResidentTerrainTile;

/// A generation token also prevents address reuse while cached chunk draws exist.
struct DetailTile {
    plan: Arc<TerrainTileMeshPlan>,
    chunks: Vec<Option<GroundDetailDraw>>,
    requested: Vec<bool>,
    bounds: [Vec3; 2],
}

/// Native detail policy and immutable chunks shared with submitted GPU frames.
pub(super) struct GroundDetailWorld {
    tiles: Vec<DetailTile>,
    retired: CpuRetirementQueue<DetailTile>,
    draws: Vec<GroundDetailDraw>,
    requests: VecDeque<DetailRequest>,
    pending: Option<PendingDetail>,
    retired_preparations: CpuRetirementQueue<PreparedDetail>,
    density: GroundDetailDensity,
    distance: f32,
    filtering: WorldModelTextureFiltering,
    base_mip: WorldModelBaseMip,
}

impl GroundDetailWorld {
    /// Captures the registered defaults; live CVars replace them before presentation.
    pub(super) fn new(filtering: WorldModelTextureFiltering, base_mip: WorldModelBaseMip) -> Self {
        Self {
            tiles: Vec::new(),
            retired: CpuRetirementQueue::new(),
            draws: Vec::new(),
            requests: VecDeque::new(),
            pending: None,
            retired_preparations: CpuRetirementQueue::new(),
            density: GroundDetailDensity::default(),
            distance: 140.0,
            filtering,
            base_mip,
        }
    }

    /// Mirrors callbacks 78DAB0/78DB10; only density invalidates generated geometry.
    pub(super) fn configure(
        &mut self,
        density: Option<f32>,
        distance: Option<f32>,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let density = density
            .filter(|value| value.is_finite())
            .and_then(|value| GroundDetailDensity::new(value.clamp(16.0, 256.0) as u32))
            .ok_or(RuntimeTerrainFrameError::InvalidGroundDetailCvar)?;
        let distance = distance
            .filter(|value| value.is_finite())
            .ok_or(RuntimeTerrainFrameError::InvalidGroundDetailCvar)?
            .clamp(0.0, 140.0);
        if density != self.density {
            self.retired.extend(self.tiles.drain(..));
            self.density = density;
        }
        self.distance = distance;
        Ok(())
    }

    /// Requests only nearby chunks admitted by native 7D3FE0/7D3390 visibility.
    /// Immutable scatter/mesh work runs on the application executor; completed
    /// chunks retain the same authored placement and density selection.
    pub(super) fn prepare<'a>(
        &mut self,
        renderer: &mut VulkanRenderer,
        residents: impl Iterator<Item = &'a ResidentTerrainTile> + Clone,
        camera: WorldCameraFrame,
        frustum: WorldFrustum,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let mut profile = crate::application::frame_profile::RuntimeFrameProfile::new(
            "Ground detail preparation",
        );
        self.draws.clear();
        self.retired.extend(self.tiles.extract_if(.., |cached| {
            !residents
                .clone()
                .any(|tile| Arc::ptr_eq(tile.mesh(), &cached.plan))
        }));
        self.requests.retain(|request| {
            request.density == self.density
                && self
                    .tiles
                    .iter()
                    .any(|tile| Arc::ptr_eq(&tile.plan, &request.plan))
        });
        self.publish_prepared(renderer)?;
        profile.mark("retirement and admission");
        if self.distance == 0.0 {
            return Ok(());
        }
        for resident in residents {
            let assets = resident.ground_detail();
            let Some(catalog) = &assets.catalog else {
                continue;
            };
            let index = match self
                .tiles
                .iter()
                .position(|tile| Arc::ptr_eq(&tile.plan, resident.mesh()))
            {
                Some(index) => index,
                None => {
                    // A prepared ADT contains all 256 chunk bounds. Their union
                    // can reject only tiles whose individual chunks also reject.
                    let bounds = resident.mesh().chunks().iter().fold(
                        [Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)],
                        |[minimum, maximum], chunk| {
                            let [lower, upper] = chunk.bounds().map(Vec3::from_array);
                            [minimum.min(lower), maximum.max(upper)]
                        },
                    );
                    self.tiles.push(DetailTile {
                        plan: Arc::clone(resident.mesh()),
                        chunks: vec![None; resident.mesh().chunks().len()],
                        requested: vec![false; resident.mesh().chunks().len()],
                        bounds,
                    });
                    self.tiles.len() - 1
                }
            };
            let cached = &mut self.tiles[index];
            let center = (cached.bounds[0] + cached.bounds[1]) * 0.5;
            let half = (cached.bounds[1] - cached.bounds[0]) * 0.5;
            if nearest_depth(cached.bounds, camera) >= self.distance
                || !frustum.intersects_box(
                    center,
                    Vec3::X * half.x,
                    Vec3::Y * half.y,
                    Vec3::Z * half.z,
                )?
            {
                continue;
            }
            for (index, (chunk, draw)) in cached
                .plan
                .chunks()
                .iter()
                .zip(&mut cached.chunks)
                .enumerate()
            {
                // 7C3E70/790650 select the nearest AABB corner along view forward;
                // chunk+88 is projected depth, consumed by 7D3FE0's strict range test.
                let depth = nearest_depth(chunk.bounds().map(Vec3::from_array), camera);
                if depth >= self.distance || !chunk.is_visible(frustum)? {
                    continue;
                }
                if draw.is_none() && !cached.requested[index] {
                    self.requests.push_back(DetailRequest {
                        plan: Arc::clone(&cached.plan),
                        decoded: Arc::clone(resident.decoded()),
                        assets: Arc::clone(assets),
                        catalog: Arc::clone(catalog),
                        chunk: chunk.chunk(),
                        index,
                        density: self.density,
                    });
                    cached.requested[index] = true;
                }
                if let Some(draw) = draw
                    .as_ref()
                    .filter(|draw| !draw.plan().batches().is_empty())
                {
                    self.draws.push(draw.clone());
                }
            }
        }
        Ok(())
    }

    /// Admits one completed immutable chunk, rejecting retired terrain/density tokens.
    fn publish_prepared(
        &mut self,
        renderer: &mut VulkanRenderer,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if !self
            .pending
            .as_ref()
            .is_some_and(PendingDetail::is_finished)
        {
            return Ok(());
        }
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        let Some(tile) = self
            .tiles
            .iter_mut()
            .find(|tile| Arc::ptr_eq(&tile.plan, &pending.plan))
            .filter(|_| pending.density == self.density)
        else {
            // A replaced density/ADT generation no longer owns either a mesh
            // or an asset error. Executor failures still belong to this owner.
            if let Ok(prepared) = pending.finish()? {
                self.retired_preparations.extend([prepared]);
            }
            return Ok(());
        };
        let prepared = pending.finish()??;
        let sources = prepared
            .mesh
            .batches()
            .iter()
            .map(|batch| {
                prepared
                    .assets
                    .textures
                    .get(batch.texture())
                    .map(|source| BlpTextureUploadRequest::new(source, BlpColorSpace::Linear))
                    .ok_or_else(|| {
                        RuntimeTerrainFrameError::MissingGroundDetailTexture(
                            batch.texture().clone(),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let textures = renderer.upload_blp_textures(&sources)?;
        tile.chunks[prepared.index] = Some(GroundDetailDraw::new(
            prepared.mesh,
            textures,
            self.filtering,
            self.base_mip,
        )?);
        Ok(())
    }

    /// Releases detached CPU plans on the bounded application worker pool.
    /// GPU resources keep their existing renderer ownership and fence lifetimes.
    pub(super) fn service_retirements(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
    ) -> Result<(), solarity_cpu::CpuError> {
        if self.pending.is_none() && !self.requests.is_empty() {
            match cpu.try_reserve() {
                Ok(permit) => {
                    if let Some(request) = self.requests.pop_front() {
                        self.pending = Some(PendingDetail::submit(permit, request));
                    }
                }
                Err(solarity_cpu::CpuError::AtCapacity { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        self.retired_preparations.service(cpu)?;
        self.retired.service(cpu)
    }

    /// Borrows prepared packets for the current submission's resource pinning.
    pub(super) fn frame(
        &self,
        camera_position: Vec3,
    ) -> Result<GroundDetailFrame<'_>, RuntimeTerrainFrameError> {
        Ok(GroundDetailFrame::new(
            &self.draws,
            self.distance,
            camera_position,
        )?)
    }
}

/// Projects the minimum-depth AABB support point, shared by tile and chunk tests.
fn nearest_depth(bounds: [Vec3; 2], camera: WorldCameraFrame) -> f32 {
    let forward = camera.forward();
    let corner = Vec3::from_array(std::array::from_fn(|axis| {
        bounds[usize::from(forward[axis] < 0.0)][axis]
    }));
    forward.dot(corner - camera.camera().position())
}

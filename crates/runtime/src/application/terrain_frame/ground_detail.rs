//! Nearby MCNK detail generations retained until density or ADT ownership changes.

use std::sync::Arc;

use glam::Vec3;
use solarity_rendering::{
    BlpColorSpace, BlpTextureUploadRequest, GroundDetailDensity, GroundDetailDraw,
    GroundDetailFrame, GroundDetailMeshPlan, TerrainDetailChunk, TerrainTileMeshPlan,
    VulkanRenderer, WorldCameraFrame, WorldFrustum, WorldModelBaseMip, WorldModelTextureFiltering,
};

mod retirement;

use retirement::CpuRetirementQueue;

use super::RuntimeTerrainFrameError;
use crate::application::terrain_coordinator::ResidentTerrainTile;

/// A generation token also prevents address reuse while cached chunk draws exist.
struct DetailTile {
    plan: Arc<TerrainTileMeshPlan>,
    chunks: Vec<Option<GroundDetailDraw>>,
    bounds: [Vec3; 2],
}

/// Native detail policy and immutable chunks shared with submitted GPU frames.
pub(super) struct GroundDetailWorld {
    tiles: Vec<DetailTile>,
    retired: CpuRetirementQueue<DetailTile>,
    draws: Vec<GroundDetailDraw>,
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

    /// Generates only admitted nearby chunks, as native 7D3FE0/7D3390 do on demand.
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
        profile.mark("retirement");
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
            for (chunk, draw) in cached.plan.chunks().iter().zip(&mut cached.chunks) {
                // 7C3E70/790650 select the nearest AABB corner along view forward;
                // chunk+88 is projected depth, consumed by 7D3FE0's strict range test.
                let depth = nearest_depth(chunk.bounds().map(Vec3::from_array), camera);
                if depth >= self.distance || !chunk.is_visible(frustum)? {
                    continue;
                }
                if draw.is_none() {
                    let mut generation =
                        crate::application::frame_profile::RuntimeFrameProfile::new(
                            "Ground detail generation",
                        );
                    let scatter = TerrainDetailChunk::prepare(
                        resident.decoded(),
                        chunk.chunk(),
                        catalog,
                        self.density,
                    )?;
                    generation.mark("scatter");
                    let mesh = Arc::new(GroundDetailMeshPlan::prepare(
                        &scatter,
                        catalog,
                        self.density,
                        |id| assets.models.get(&id).map(Arc::as_ref),
                    )?);
                    generation.mark("mesh");
                    let sources = mesh
                        .batches()
                        .iter()
                        .map(|batch| {
                            assets
                                .textures
                                .get(batch.texture())
                                .map(|source| {
                                    BlpTextureUploadRequest::new(source, BlpColorSpace::Linear)
                                })
                                .ok_or_else(|| {
                                    RuntimeTerrainFrameError::MissingGroundDetailTexture(
                                        batch.texture().clone(),
                                    )
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let textures = renderer.upload_blp_textures(&sources)?;
                    generation.mark("textures");
                    *draw = Some(GroundDetailDraw::new(
                        mesh,
                        textures,
                        self.filtering,
                        self.base_mip,
                    )?);
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

    /// Releases detached CPU plans on the bounded application worker pool.
    /// GPU resources keep their existing renderer ownership and fence lifetimes.
    pub(super) fn service_retirements(
        &mut self,
        cpu: &solarity_cpu::CpuExecutor,
    ) -> Result<(), solarity_cpu::CpuError> {
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

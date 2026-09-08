//! Process world-system particulate pool and retained underwater atlas request.

use std::sync::Arc;

use glam::{Mat4, Vec3};
use solarity_asset::{
    AssetError, AssetPath, AssetStore, BlpTextureSource, LiquidTypeCatalog, LiquidTypeDefinition,
};
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, UnderwaterParticleFog, UnderwaterParticleFrame,
    UnderwaterParticleVertex, VulkanRenderer, WorldCameraFrame,
};
use solarity_systems::UnderwaterParticles;
use thiserror::Error;

use crate::random::BlizzardRand;

/// Failure while advancing or presenting the native underwater particle pool.
#[derive(Debug, Error)]
pub enum RuntimeUnderwaterParticleError {
    /// Required atlas metadata was invalid.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Camera or time could not enter the native simulation.
    #[error(transparent)]
    Simulation(#[from] solarity_systems::UnderwaterParticleError),
    /// The retained pool lost its immutable liquid definition.
    #[error("underwater particle pool liquid type {0} has no definition")]
    MissingLiquidType(u32),
    /// A native billboard could not enter the packed vertex bank.
    #[error(transparent)]
    Vertex(#[from] solarity_rendering::UnderwaterParticleVertexError),
    /// Projection or geometry did not form a complete frame.
    #[error(transparent)]
    Frame(#[from] solarity_rendering::UnderwaterParticleFrameError),
    /// The atlas could not enter renderer storage.
    #[error(transparent)]
    Texture(#[from] solarity_rendering::BlpTextureUploadError),
    /// Vulkan allocation or submission failed.
    #[error(transparent)]
    Vulkan(#[from] solarity_rendering::VulkanError),
}

pub(super) struct RuntimeUnderwaterParticles {
    enabled: bool,
    world: Option<WorldObjectIdentity>,
    pool: Option<UnderwaterParticles>,
    source: Option<Arc<BlpTextureSource>>,
    texture: Option<BlpTextureHandle>,
    vertices: Vec<UnderwaterParticleVertex>,
    indices: Vec<u16>,
}

impl RuntimeUnderwaterParticles {
    /// Preloads 79E100's single fixed atlas before interactive world rendering.
    pub(super) fn load(store: &mut AssetStore) -> Result<Self, RuntimeUnderwaterParticleError> {
        let path = AssetPath::new("Textures/WaterPoop02.blp")?;
        let source = match BlpTextureSource::load(store, &path) {
            Ok(source) => Some(Arc::new(source)),
            Err(error) => {
                tracing::warn!(texture = %path, %error, "underwater atlas request failed; using stock green texture");
                None
            }
        };
        Ok(Self {
            enabled: true,
            world: None,
            pool: None,
            source,
            texture: None,
            vertices: Vec::new(),
            indices: Vec::new(),
        })
    }

    /// Native `waterParticulates` toggles render bit 0x02000000; it is not a CVar.
    pub(super) fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.enabled
    }

    /// 404130 initializes the world singleton before Glue presentation. The
    /// 16,004 constructor words are consumed once at that process boundary.
    pub(super) fn initialize(mut self, random: &mut BlizzardRand) -> Self {
        self.pool = Some(UnderwaterParticles::new(|| random.next_u32()));
        self
    }

    /// Scene replacement clears the selected camera liquid and frame geometry;
    /// it retains the process pool, its previous eye, and the random stream.
    pub(super) fn synchronize_world(
        &mut self,
        world: Option<&ActiveWorld>,
        random: &mut BlizzardRand,
    ) {
        let identity = world.and_then(|world| {
            world
                .local_player_guid()
                .ok()
                .and_then(|guid| world.object_identity(guid))
        });
        if self.world != identity {
            self.world = identity;
            if let Some(pool) = &mut self.pool {
                pool.select_liquid(None, self.enabled, || random.next_u32());
            }
            self.vertices.clear();
            self.indices.clear();
        }
    }

    /// 7831A0 advances the old liquid before 790920 queries the final camera.
    pub(super) fn advance(
        &mut self,
        camera: Vec3,
        seconds: f32,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeUnderwaterParticleError> {
        if let Some(pool) = &mut self.pool {
            pool.advance(camera, seconds, self.enabled, || random.next_u32())?;
        }
        Ok(())
    }

    /// Applies the current camera liquid, retaining native reseed/scale order,
    /// then prepares only the admitted billboard bank for this frame.
    pub(super) fn prepare_frame(
        &mut self,
        renderer: &mut VulkanRenderer,
        camera: WorldCameraFrame,
        liquid: Option<&LiquidTypeDefinition>,
        liquids: &LiquidTypeCatalog,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeUnderwaterParticleError> {
        self.vertices.clear();
        self.indices.clear();
        let Some(pool) = &mut self.pool else {
            return Ok(());
        };
        pool.select_liquid(liquid, self.enabled, || random.next_u32());
        if !self.enabled || !pool.visible() {
            return Ok(());
        }
        let definition = liquids.entry(pool.pool_liquid_type()).ok_or(
            RuntimeUnderwaterParticleError::MissingLiquidType(pool.pool_liquid_type()),
        )?;
        let native_view = Mat4::from_scale(Vec3::new(1., 1., -1.)) * camera.view();
        UnderwaterParticleVertex::project_into(
            pool.particles(),
            native_view,
            definition.particle_texture_slots(),
            &mut self.vertices,
            &mut self.indices,
        )?;
        if !self.vertices.is_empty() && self.texture.is_none() {
            self.texture = Some(match &self.source {
                Some(source) => renderer.upload_blp_texture(source, BlpColorSpace::Linear)?,
                None => renderer.upload_stock_m2_failure()?,
            });
        }
        Ok(())
    }

    /// Converts native positive-Z vertices through the renderer's projection.
    pub(super) fn frame(
        &self,
        camera: WorldCameraFrame,
        liquids: &LiquidTypeCatalog,
        fog: solarity_asset::WorldFogSample,
    ) -> Result<Option<UnderwaterParticleFrame<'_>>, RuntimeUnderwaterParticleError> {
        let Some(texture) = self.texture.filter(|_| !self.indices.is_empty()) else {
            return Ok(None);
        };
        let Some(pool) = &self.pool else {
            return Ok(None);
        };
        let definition = liquids.entry(pool.pool_liquid_type()).ok_or(
            RuntimeUnderwaterParticleError::MissingLiquidType(pool.pool_liquid_type()),
        )?;
        let fog = if definition.flags() & 16 == 0 {
            None
        } else {
            let (start, end) = fog.range();
            Some(UnderwaterParticleFog::new(start, end, fog.color())?)
        };
        Ok(Some(UnderwaterParticleFrame::new(
            camera.projection() * Mat4::from_scale(Vec3::new(1., 1., -1.)),
            fog,
            texture,
            &self.vertices,
            &self.indices,
        )?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solarity_ecs::{WorldBootstrap, WorldMapId};

    /// The native singleton's random stream and pool survive scene replacements.
    #[test]
    fn world_replacement_retains_process_particle_pool_and_random_state() {
        let mut random = BlizzardRand::new(12340);
        let mut expected = random;
        for _ in 0..16_004 {
            let _ = expected.next_u32();
        }
        let mut particles = RuntimeUnderwaterParticles {
            enabled: true,
            world: None,
            pool: None,
            source: None,
            texture: None,
            vertices: Vec::new(),
            indices: Vec::new(),
        }
        .initialize(&mut random);
        assert_eq!(random, expected);
        let original = particles
            .pool
            .as_ref()
            .map(|pool| pool.particles().as_ptr());
        for (map, guid) in [(0, 1), (1, 2), (389, 3)] {
            let world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(map),
                guid,
                "ParticleLifetime",
                Vec3::ZERO,
                0.,
            ));
            particles.synchronize_world(Some(&world), &mut random);
            assert_eq!(particles.world, world.object_identity(guid));
            assert_eq!(
                particles
                    .pool
                    .as_ref()
                    .map(|pool| pool.particles().as_ptr()),
                original
            );
            particles.synchronize_world(None, &mut random);
            assert_eq!(random, expected);
        }
        assert!(!particles.toggle());
        particles.synchronize_world(None, &mut random);
        assert!(!particles.enabled);
        assert_eq!(random, expected);
    }
}

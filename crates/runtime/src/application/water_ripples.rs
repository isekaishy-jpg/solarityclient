//! Unit notifications, native ripple lifetimes, and reusable projected frame banks.

use std::collections::HashMap;
use std::sync::Arc;

use solarity_asset::{AssetError, AssetPath, AssetStore, BlpTextureSource, LiquidTypeCatalog};
use solarity_ecs::{ActiveWorld, WorldObjectIdentity};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, VulkanRenderer, WaterRippleFrame, WaterRipplePass,
    WaterRippleRenderVertex, WorldCameraFrame, water_ripple_surface_transform,
};
use solarity_systems::{
    MovementCollisionBounds, MovementCollisionTriangle, WaterRipple, WaterRippleClock,
    WaterRippleEnvelope, WaterRippleOwner, WaterRipplePool, WaterRippleUnit,
    resolve_unit_movement_speed,
};
use thiserror::Error;

use super::character_directory::RuntimeCharacterMetadata;
use super::unit_water::UnitWaterSample;
use super::{
    RuntimeMovementRegistrationError, RuntimePlayerPresentation, RuntimeStaticMovementError,
    RuntimeTerrainCoordinator,
};
use crate::random::BlizzardRand;

/// A unit registration or scene ripple could not enter its native presentation boundary.
#[derive(Debug, Error)]
pub enum RuntimeWaterRippleError {
    /// The registered native depth-bias CVar is absent or not numeric.
    #[error("footstepBias CVar is unavailable")]
    DepthBiasCvar,
    /// Required archive metadata was invalid.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// Unit area registration failed against retained scene geometry.
    #[error(transparent)]
    Registration(#[from] RuntimeMovementRegistrationError),
    /// A liquid triangle query failed against retained scene geometry.
    #[error(transparent)]
    Geometry(#[from] RuntimeStaticMovementError),
    /// A native ripple query could not form finite bounds.
    #[error(transparent)]
    Bounds(#[from] solarity_systems::MovementCollectionError),
    /// The unit supplied invalid emission scalars.
    #[error(transparent)]
    Emission(#[from] solarity_systems::WaterRippleError),
    /// A ripple lifetime or frame time was invalid.
    #[error(transparent)]
    Envelope(#[from] solarity_systems::WaterRippleEnvelopeError),
    /// A ripple projection could not be formed.
    #[error(transparent)]
    Projection(#[from] solarity_rendering::WaterRippleProjectionError),
    /// Frozen water triangles could not enter the packed vertex stream.
    #[error(transparent)]
    Vertex(#[from] solarity_rendering::WaterRippleVertexError),
    /// The current camera or depth bias was invalid.
    #[error(transparent)]
    Frame(#[from] solarity_rendering::WaterRippleFrameError),
    /// An authored texture could not enter renderer storage.
    #[error(transparent)]
    Texture(#[from] solarity_rendering::BlpTextureUploadError),
    /// Vulkan allocation or submission failed.
    #[error(transparent)]
    Vulkan(#[from] solarity_rendering::VulkanError),
}

/// The scene owns ripple lifetimes; unit clocks follow exact ECS object lifetimes.
pub(super) struct RuntimeWaterRipples {
    world: Option<WorldObjectIdentity>,
    clocks: HashMap<WorldObjectIdentity, WaterRippleClock>,
    pool: WaterRipplePool,
    geometry: Vec<MovementCollisionTriangle>,
    vertices: [Vec<WaterRippleRenderVertex>; 2],
    sources: [Option<Arc<BlpTextureSource>>; 2],
    textures: [Option<BlpTextureHandle>; 2],
}

impl RuntimeWaterRipples {
    /// Preloads the two fixed native requests before interactive world rendering.
    /// Texture.cpp's failure image occupies a failed request's original slot.
    pub(super) fn load(store: &mut AssetStore) -> Result<Self, RuntimeWaterRippleError> {
        let mut sources = [None, None];
        for (index, path) in ["XTextures/splash/splash.blp", "XTextures/splash/wake.blp"]
            .into_iter()
            .enumerate()
        {
            let path = AssetPath::new(path)?;
            sources[index] = match BlpTextureSource::load(store, &path) {
                Ok(source) => Some(Arc::new(source)),
                Err(error) => {
                    tracing::warn!(texture = %path, %error, "ripple texture request failed; using stock green texture");
                    None
                }
            };
        }
        Ok(Self {
            world: None,
            clocks: HashMap::new(),
            pool: WaterRipplePool::default(),
            geometry: Vec::new(),
            vertices: [Vec::new(), Vec::new()],
            sources,
            textures: [None; 2],
        })
    }

    /// A world replacement clears effects; departures retire only unit clocks.
    pub(super) fn synchronize_world(&mut self, world: Option<&ActiveWorld>) {
        let identity = world.and_then(|world| {
            world
                .local_player_guid()
                .ok()
                .and_then(|guid| world.object_identity(guid))
        });
        if self.world != identity {
            self.world = identity;
            self.clocks.clear();
            self.pool = WaterRipplePool::default();
            self.geometry.clear();
            for vertices in &mut self.vertices {
                vertices.clear();
            }
        }
        self.clocks.retain(|identity, _| {
            world.is_some_and(|world| world.object_identity(identity.guid()) == Some(*identity))
        });
    }

    /// Uses movement's existing liquid query and preserves crossing-before-periodic order.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_sample(
        &mut self,
        sample: UnitWaterSample,
        creature_flags: u32,
        world: &ActiveWorld,
        player: &RuntimePlayerPresentation,
        terrain: &mut RuntimeTerrainCoordinator,
        metadata: &RuntimeCharacterMetadata,
        liquids: &LiquidTypeCatalog,
        now_ms: u32,
        scene_time: f32,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeWaterRippleError> {
        // 715D90 passes template bit 22 through 781A10 to registration bit 0x2000.
        if creature_flags & (1 << 22) != 0
            || world.object_identity(sample.identity.guid()) != Some(sample.identity)
        {
            return Ok(());
        }
        let Some(liquid) = sample.liquid else {
            return Ok(());
        };
        let maximum_depth = (sample.height * 2.).max(1.);
        if !sample.splash && liquid.depth >= maximum_depth {
            return Ok(());
        }
        if !sample.splash
            && self.clocks.get(&sample.identity).is_some_and(|clock| {
                clock.next_emission_ms != 0
                    && (now_ms.wrapping_sub(clock.next_emission_ms) as i32) < 0
            })
        {
            return Ok(());
        }
        let position = sample.transform.position();
        if liquid.surface_height
            > player.liquid_registration_top(sample.identity, sample.transform)?
        {
            return Ok(());
        }
        let location = terrain.unit_world_model_location(position)?;
        let Some(flags) = metadata.liquid_flags_at(
            liquids,
            terrain.area_id_at(position),
            location,
            liquid.liquid_type,
        ) else {
            return Ok(());
        };
        // 7A1BC0 publishes the resolved behavior bits only below the admitted
        // surface. Bit 4 permits the native 0.01 epsilon at the interface.
        let permits_ripples = flags & 1 != 0
            && f64::from(position.z) < f64::from(liquid.surface_height) + f64::from(0.01_f32)
            && (flags & 4 != 0 || position.z < liquid.surface_height);
        if !permits_ripples {
            return Ok(());
        }
        let Some(presentation) = world.object_presentation(sample.identity.guid()) else {
            return Ok(());
        };
        let unit = WaterRippleUnit {
            position,
            surface: Some(liquid.surface_height),
            permits_ripples,
            height: sample.height,
            scale: presentation.scale(),
            yaw: sample.transform.orientation(),
            movement_flags: sample.movement.flags() as u32,
            speed: resolve_unit_movement_speed(sample.movement),
            local_player: self.world == Some(sample.identity),
        };
        let notifications: &[u32] = if sample.splash { &[0xc9, 0] } else { &[0] };
        for &notification in notifications {
            let Some(emission) = self.clocks.entry(sample.identity).or_default().emit(
                unit,
                notification,
                now_ms,
                || random.next_u32(),
            )?
            else {
                continue;
            };
            let envelope = WaterRippleEnvelope::new(emission, scene_time)?;
            let [minimum, maximum] = envelope.surface_bounds();
            terrain.collect_water_ripple_surfaces(
                MovementCollisionBounds::new(minimum, maximum)?,
                &mut self.geometry,
            )?;
            let triangles = self
                .geometry
                .iter()
                .map(|triangle| *triangle.vertices())
                .collect();
            self.pool.insert(
                WaterRipple::new(envelope, triangles),
                if emission.local_player {
                    WaterRippleOwner::LocalPlayer
                } else {
                    WaterRippleOwner::OtherUnit
                },
            );
        }
        Ok(())
    }

    /// Advances once for a scene frame and reuses both CPU vertex banks.
    pub(super) fn prepare_frame(
        &mut self,
        renderer: &mut VulkanRenderer,
        elapsed: f32,
        scene_time: f32,
    ) -> Result<(), RuntimeWaterRippleError> {
        self.pool.advance(elapsed, scene_time)?;
        for vertices in &mut self.vertices {
            vertices.clear();
        }
        for ripple in self.pool.ripples() {
            let envelope = ripple.envelope();
            let projection = water_ripple_surface_transform(
                envelope.position(),
                envelope.radius(),
                envelope.yaw(),
            )?;
            WaterRippleRenderVertex::project_into(
                ripple.triangles(),
                projection,
                envelope.opacity(),
                &mut self.vertices[usize::from(envelope.directional())],
            )?;
        }
        for (index, vertices) in self.vertices.iter().enumerate() {
            if !vertices.is_empty() && self.textures[index].is_none() {
                self.textures[index] = Some(match &self.sources[index] {
                    Some(source) => renderer.upload_blp_texture(source, BlpColorSpace::Linear)?,
                    None => renderer.upload_stock_m2_failure()?,
                });
            }
        }
        Ok(())
    }

    /// The world compositor supplies the camera-dependent water ordinal later.
    pub(super) fn frame(
        &self,
        camera: WorldCameraFrame,
        footstep_bias: f32,
    ) -> Result<WaterRippleFrame<'_>, RuntimeWaterRippleError> {
        let passes = std::array::from_fn::<_, 2, _>(|index| {
            self.textures[index].map(|texture| WaterRipplePass::new(texture, &self.vertices[index]))
        });
        Ok(WaterRippleFrame::new(
            camera.view_projection(),
            footstep_bias,
            u32::MAX,
            passes[0],
            passes[1],
        )?)
    }
}

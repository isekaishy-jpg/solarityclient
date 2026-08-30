//! Active-world view distance and authored exterior-light composition.

use glam::Vec3;
use solarity_asset::{
    LightCatalog, WorldLightQuery, WorldLightSample, WorldLightSampleError,
    exterior_light_direction,
};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_systems::{
    DEFAULT_WORLD_VIEW_DISTANCE, WorldViewDistance, WorldViewDistanceError,
    WorldViewDistanceRequest, resolve_world_view_distance,
};
use thiserror::Error;

use crate::time::RealmClock;

/// A failure while deriving one complete stock exterior environment.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum RuntimeWorldEnvironmentError {
    /// Platform startup did not supply a usable physical-memory report.
    #[error("world environment requires a positive physical-memory report")]
    MissingPhysicalMemory,
    /// The active ECS world lost its required local-player transform.
    #[error(transparent)]
    World(#[from] WorldStateError),
    /// The configured far clip could not enter stock's native clamp.
    #[error(transparent)]
    ViewDistance(#[from] WorldViewDistanceError),
    /// Required authored light state was absent or malformed.
    #[error(transparent)]
    Light(#[from] WorldLightSampleError),
}

/// Complete exterior state shared by camera, terrain, sky, fog, water, and models.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeWorldEnvironmentFrame {
    map_id: u32,
    position: Vec3,
    half_minutes: u32,
    view_distance: WorldViewDistance,
    light: WorldLightSample,
    light_direction: Vec3,
}

impl RuntimeWorldEnvironmentFrame {
    /// Returns the active Map.dbc identifier.
    #[must_use]
    pub const fn map_id(self) -> u32 {
        self.map_id
    }

    /// Returns the authoritative local-player position used for light volumes.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns the server-derived cyclic DBC time.
    #[must_use]
    pub const fn half_minutes(self) -> u32 {
        self.half_minutes
    }

    /// Returns the map- and hardware-clamped world viewing distance.
    #[must_use]
    pub const fn view_distance(self) -> WorldViewDistance {
        self.view_distance
    }

    /// Returns the complete joined and blended Light.dbc sample.
    #[must_use]
    pub const fn light(self) -> WorldLightSample {
        self.light
    }

    /// Returns stock's time-derived exterior sun direction.
    #[must_use]
    pub const fn light_direction(self) -> Vec3 {
        self.light_direction
    }
}

/// Owns immutable exterior tables and the process-start hardware policy fact.
pub struct RuntimeWorldEnvironment {
    lights: LightCatalog,
    total_physical_memory_bytes: u64,
    requested_view_distance: f32,
    current: Option<RuntimeWorldEnvironmentFrame>,
}

impl RuntimeWorldEnvironment {
    /// Creates an environment owner with stock's registered `farclip` value.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeWorldEnvironmentError::MissingPhysicalMemory`] rather
    /// than selecting a policy branch without the stock hardware fact.
    pub fn new(
        lights: LightCatalog,
        total_physical_memory_bytes: u64,
    ) -> Result<Self, RuntimeWorldEnvironmentError> {
        if total_physical_memory_bytes == 0 {
            return Err(RuntimeWorldEnvironmentError::MissingPhysicalMemory);
        }
        Ok(Self {
            lights,
            total_physical_memory_bytes,
            requested_view_distance: DEFAULT_WORLD_VIEW_DISTANCE,
            current: None,
        })
    }

    /// Samples one complete environment when world and realm time both exist.
    ///
    /// Absence is a pending state, not a request to use a local clock or generic
    /// light. A later CVar owner can replace `requested_view_distance` directly.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeWorldEnvironmentError`] for invalid world, view, or
    /// authored light state.
    pub fn synchronize(
        &mut self,
        world: Option<&ActiveWorld>,
        clock: Option<&RealmClock>,
    ) -> Result<Option<RuntimeWorldEnvironmentFrame>, RuntimeWorldEnvironmentError> {
        let (Some(world), Some(clock)) = (world, clock) else {
            self.current = None;
            return Ok(None);
        };
        self.current = None;
        let map_id = world.map_id();
        let position = world.local_player_transform()?.position();
        let half_minutes = clock.half_minutes();
        let view_distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
            self.requested_view_distance,
            map_id,
            self.total_physical_memory_bytes,
        ))?;
        let light =
            self.lights
                .sample(WorldLightQuery::new(map_id.value(), position, half_minutes))?;
        let current = RuntimeWorldEnvironmentFrame {
            map_id: map_id.value(),
            position,
            half_minutes,
            view_distance,
            light,
            light_direction: exterior_light_direction(half_minutes),
        };
        self.current = Some(current);
        Ok(Some(current))
    }

    /// Returns the most recently completed environment snapshot.
    #[must_use]
    pub const fn current(&self) -> Option<RuntimeWorldEnvironmentFrame> {
        self.current
    }

    /// Clears world-dependent state while retaining immutable DBC tables.
    pub fn disconnect(&mut self) {
        self.current = None;
    }
}

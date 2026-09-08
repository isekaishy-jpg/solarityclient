//! Active-world view distance and authored exterior-light composition.

mod weather;

use glam::Vec3;
use solarity_asset::{
    LightCatalog, LiquidTypeCatalog, MapCatalog, WorldLightCondition, WorldLightQuery,
    WorldLightSample, WorldLightSampleError, exterior_light_direction_at,
};
use solarity_ecs::{ActiveWorld, WorldStateError};
use solarity_systems::{
    DEFAULT_WORLD_VIEW_DISTANCE, SubmergedLiquid, WorldViewDistance, WorldViewDistanceError,
    WorldViewDistanceRequest, resolve_world_view_distance,
};
use thiserror::Error;

use crate::time::RealmClock;

/// A failure while deriving one complete stock exterior environment.
#[derive(Clone, Copy, Debug, Error, PartialEq)]
pub enum RuntimeWorldEnvironmentError {
    /// The platform CRT could not convert the authoritative realm calendar.
    #[error("world environment cannot convert the realm calendar")]
    Calendar,
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
    /// The admitted camera liquid has no authored environment definition.
    #[error("submerged liquid type {id} has no environment definition")]
    MissingLiquidType {
        /// Raw LiquidType identifier returned by the scene query.
        id: u32,
    },
}

/// Complete exterior state shared by camera, terrain, sky, fog, water, and models.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeWorldEnvironmentFrame {
    map_id: u32,
    position: Vec3,
    half_minutes: u32,
    day_fraction: f32,
    calendar_days: i32,
    weather_blend: f32,
    view_distance: WorldViewDistance,
    light: WorldLightSample,
    light_direction: Vec3,
}

impl RuntimeWorldEnvironmentFrame {
    /// Returns the native precipitation palette and cloud-light attenuation.
    #[must_use]
    pub const fn weather_blend(self) -> f32 {
        self.weather_blend
    }
    /// Returns the native calendar-day provider for the second moon's phase.
    #[must_use]
    pub const fn calendar_days(self) -> i32 {
        self.calendar_days
    }
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

    /// Returns the continuous server-derived day sample used by sky effects.
    #[must_use]
    pub const fn day_fraction(self) -> f32 {
        self.day_fraction
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

    /// Returns stock's DayNight scalar for WMO MOMT additive color.
    #[must_use]
    pub fn world_model_emissive(self) -> f32 {
        world_model_environment_emissive(self.half_minutes)
    }
}

/// Resolves the cyclic DayNight scalar consumed by MapObj material color.
///
/// Stock is fully enabled through 06:00, fades out by 07:00, remains disabled
/// through 20:30, and fades back to full by 21:30.
#[must_use]
pub fn world_model_environment_emissive(half_minutes: u32) -> f32 {
    const DAY_LENGTH: u32 = 24 * 120;
    const DAWN_START: u32 = 6 * 120;
    const DAWN_END: u32 = 7 * 120;
    const DUSK_START: u32 = 20 * 120 + 60;
    const DUSK_END: u32 = 21 * 120 + 60;

    let time = half_minutes % DAY_LENGTH;
    match time {
        ..DAWN_START => 1.0,
        DAWN_START..DAWN_END => (DAWN_END - time) as f32 / (DAWN_END - DAWN_START) as f32,
        DAWN_END..DUSK_START => 0.0,
        DUSK_START..DUSK_END => (time - DUSK_START) as f32 / (DUSK_END - DUSK_START) as f32,
        DUSK_END.. => 1.0,
    }
}

/// Owns immutable exterior tables and the process-start hardware policy fact.
pub struct RuntimeWorldEnvironment {
    lights: LightCatalog,
    total_physical_memory_bytes: u64,
    requested_view_distance: f32,
    current: Option<RuntimeWorldEnvironmentFrame>,
    map_time_overrides: Vec<(u32, i32)>,
    weather_catalog: solarity_asset::WeatherCatalog,
    weather: weather::WeatherTransition,
}

impl RuntimeWorldEnvironment {
    /// Retains the installed Weather.dbc selections for server updates.
    #[must_use]
    pub fn with_weather(mut self, catalog: solarity_asset::WeatherCatalog) -> Self {
        self.weather_catalog = catalog;
        self
    }

    /// Applies the original weather receiver's unknown-row fallback and anchors.
    pub fn receive_weather(&mut self, update: solarity_network::WorldWeatherUpdate, now: u32) {
        let definition = self.weather_catalog.definition(update.weather_id);
        let kind = definition.map_or(0, solarity_asset::WeatherDefinition::precipitation_type);
        let weight = definition.map_or(1., solarity_asset::WeatherDefinition::light_weight);
        self.weather
            .receive(kind, update.grade, update.instant, weight, now);
    }

    /// Retains authored map clock overrides before their catalog enters streaming.
    #[must_use]
    pub fn with_map_time_overrides(mut self, maps: &MapCatalog) -> Self {
        self.map_time_overrides = maps
            .maps()
            .iter()
            .filter_map(|map| {
                map.time_of_day_override()
                    .map(|minutes| (map.id(), minutes))
            })
            .collect();
        self
    }
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
            map_time_overrides: Vec::new(),
            weather_catalog: solarity_asset::WeatherCatalog::default(),
            weather: weather::WeatherTransition::default(),
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
        // 4F8501 uses the camera's followed object's position for light volumes;
        // normal player-follow cameras therefore retain player-space volume weights.
        let position = world.local_player_transform()?.position();
        let mut sky_time = clock
            .sky_time()
            .ok_or(RuntimeWorldEnvironmentError::Calendar)?;
        if let Ok(index) = self
            .map_time_overrides
            .binary_search_by_key(&map_id.value(), |entry| entry.0)
        {
            sky_time = sky_time.with_map_time_override(self.map_time_overrides[index].1);
        }
        let half_minutes = sky_time.half_minutes();
        let view_distance = resolve_world_view_distance(WorldViewDistanceRequest::new(
            self.requested_view_distance,
            map_id,
            self.total_physical_memory_bytes,
        ))?;
        let weather_blend = self.weather.sample(crate::platform::client_milliseconds());
        let light = self.lights.sample(
            WorldLightQuery::new(map_id.value(), position, half_minutes)
                .with_weather(weather_blend),
        )?;
        let current = RuntimeWorldEnvironmentFrame {
            map_id: map_id.value(),
            position,
            half_minutes,
            day_fraction: sky_time.day_fraction(),
            calendar_days: sky_time.calendar_days(),
            weather_blend,
            view_distance,
            light,
            light_direction: exterior_light_direction_at(sky_time.day_fraction()),
        };
        self.current = Some(current);
        Ok(Some(current))
    }

    /// Returns the most recently completed environment snapshot.
    #[must_use]
    pub const fn current(&self) -> Option<RuntimeWorldEnvironmentFrame> {
        self.current
    }

    /// Resolves 7F3230's underwater bank or direct LightParams override after
    /// camera collision and the scene's submerged query have completed.
    ///
    /// # Errors
    /// Returns an error when the admitted liquid or its required light data is absent.
    pub fn resolve_liquid(
        &self,
        mut frame: RuntimeWorldEnvironmentFrame,
        submerged: Option<SubmergedLiquid>,
        liquids: &LiquidTypeCatalog,
    ) -> Result<RuntimeWorldEnvironmentFrame, RuntimeWorldEnvironmentError> {
        let Some(submerged) = submerged else {
            return Ok(frame);
        };
        let liquid = liquids.entry(submerged.liquid_type).ok_or(
            RuntimeWorldEnvironmentError::MissingLiquidType {
                id: submerged.liquid_type,
            },
        )?;
        let light = if liquid.light_id() == 0 {
            self.lights.sample(
                WorldLightQuery::new(frame.map_id, frame.position, frame.half_minutes)
                    .with_condition(WorldLightCondition::UNDERWATER)
                    .with_weather(frame.weather_blend),
            )?
        } else {
            self.lights
                .sample_parameter(liquid.light_id(), frame.half_minutes)?
        };
        frame.light = light.with_liquid_depth(liquid, submerged.depth);
        Ok(frame)
    }

    /// Clears world-dependent state while retaining immutable DBC tables.
    pub fn disconnect(&mut self) {
        self.current = None;
        self.weather = weather::WeatherTransition::default();
    }
}

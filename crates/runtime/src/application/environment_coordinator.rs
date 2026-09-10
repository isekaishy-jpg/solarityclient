//! Active-world view distance and authored exterior-light composition.

mod screen_effect;
mod weather;

#[cfg(test)]
#[path = "../../tests/application/world_model_sky_fade.rs"]
mod sky_fade_tests;

use glam::Vec3;
use solarity_asset::{
    LightCatalog, LiquidTypeCatalog, MapCatalog, WorldFogContext, WorldFogSample,
    WorldLightCondition, WorldLightQuery, WorldLightSample, WorldLightSampleError,
    exterior_light_direction_at,
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
    /// The resolved camera clip could not form a fog context.
    #[error("world environment camera fog clip is invalid")]
    InvalidFogClip,
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
    realm_minute: i32,
    weather_blend: f32,
    view_distance: WorldViewDistance,
    light: WorldLightSample,
    fog_context: WorldFogContext,
    base_fog: WorldFogSample,
    manual_fog: Option<solarity_asset::WorldManualFog>,
    ghost_effect: bool,
    liquid_flags: Option<u32>,
    world_model_skybox_weight: f32,
    fog: WorldFogSample,
    ordinary_fog: WorldFogSample,
    light_direction: Vec3,
}

impl RuntimeWorldEnvironmentFrame {
    /// Returns the realm calendar minute independently of map daylight overrides.
    #[must_use]
    pub const fn realm_minute(self) -> i32 {
        self.realm_minute
    }
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

    /// Returns final camera fog shared by terrain, models, and liquid rendering.
    #[must_use]
    pub const fn fog(self) -> WorldFogSample {
        self.fog
    }

    /// Returns the ordinary model bank retained alongside camera indoor fog.
    /// Both banks share distances and exponent; portal visibility selects color.
    #[must_use]
    pub const fn ordinary_model_fog(self) -> WorldFogSample {
        self.ordinary_fog
    }

    /// Applies camera-owned MFOG banks after the liquid palette is resolved.
    #[must_use]
    pub fn with_world_model_fog(
        mut self,
        environment: Option<solarity_systems::WorldModelFogEnvironment>,
    ) -> Self {
        // DayNight+9C is stored as f32 before 79A870 passes it to 7F31C0.
        // Fog colors retain their independently recovered extended arithmetic.
        self.world_model_skybox_weight =
            world_model_skybox_weight(environment.and_then(|value| value.boundary_distance()));
        if let Some(environment) = environment {
            [self.ordinary_fog, self.fog] = self.fog_context.world_model_scene_banks(
                self.base_fog,
                environment.palette(),
                environment.boundary_distance(),
                self.liquid_flags,
            );
        }
        self
    }

    /// Returns the stored WMO boundary fade shared with its MOSB skybox slot.
    #[must_use]
    pub const fn world_model_skybox_weight(self) -> f32 {
        self.world_model_skybox_weight
    }

    /// Invisibility's manual fog disables all sky drawing and sky animation.
    #[must_use]
    pub const fn sky_enabled(self) -> bool {
        self.manual_fog.is_none()
    }

    /// FFXDeath is selected independently of whether the global light row exists.
    #[must_use]
    pub const fn ghost_effect_enabled(self) -> bool {
        self.ghost_effect
    }

    /// Any camera liquid type suppresses sky drawing, including flag-zero rows.
    #[must_use]
    pub const fn has_camera_liquid(self) -> bool {
        self.liquid_flags.is_some()
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

/// 7F16F0 stores the clamped boundary product before any skybox slot consumes it.
fn world_model_skybox_weight(boundary_distance: Option<f32>) -> f32 {
    boundary_distance.map_or(0., |distance| (distance * 0.04).clamp(0., 1.))
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
    screen_effects: solarity_asset::ScreenEffectCatalog,
    screen_effect: Option<solarity_asset::ScreenEffectDefinition>,
    screen_effect_fog: screen_effect::ScreenEffectFog,
    full_screen_effects: bool,
    death_effects: bool,
}

impl RuntimeWorldEnvironment {
    /// Retains authored world-view declarations alongside the environment tables.
    #[must_use]
    pub fn with_screen_effects(mut self, catalog: solarity_asset::ScreenEffectCatalog) -> Self {
        self.screen_effects = catalog;
        self
    }

    /// An absent declaration clears the native global Light-condition override.
    pub fn select_screen_effect(&mut self, id: u32) {
        self.screen_effect = self.screen_effects.definition(id);
        self.screen_effect_fog.select(
            self.screen_effect.map(|effect| effect.effect_type),
            self.current.map(|frame| frame.fog_context),
            self.full_screen_effects,
        );
    }

    /// Retains the CVar request; synchronization applies native map/memory clamps.
    pub fn set_view_distance(&mut self, requested: f32) {
        self.requested_view_distance = requested;
    }

    /// Captures the native ffx setting for subsequent effect callbacks.
    pub fn set_full_screen_effects(&mut self, enabled: bool) {
        self.full_screen_effects = enabled;
    }

    /// Live ffxDeath policy gates rendering without changing effect selection.
    pub fn set_death_effects(&mut self, enabled: bool) {
        self.death_effects = enabled;
    }
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
            screen_effects: solarity_asset::ScreenEffectCatalog::default(),
            screen_effect: None,
            screen_effect_fog: screen_effect::ScreenEffectFog::default(),
            full_screen_effects: true,
            death_effects: true,
        })
    }

    /// Samples one complete environment when world and realm time both exist.
    ///
    /// Absence is a pending state, not a request to use a local clock or generic
    /// light. The composition root supplies the live farclip CVar request.
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
        let fog_context = WorldFogContext::new(map_id.value(), view_distance.value())
            .ok_or(RuntimeWorldEnvironmentError::InvalidFogClip)?;
        let manual_fog = self.screen_effect_fog.resolve(fog_context);
        let weather_blend = self.weather.sample(crate::platform::client_milliseconds());
        let light = self.lights.sample(
            WorldLightQuery::new(map_id.value(), position, half_minutes)
                .with_weather(weather_blend)
                .with_global_condition(self.screen_effect.and_then(|effect| effect.light_condition))
                .with_fog_context(fog_context),
        )?;
        let base_fog = manual_fog.map_or_else(
            || light.final_fog(fog_context, false),
            |fog| fog_context.resolve_manual_fog(fog, false),
        );
        let current = RuntimeWorldEnvironmentFrame {
            map_id: map_id.value(),
            position,
            half_minutes,
            day_fraction: sky_time.day_fraction(),
            calendar_days: sky_time.calendar_days(),
            realm_minute: clock.whole_minute(),
            weather_blend,
            view_distance,
            light,
            fog_context,
            fog: base_fog,
            ordinary_fog: base_fog,
            base_fog,
            manual_fog,
            ghost_effect: self.screen_effect_fog.ghost()
                && self.full_screen_effects
                && self.death_effects,
            liquid_flags: None,
            world_model_skybox_weight: 0.,
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
                    .with_global_condition(
                        self.screen_effect.and_then(|effect| effect.light_condition),
                    )
                    .with_weather(frame.weather_blend)
                    .with_fog_context(frame.fog_context),
            )?
        } else {
            self.lights.sample_parameter_with_fog(
                liquid.light_id(),
                frame.half_minutes,
                frame.fog_context,
            )?
        };
        // 7F3230 darkens the horizon's working color before 7F0530. The later
        // 7F16F0 scene-fog pass reads the undarkened palette color at D38BF4.
        frame.base_fog = frame.manual_fog.map_or_else(
            || light.final_fog(frame.fog_context, false),
            |fog| frame.fog_context.resolve_manual_fog(fog, false),
        );
        frame.liquid_flags = (submerged.liquid_type != 0).then_some(liquid.flags());
        frame.fog = frame.manual_fog.map_or_else(
            || light.final_fog(frame.fog_context, submerged.liquid_type != 0),
            |fog| {
                frame
                    .fog_context
                    .resolve_manual_fog(fog, submerged.liquid_type != 0)
            },
        );
        frame.ordinary_fog = frame.fog;
        frame.light = light.with_liquid_depth(liquid, submerged.depth);
        Ok(frame)
    }

    /// Clears world-dependent state while retaining immutable DBC tables.
    pub fn disconnect(&mut self) {
        self.current = None;
        self.weather = weather::WeatherTransition::default();
        self.screen_effect = None;
        self.screen_effect_fog = screen_effect::ScreenEffectFog::default();
    }
}

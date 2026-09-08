//! Public light-domain values and private decoded band storage.

use glam::Vec3;

pub(super) const BAND_KEY_COUNT: usize = 16;
pub(super) const COLOR_BAND_COUNT: u32 = 18;
pub(super) const FLOAT_BAND_COUNT: u32 = 6;

/// Ambient and diffuse M2 colors sampled from one LightParams row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelLightColors {
    pub(super) ambient: Vec3,
    pub(super) diffuse: Vec3,
}

impl ModelLightColors {
    /// Returns the unpacked RGB value from color channel one.
    #[must_use]
    pub const fn ambient(self) -> Vec3 {
        self.ambient
    }

    /// Returns the unpacked RGB value from color channel zero.
    #[must_use]
    pub const fn diffuse(self) -> Vec3 {
        self.diffuse
    }
}

/// One positioned or map-global Light.dbc environment volume.
#[derive(Clone, Debug, PartialEq)]
pub struct LightDefinition {
    pub(super) id: u32,
    pub(super) map_id: u32,
    pub(super) is_global: bool,
    pub(super) position: Vec3,
    pub(super) falloff_start: f32,
    pub(super) falloff_end: f32,
    pub(super) parameter_ids: [u32; 8],
}

impl LightDefinition {
    /// Returns the Light.dbc primary key.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the Map.dbc identifier owning this environment volume.
    #[must_use]
    pub const fn map_id(&self) -> u32 {
        self.map_id
    }

    /// Returns whether the authored zero position denotes the map-global light.
    #[must_use]
    pub const fn is_global(&self) -> bool {
        self.is_global
    }

    /// Returns the transformed client-space center for a local light.
    #[must_use]
    pub const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the full-strength local-light radius in world units.
    #[must_use]
    pub const fn falloff_start(&self) -> f32 {
        self.falloff_start
    }

    /// Returns the zero-strength local-light radius in world units.
    #[must_use]
    pub const fn falloff_end(&self) -> f32 {
        self.falloff_end
    }

    /// Returns all eight weather/death/zone parameter slots.
    #[must_use]
    pub const fn parameter_ids(&self) -> &[u32; 8] {
        &self.parameter_ids
    }
}

/// One LightParams.dbc row shared by its 18 color and six scalar bands.
#[derive(Clone, Debug, PartialEq)]
pub struct LightParameter {
    pub(super) id: u32,
    pub(super) highlight_sky: u32,
    pub(super) skybox_id: u32,
    pub(super) cloud_type_id: u32,
    pub(super) glow: f32,
    pub(super) river_shallow_alpha: f32,
    pub(super) river_deep_alpha: f32,
    pub(super) ocean_shallow_alpha: f32,
    pub(super) ocean_deep_alpha: f32,
}

impl LightParameter {
    /// Returns the LightParams.dbc primary key.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the authored sky-highlight flag/value.
    #[must_use]
    pub const fn highlight_sky(&self) -> u32 {
        self.highlight_sky
    }

    /// Returns the optional LightSkybox.dbc identifier.
    #[must_use]
    pub const fn skybox_id(&self) -> u32 {
        self.skybox_id
    }

    /// Returns LightParams field three, retained by the native cloud owner.
    #[must_use]
    pub const fn cloud_type_id(&self) -> u32 {
        self.cloud_type_id
    }

    /// Returns the authored post-process glow amount.
    #[must_use]
    pub const fn glow(&self) -> f32 {
        self.glow
    }

    /// Returns river shallow/deep and ocean shallow/deep alpha values.
    #[must_use]
    pub const fn liquid_alphas(&self) -> [f32; 4] {
        [
            self.river_shallow_alpha,
            self.river_deep_alpha,
            self.ocean_shallow_alpha,
            self.ocean_deep_alpha,
        ]
    }
}

/// One exact three-field LightSkybox.dbc row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LightSkybox {
    pub(super) id: u32,
    pub(super) model_path: String,
    pub(super) flags: u32,
}

impl LightSkybox {
    /// Returns the LightSkybox.dbc primary key.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the authored MDX/M2 environment model path.
    #[must_use]
    pub fn model_path(&self) -> &str {
        &self.model_path
    }

    /// Returns the native sky-slot flags.
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// Valid zero-based selector for one of a Light.dbc row's eight parameter IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorldLightCondition(u8);

impl WorldLightCondition {
    /// Normal exterior weather/death/zone state.
    pub const EXTERIOR: Self = Self(0);

    /// Normal underwater bank selected by 7F3230/7EE510.
    pub const UNDERWATER: Self = Self(1);

    /// Creates a condition only for the closed stock slot range.
    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value < 8 { Some(Self(value)) } else { None }
    }

    /// Returns the zero-based Light.dbc parameter slot.
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    pub(super) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Complete inputs for one exterior environment sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLightQuery {
    pub(super) map_id: u32,
    pub(super) position: Vec3,
    pub(super) half_minutes: u32,
    pub(super) condition: WorldLightCondition,
    pub(super) light_id_override: Option<u32>,
    pub(super) weather_blend: f32,
    pub(super) fog_context: Option<super::WorldFogContext>,
}

impl WorldLightQuery {
    /// Creates a normal-exterior query at the server-derived time of day.
    #[must_use]
    pub const fn new(map_id: u32, position: Vec3, half_minutes: u32) -> Self {
        Self {
            map_id,
            position,
            half_minutes,
            condition: WorldLightCondition::EXTERIOR,
            light_id_override: None,
            weather_blend: 0.0,
            fog_context: None,
        }
    }

    /// Applies the native camera fog conversion to each palette before blending.
    #[must_use]
    pub const fn with_fog_context(mut self, context: super::WorldFogContext) -> Self {
        self.fog_context = Some(context);
        self
    }

    /// Blends the exterior/underwater precipitation bank before local overlays.
    /// Finite values are clamped to zero through one during sampling.
    #[must_use]
    pub const fn with_weather(mut self, weight: f32) -> Self {
        self.weather_blend = weight;
        self
    }

    /// Selects one of the seven alternate authored environment conditions.
    #[must_use]
    pub const fn with_condition(mut self, condition: WorldLightCondition) -> Self {
        self.condition = condition;
        self
    }

    /// Selects one exact Light.dbc row as the complete base environment.
    #[must_use]
    pub const fn with_light_override(mut self, light_id: u32) -> Self {
        self.light_id_override = Some(light_id);
        self
    }
}

/// One skybox contribution after global/local environment blending.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SkyboxBlend {
    pub(super) id: u32,
    pub(super) weight: f32,
}

impl SkyboxBlend {
    /// Returns the LightSkybox.dbc identifier, or zero for an unused slot.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the clamped contribution in the zero-to-one range.
    #[must_use]
    pub const fn weight(self) -> f32 {
        self.weight
    }
}

/// Fully joined and blended exterior environment at one place and time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLightSample {
    pub(super) fog_near: f32,
    pub(super) fog_far: f32,
    pub(super) fog_ratio: f32,
    pub(super) fog_exponent: f32,
    pub(super) fog_color: Vec3,
    pub(super) ambient_color: Vec3,
    pub(super) diffuse_color: Vec3,
    pub(super) specular_color: Vec3,
    pub(super) sky_colors: [Vec3; 5],
    pub(super) additional_colors: [Vec3; 5],
    pub(super) highlight_sky: f32,
    pub(super) glow: f32,
    pub(super) sky_floats: [f32; 4],
    pub(super) liquid_colors: [Vec3; 4],
    pub(super) liquid_alphas: [f32; 4],
    pub(super) skyboxes: [SkyboxBlend; 3],
    pub(super) cloud_type_id: u32,
    pub(super) cloud_type_weight: f32,
}

impl WorldLightSample {
    /// Returns the authored fog start and end distances in world units.
    #[must_use]
    pub const fn fog_range(self) -> (f32, f32) {
        (self.fog_near, self.fog_far)
    }

    /// Returns the blended fog visibility exponent before the camera-liquid factor.
    #[must_use]
    pub const fn fog_exponent(self) -> f32 {
        self.fog_exponent
    }

    /// Returns the linear fog color.
    #[must_use]
    pub const fn fog_color(self) -> Vec3 {
        self.fog_color
    }

    /// Returns global ambient terrain/model illumination.
    #[must_use]
    pub const fn ambient_color(self) -> Vec3 {
        self.ambient_color
    }

    /// Returns global directional terrain/model illumination.
    #[must_use]
    pub const fn diffuse_color(self) -> Vec3 {
        self.diffuse_color
    }

    /// Returns the separate sun/halo/specular color from color channel nine.
    #[must_use]
    pub const fn specular_color(self) -> Vec3 {
        self.specular_color
    }

    /// Returns top through horizon colors from channels two through six.
    #[must_use]
    pub const fn sky_colors(self) -> [Vec3; 5] {
        self.sky_colors
    }

    /// Returns ambient, diffuse and emissive cloud colors (bands 11, 10 and 12).
    #[must_use]
    pub const fn cloud_colors(self) -> [Vec3; 3] {
        [
            self.additional_colors[2],
            self.additional_colors[1],
            self.additional_colors[3],
        ]
    }

    /// Returns one of the 18 authored LightIntBand channels in DBC order.
    /// Channels 8 and 10..13 are retained for sky/cloud presentation.
    #[must_use]
    pub const fn color_channel(self, channel: usize) -> Option<Vec3> {
        Some(match channel {
            0 => self.diffuse_color,
            1 => self.ambient_color,
            2..=6 => self.sky_colors[channel - 2],
            7 => self.fog_color,
            8 => self.additional_colors[0],
            9 => self.specular_color,
            10..=13 => self.additional_colors[channel - 9],
            14..=17 => self.liquid_colors[channel - 14],
            _ => return None,
        })
    }

    /// Returns the last admitted cloud-type identifier and local contribution.
    /// Native 7ED4C0 assigns this pair independently of skybox blend slots.
    #[must_use]
    pub const fn cloud_type(self) -> (u32, f32) {
        (self.cloud_type_id, self.cloud_type_weight)
    }

    /// Returns the blended highlight-sky value.
    #[must_use]
    pub const fn highlight_sky(self) -> f32 {
        self.highlight_sky
    }

    /// Returns the blended post-process glow amount.
    #[must_use]
    pub const fn glow(self) -> f32 {
        self.glow
    }

    /// Returns celestial glow-through, cloud density, and retained channels 4/5.
    #[must_use]
    pub const fn sky_floats(self) -> [f32; 4] {
        self.sky_floats
    }

    /// Returns river shallow/deep followed by ocean shallow/deep colors (bands 14..17).
    #[must_use]
    pub const fn liquid_colors(self) -> [Vec3; 4] {
        self.liquid_colors
    }

    /// Returns ocean shallow/deep followed by river shallow/deep alphas.
    #[must_use]
    pub const fn liquid_alphas(self) -> [f32; 4] {
        self.liquid_alphas
    }

    /// Returns the three bounded skybox blend slots.
    #[must_use]
    pub const fn skyboxes(self) -> [SkyboxBlend; 3] {
        self.skyboxes
    }
}

/// One validated cyclic 16-key LightIntBand or LightFloatBand row.
pub(super) struct LightBand<T> {
    pub(super) entries: usize,
    pub(super) times: [u32; BAND_KEY_COUNT],
    pub(super) values: [T; BAND_KEY_COUNT],
}

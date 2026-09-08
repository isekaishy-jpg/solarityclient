//! Cyclic band interpolation and ordered global/local light blending.

use std::cmp::Ordering;

#[cfg(test)]
#[path = "../../../tests/stock_seed/light_band_native.rs"]
mod tests;

use glam::Vec3;

use super::catalog::LightCatalog;
use super::status::WorldLightSampleError;
use super::types::{
    COLOR_BAND_COUNT, FLOAT_BAND_COUNT, LightBand, LightDefinition, LightParameter,
    ModelLightColors, SkyboxBlend, WorldLightQuery, WorldLightSample,
};

const DAY_HALF_MINUTES: u32 = 2_880;
const CLIENT_COORDINATE_SCALE: f32 = 1.0 / 36.0;
const COINCIDENT_LIGHT_DISTANCE: f32 = 1.0 / 3.0;

const DIRECT_COLOR_CHANNEL: u32 = 0;
const AMBIENT_COLOR_CHANNEL: u32 = 1;
const SKY_COLOR_FIRST_CHANNEL: u32 = 2;
const FOG_COLOR_CHANNEL: u32 = 7;
const SPECULAR_COLOR_CHANNEL: u32 = 9;
const LIQUID_COLOR_CHANNELS: [u32; 4] = [14, 15, 16, 17];

/// Samples the native direct-parameter color path used by M2 callbacks.
pub(super) fn model_light_colors(
    catalog: &LightCatalog,
    parameter_id: u32,
    half_minutes: u32,
) -> Result<ModelLightColors, WorldLightSampleError> {
    let parameter = catalog
        .parameter(parameter_id)
        .ok_or(WorldLightSampleError::MissingParameterId { parameter_id })?;
    // Decoding has already checked that each parameter's band IDs fit u32.
    let first_band = parameter.id() * COLOR_BAND_COUNT - (COLOR_BAND_COUNT - 1);
    Ok(ModelLightColors {
        ambient: sample_color_at(catalog, first_band + AMBIENT_COLOR_CHANNEL, half_minutes)?,
        diffuse: sample_color_at(catalog, first_band + DIRECT_COLOR_CHANNEL, half_minutes)?,
    })
}

/// Resolves one complete sample with the stock map-global fallback.
pub(super) fn sample(
    catalog: &LightCatalog,
    query: WorldLightQuery,
) -> Result<WorldLightSample, WorldLightSampleError> {
    if !query.position.is_finite() {
        return Err(WorldLightSampleError::NonFinitePosition);
    }
    if !query.weather_blend.is_finite() {
        return Err(WorldLightSampleError::NonFiniteWeather);
    }
    let candidates = weighted_lights(catalog, query)?;
    accumulate(catalog, &candidates, query)
}

/// Selects the base global light and applies local overlays farthest-to-nearest.
fn weighted_lights(
    catalog: &LightCatalog,
    query: WorldLightQuery,
) -> Result<Vec<WeightedLight<'_>>, WorldLightSampleError> {
    if let Some(light_id) = query.light_id_override {
        let light = catalog
            .lights
            .iter()
            .find(|light| light.id == light_id)
            .ok_or(WorldLightSampleError::MissingLightOverride { light_id })?;
        require_parameter_id(light, query)?;
        return Ok(vec![WeightedLight { light, weight: 1.0 }]);
    }

    let condition = query.condition.index();
    // 7ECB30 builds a map's light list before selecting a parameter bank.
    // Its last zero-position row wins; absent one, slot zero is Light.dbc
    // ID 1, even when that row belongs to another map (including RFC).
    let global = catalog
        .lights
        .iter()
        .rev()
        .find(|light| light.map_id == query.map_id && light.is_global)
        .or_else(|| catalog.lights.iter().find(|light| light.id == 1))
        .ok_or(WorldLightSampleError::MissingGlobalLight {
            map_id: query.map_id,
            condition: query.condition.value(),
        })?;
    let mut locals = catalog
        .lights
        .iter()
        .filter_map(|light| {
            if light.map_id != query.map_id
                || light.is_global
                || light.parameter_ids[condition] == 0
            {
                return None;
            }
            let distance = query.position.distance(light.position);
            (distance < light.falloff_end).then_some(LocalLight { light, distance })
        })
        .collect::<Vec<_>>();
    // Build 12340 overlays farthest-to-nearest. Co-located volumes use the
    // larger inner radius first, preserving DBC order for equal keys.
    locals.sort_by(|left, right| {
        if left.light.position.distance(right.light.position) < COINCIDENT_LIGHT_DISTANCE {
            right
                .light
                .falloff_start
                .partial_cmp(&left.light.falloff_start)
                .unwrap_or(Ordering::Equal)
        } else {
            right
                .distance
                .partial_cmp(&left.distance)
                .unwrap_or(Ordering::Equal)
        }
    });

    let mut candidates = Vec::with_capacity(locals.len() + 1);
    candidates.push(WeightedLight {
        light: global,
        weight: 1.0,
    });
    for local in locals {
        let weight = local_weight(local);
        candidates.push(WeightedLight {
            light: local.light,
            weight,
        });
    }
    Ok(candidates)
}

/// Computes one local volume's independent inner/outer-radius factor.
fn local_weight(local: LocalLight<'_>) -> f32 {
    if local.distance <= local.light.falloff_start {
        return 1.0;
    }
    let width = local.light.falloff_end - local.light.falloff_start;
    (1.0 - (local.distance - local.light.falloff_start) / width).clamp(0.0, 1.0)
}

/// Accumulates all 18 color, six scalar, parameter, and skybox channels.
fn accumulate(
    catalog: &LightCatalog,
    candidates: &[WeightedLight<'_>],
    query: WorldLightQuery,
) -> Result<WorldLightSample, WorldLightSampleError> {
    let mut output = None;
    for candidate in candidates.iter().filter(|candidate| candidate.weight > 0.0) {
        let parameter_id = require_parameter_id(candidate.light, query)?;
        let parameter = catalog.parameters.get(&parameter_id).ok_or(
            WorldLightSampleError::MissingParameter {
                light_id: candidate.light.id,
                condition: query.condition.value(),
            },
        )?;
        let mut palette =
            parameter_palette(catalog, parameter, query.half_minutes, query.fog_context)?;
        if query.weather_blend > 0.0 && query.condition.index() < 2 {
            let condition = query.condition.index() + 2;
            let parameter = catalog
                .parameters
                .get(&candidate.light.parameter_ids[condition])
                .ok_or(WorldLightSampleError::MissingParameter {
                    light_id: candidate.light.id,
                    condition: condition as u8,
                })?;
            palette.weather(
                parameter_palette(catalog, parameter, query.half_minutes, query.fog_context)?,
                query.weather_blend.min(1.0),
            );
        }
        match &mut output {
            None => output = Some(palette),
            Some(output) => output.overlay(palette, candidate.weight),
        }
    }
    // Every admitted query starts with its required, full-strength global row.
    output
        .map(Accumulator::finish)
        .ok_or(WorldLightSampleError::MissingGlobalLight {
            map_id: query.map_id,
            condition: query.condition.value(),
        })
}

/// Samples a complete LightParams override without a Light.dbc volume.
pub(super) fn sample_parameter(
    catalog: &LightCatalog,
    parameter_id: u32,
    half_minutes: u32,
    fog_context: Option<super::WorldFogContext>,
) -> Result<WorldLightSample, WorldLightSampleError> {
    let parameter = catalog
        .parameter(parameter_id)
        .ok_or(WorldLightSampleError::MissingParameterId { parameter_id })?;
    Ok(parameter_palette(catalog, parameter, half_minutes, fog_context)?.finish())
}

/// Joins the same bands for world volumes and a liquid's direct parameter.
fn parameter_palette(
    catalog: &LightCatalog,
    parameter: &LightParameter,
    half_minutes: u32,
    fog_context: Option<super::WorldFogContext>,
) -> Result<Accumulator, WorldLightSampleError> {
    let parameter_id = parameter.id();
    let color_first = parameter_id * COLOR_BAND_COUNT - (COLOR_BAND_COUNT - 1);
    let float_first = parameter_id * FLOAT_BAND_COUNT - (FLOAT_BAND_COUNT - 1);

    let mut output = Accumulator::default();
    for (index, color) in output.colors.iter_mut().enumerate() {
        *color = sample_packed_color_at(catalog, color_first + index as u32, half_minutes)?;
    }
    output.fog_end = sample_float_at(catalog, float_first, half_minutes)?.max(10.0);
    output.fog_ratio = sample_float_at(catalog, float_first + 1, half_minutes)?.clamp(-1.0, 1.0);
    output.fog_exponent = 1.0;
    if let Some(context) = fog_context {
        (output.fog_end, output.fog_ratio, output.fog_exponent) =
            context.palette(output.fog_end, output.fog_ratio);
    }
    for (index, value) in output.sky_floats.iter_mut().enumerate() {
        *value = sample_float_at(catalog, float_first + 2 + index as u32, half_minutes)?;
    }
    output.highlight_sky = parameter.highlight_sky as i32 as f32;
    output.glow = parameter.glow;
    output.liquid_alphas = [
        parameter.ocean_shallow_alpha,
        parameter.ocean_deep_alpha,
        parameter.river_shallow_alpha,
        parameter.river_deep_alpha,
    ];
    output.skyboxes[0] = SkyboxBlend {
        id: parameter.skybox_id,
        weight: 1.0,
    };
    output.cloud_type_id = parameter.cloud_type_id;
    output.cloud_type_weight = 1.0;
    Ok(output)
}

/// Returns the selected nonzero parameter ID for one candidate.
fn require_parameter_id(
    light: &LightDefinition,
    query: WorldLightQuery,
) -> Result<u32, WorldLightSampleError> {
    let parameter_id = light.parameter_ids[query.condition.index()];
    if parameter_id != 0 {
        return Ok(parameter_id);
    }
    Err(WorldLightSampleError::MissingParameter {
        light_id: light.id,
        condition: query.condition.value(),
    })
}

/// Shares cyclic packed-color sampling with direct model palettes.
fn sample_color_at(
    catalog: &LightCatalog,
    band_id: u32,
    half_minutes: u32,
) -> Result<Vec3, WorldLightSampleError> {
    Ok(color_vector(sample_packed_color_at(
        catalog,
        band_id,
        half_minutes,
    )?))
}

fn sample_packed_color_at(
    catalog: &LightCatalog,
    band_id: u32,
    half_minutes: u32,
) -> Result<u32, WorldLightSampleError> {
    let band = catalog
        .color_bands
        .get(&band_id)
        .ok_or(WorldLightSampleError::MissingColorBand { band_id })?;
    if band.entries == 0 {
        // 0x007EB07B returns packed 0xFF000000 for zero-key rows, including
        // underwater specular band 3826. Padded values are not authored keys.
        return Ok(0xff000000);
    }
    Ok(sample_color_band(band, half_minutes))
}

/// Samples one required scalar band.
fn sample_float_at(
    catalog: &LightCatalog,
    band_id: u32,
    half_minutes: u32,
) -> Result<f32, WorldLightSampleError> {
    let band = catalog
        .float_bands
        .get(&band_id)
        .ok_or(WorldLightSampleError::MissingFloatBand { band_id })?;
    if band.entries == 0 {
        // 0x007EAEFB returns FLDZ before interpolation for a zero-key row.
        return Ok(0.0);
    }
    let scale = if (band_id - 1).is_multiple_of(FLOAT_BAND_COUNT) {
        CLIENT_COORDINATE_SCALE
    } else {
        1.0
    };
    Ok(sample_float_band(band, half_minutes, scale))
}

/// Finds the cyclic key interval, including the final-to-first midnight wrap.
fn band_interval<T>(band: &LightBand<T>, half_minutes: u32) -> (usize, usize, f32) {
    if band.entries == 1 {
        return (0, 0, 0.0);
    }
    let query = half_minutes % DAY_HALF_MINUTES;
    for current in 0..band.entries {
        let next = (current + 1) % band.entries;
        let start = band.times[current];
        let mut end = band.times[next];
        let mut adjusted = query;
        if next == 0 || end <= start {
            end += DAY_HALF_MINUTES;
        }
        if adjusted < start {
            adjusted += DAY_HALF_MINUTES;
        }
        if adjusted < start || adjusted > end {
            continue;
        }
        let amount = if end == start {
            0.0
        } else {
            (f64::from(adjusted - start) / f64::from(end - start)) as f32
        };
        return (current, next, amount);
    }
    // Valid stock bands cover the cyclic day. If custom keys leave a gap,
    // build 12340 returns the first authored value after its interval scan.
    (0, 0, 0.0)
}

/// Linearly samples a scalar band.
fn sample_float_band(band: &LightBand<f32>, half_minutes: u32, scale: f32) -> f32 {
    let (left, right, amount) = band_interval(band, half_minutes);
    let left = f64::from(band.values[left]) * f64::from(scale);
    let right = f64::from(band.values[right]) * f64::from(scale);
    (left + (right - left) * f64::from(amount)) as f32
}

/// Interpolates packed colors per channel and applies stock integer rounding.
fn sample_color_band(band: &LightBand<u32>, half_minutes: u32) -> u32 {
    let (left, right, amount) = band_interval(band, half_minutes);
    let left = unpack_color(band.values[left]);
    let right = unpack_color(band.values[right]);
    let component = |index: usize| {
        ((f64::from(left[index]) + f64::from(right[index] - left[index]) * f64::from(amount))
            as f32)
            .round_ties_even()
            .clamp(0.0, 255.0) as u32
    };
    0xff000000 | (component(0) << 16) | (component(1) << 8) | component(2)
}

/// Expands one packed 0xRRGGBB value for component interpolation.
fn unpack_color(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        (color & 0xff) as f32,
    ]
}

/// Adds or merges one nonzero skybox within the native three-slot bound.
fn add_skybox(skyboxes: &mut [SkyboxBlend; 3], id: u32, weight: f32) {
    if id == 0 {
        return;
    }
    if let Some(slot) = skyboxes
        .iter_mut()
        .find(|slot| slot.id == id || slot.id == 0)
    {
        if slot.id == 0 {
            slot.id = id;
        }
        slot.weight = (slot.weight + weight).min(1.0);
    }
}

#[derive(Clone, Copy)]
struct LocalLight<'a> {
    light: &'a LightDefinition,
    distance: f32,
}

#[derive(Clone, Copy)]
struct WeightedLight<'a> {
    light: &'a LightDefinition,
    weight: f32,
}

#[derive(Default)]
struct Accumulator {
    colors: [u32; 18],
    fog_end: f32,
    fog_ratio: f32,
    fog_exponent: f32,
    highlight_sky: f32,
    glow: f32,
    sky_floats: [f32; 4],
    liquid_alphas: [f32; 4],
    skyboxes: [SkyboxBlend; 3],
    cloud_type_id: u32,
    cloud_type_weight: f32,
}

impl Accumulator {
    /// Native 7EC220 quantizes weather opacity before integer RGB blending.
    fn weather(&mut self, other: Self, weight: f32) {
        let alpha = (weight * 255.0).round_ties_even() as i32;
        for (current, next) in self.colors.iter_mut().zip(other.colors) {
            if alpha == 255 {
                *current = (*current & 0xff000000) | (next & 0xffffff);
            } else if alpha != 0 {
                let mut blended = *current & 0xff000000;
                for shift in [0, 8, 16] {
                    let from = ((*current >> shift) & 255) as i32;
                    let to = ((next >> shift) & 255) as i32;
                    blended |= ((from + (((to - from) * alpha) >> 8)) as u32 & 255) << shift;
                }
                *current = blended;
            }
        }
        self.fog_end = overlay_float(self.fog_end, other.fog_end, weight);
        self.fog_ratio = overlay_float(self.fog_ratio, other.fog_ratio, weight);
        self.fog_exponent = overlay_float(self.fog_exponent, other.fog_exponent, weight);
        self.glow = overlay_float(self.glow, other.glow, weight);
        self.sky_floats[1] = overlay_float(self.sky_floats[1], other.sky_floats[1], weight);
        for (current, next) in self.liquid_alphas.iter_mut().zip(other.liquid_alphas) {
            *current = overlay_float(*current, next, weight);
        }
    }

    /// Native 7ED4C0 blends each admitted local into the current packed palette.
    fn overlay(&mut self, local: Self, weight: f32) {
        for (current, next) in self.colors.iter_mut().zip(local.colors) {
            *current = overlay_color(*current, next, weight);
        }
        self.fog_end = overlay_float(self.fog_end, local.fog_end, weight);
        self.fog_ratio = overlay_float(self.fog_ratio, local.fog_ratio, weight);
        self.fog_exponent = overlay_float(self.fog_exponent, local.fog_exponent, weight);
        self.highlight_sky = overlay_float(self.highlight_sky, local.highlight_sky, weight);
        self.glow = overlay_float(self.glow, local.glow, weight);
        // Only cloud density is locally blended. Native retains global bands
        // 2, 4 and 5 (glow-through and two reserved scalars) unchanged.
        self.sky_floats[1] = overlay_float(self.sky_floats[1], local.sky_floats[1], weight);
        for (current, next) in self.liquid_alphas.iter_mut().zip(local.liquid_alphas) {
            *current = overlay_float(*current, next, weight);
        }
        add_skybox(&mut self.skyboxes, local.skyboxes[0].id, weight);
        self.cloud_type_id = local.cloud_type_id;
        self.cloud_type_weight = weight;
    }

    /// Expands the final packed channels once, after ordered quantization.
    fn finish(self) -> WorldLightSample {
        let fog_far = self.fog_end;
        WorldLightSample {
            fog_near: fog_far * self.fog_ratio,
            fog_far,
            fog_ratio: self.fog_ratio,
            fog_exponent: self.fog_exponent,
            fog_color: color_vector(self.colors[FOG_COLOR_CHANNEL as usize]),
            ambient_color: color_vector(self.colors[AMBIENT_COLOR_CHANNEL as usize]),
            diffuse_color: color_vector(self.colors[DIRECT_COLOR_CHANNEL as usize]),
            specular_color: color_vector(self.colors[SPECULAR_COLOR_CHANNEL as usize]),
            sky_colors: std::array::from_fn(|i| {
                color_vector(self.colors[SKY_COLOR_FIRST_CHANNEL as usize + i])
            }),
            additional_colors: [8, 10, 11, 12, 13].map(|i| color_vector(self.colors[i])),
            highlight_sky: self.highlight_sky,
            glow: self.glow,
            sky_floats: self.sky_floats,
            liquid_colors: LIQUID_COLOR_CHANNELS.map(|i| color_vector(self.colors[i as usize])),
            liquid_alphas: self.liquid_alphas,
            skyboxes: self.skyboxes,
            cloud_type_id: self.cloud_type_id,
            cloud_type_weight: self.cloud_type_weight,
        }
    }
}

fn color_vector(color: u32) -> Vec3 {
    Vec3::from_array(unpack_color(color)) / 255.0
}

fn overlay_float(current: f32, next: f32, weight: f32) -> f32 {
    let (current, next, weight) = (f64::from(current), f64::from(next), f64::from(weight));
    if next < current {
        (current - (current - next) * weight) as f32
    } else {
        (current + (next - current) * weight) as f32
    }
}

fn overlay_color(current: u32, next: u32, weight: f32) -> u32 {
    let channel = |shift: u32| {
        let from = ((current >> shift) & 255_u32) as f32;
        let to = ((next >> shift) & 255_u32) as f32;
        // 7ED2D0 stores the interpolated float, subtracts AF4B78 (0.5),
        // then FISTP rounds to nearest-even and retains the low byte.
        let value = f64::from(overlay_float(from, to, weight));
        ((value - 0.5).round_ties_even() as i32 as u8) as u32
    };
    0xff000000 | (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

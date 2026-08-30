//! Cyclic band interpolation and ordered global/local light blending.

use std::cmp::Ordering;

use glam::Vec3;

use super::catalog::LightCatalog;
use super::status::WorldLightSampleError;
use super::types::{
    COLOR_BAND_COUNT, FLOAT_BAND_COUNT, LightBand, LightDefinition, SkyboxBlend, WorldLightQuery,
    WorldLightSample,
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

/// Resolves one complete sample without substituting missing global state.
pub(super) fn sample(
    catalog: &LightCatalog,
    query: WorldLightQuery,
) -> Result<WorldLightSample, WorldLightSampleError> {
    if !query.position.is_finite() {
        return Err(WorldLightSampleError::NonFinitePosition);
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
    let global = catalog
        .lights
        .iter()
        .find(|light| {
            light.map_id == query.map_id && light.is_global && light.parameter_ids[condition] != 0
        })
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
        for accumulated in &mut candidates {
            accumulated.weight *= 1.0 - weight;
        }
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
    let mut output = Accumulator::default();
    for candidate in candidates.iter().filter(|candidate| candidate.weight > 0.0) {
        let parameter_id = require_parameter_id(candidate.light, query)?;
        let parameter = catalog.parameters.get(&parameter_id).ok_or(
            WorldLightSampleError::MissingParameter {
                light_id: candidate.light.id,
                condition: query.condition.value(),
            },
        )?;
        let color_first = parameter_id * COLOR_BAND_COUNT - (COLOR_BAND_COUNT - 1);
        let float_first = parameter_id * FLOAT_BAND_COUNT - (FLOAT_BAND_COUNT - 1);
        let weight = candidate.weight;

        output.total += weight;
        output.diffuse +=
            sample_color(catalog, color_first + DIRECT_COLOR_CHANNEL, query)? * weight;
        output.ambient +=
            sample_color(catalog, color_first + AMBIENT_COLOR_CHANNEL, query)? * weight;
        output.fog += sample_color(catalog, color_first + FOG_COLOR_CHANNEL, query)? * weight;
        output.specular +=
            sample_color(catalog, color_first + SPECULAR_COLOR_CHANNEL, query)? * weight;
        for (index, color) in output.sky.iter_mut().enumerate() {
            *color += sample_color(
                catalog,
                color_first + SKY_COLOR_FIRST_CHANNEL + index as u32,
                query,
            )? * weight;
        }
        for (index, color) in output.liquid.iter_mut().enumerate() {
            *color +=
                sample_color(catalog, color_first + LIQUID_COLOR_CHANNELS[index], query)? * weight;
        }

        let fog_end = sample_float(catalog, float_first, query)? * CLIENT_COORDINATE_SCALE;
        output.fog_end += fog_end.max(10.0) * weight;
        output.fog_ratio += sample_float(catalog, float_first + 1, query)?.clamp(0.0, 1.0) * weight;
        for (index, value) in output.sky_floats.iter_mut().enumerate() {
            *value += sample_float(catalog, float_first + 2 + index as u32, query)? * weight;
        }
        output.highlight_sky += parameter.highlight_sky as f32 * weight;
        output.glow += parameter.glow * weight;
        output.liquid_alphas[0] += parameter.ocean_shallow_alpha * weight;
        output.liquid_alphas[1] += parameter.ocean_deep_alpha * weight;
        output.liquid_alphas[2] += parameter.river_shallow_alpha * weight;
        output.liquid_alphas[3] += parameter.river_deep_alpha * weight;
        add_skybox(&mut output.skyboxes, parameter.skybox_id, weight);
    }
    Ok(output.finish())
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

/// Samples and unpacks one required packed-RGB band.
fn sample_color(
    catalog: &LightCatalog,
    band_id: u32,
    query: WorldLightQuery,
) -> Result<Vec3, WorldLightSampleError> {
    let band = catalog
        .color_bands
        .get(&band_id)
        .ok_or(WorldLightSampleError::MissingColorBand { band_id })?;
    if band.entries == 0 {
        return Err(WorldLightSampleError::EmptyColorBand { band_id });
    }
    let color = sample_color_band(band, query.half_minutes);
    Ok(Vec3::new(
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        (color & 0xff) as f32,
    ) / 255.0)
}

/// Samples one required scalar band.
fn sample_float(
    catalog: &LightCatalog,
    band_id: u32,
    query: WorldLightQuery,
) -> Result<f32, WorldLightSampleError> {
    let band = catalog
        .float_bands
        .get(&band_id)
        .ok_or(WorldLightSampleError::MissingFloatBand { band_id })?;
    if band.entries == 0 {
        return Err(WorldLightSampleError::EmptyFloatBand { band_id });
    }
    Ok(sample_float_band(band, query.half_minutes))
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
            (adjusted - start) as f32 / (end - start) as f32
        };
        return (current, next, amount);
    }
    // Valid stock bands cover the cyclic day. If custom keys leave a gap,
    // build 12340 returns the first authored value after its interval scan.
    (0, 0, 0.0)
}

/// Linearly samples a scalar band.
fn sample_float_band(band: &LightBand<f32>, half_minutes: u32) -> f32 {
    let (left, right, amount) = band_interval(band, half_minutes);
    band.values[left] + (band.values[right] - band.values[left]) * amount
}

/// Interpolates packed colors per channel and applies stock integer rounding.
fn sample_color_band(band: &LightBand<u32>, half_minutes: u32) -> u32 {
    let (left, right, amount) = band_interval(band, half_minutes);
    let left = unpack_color(band.values[left]);
    let right = unpack_color(band.values[right]);
    let component = |index: usize| {
        (left[index] + (right[index] - left[index]) * amount)
            .round()
            .clamp(0.0, 255.0) as u32
    };
    (component(0) << 16) | (component(1) << 8) | component(2)
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
    total: f32,
    fog_end: f32,
    fog_ratio: f32,
    fog: Vec3,
    ambient: Vec3,
    diffuse: Vec3,
    specular: Vec3,
    sky: [Vec3; 5],
    highlight_sky: f32,
    glow: f32,
    sky_floats: [f32; 4],
    liquid: [Vec3; 4],
    liquid_alphas: [f32; 4],
    skyboxes: [SkyboxBlend; 3],
}

impl Accumulator {
    /// Normalizes independently weighted channels into the public snapshot.
    fn finish(self) -> WorldLightSample {
        let inverse = 1.0 / self.total;
        let fog_far = self.fog_end * inverse;
        WorldLightSample {
            fog_near: fog_far * self.fog_ratio * inverse,
            fog_far,
            fog_color: self.fog * inverse,
            ambient_color: self.ambient * inverse,
            diffuse_color: self.diffuse * inverse,
            specular_color: self.specular * inverse,
            sky_colors: self.sky.map(|color| color * inverse),
            highlight_sky: self.highlight_sky * inverse,
            glow: self.glow * inverse,
            sky_floats: self.sky_floats.map(|value| value * inverse),
            liquid_colors: self.liquid.map(|color| color * inverse),
            liquid_alphas: self.liquid_alphas.map(|alpha| alpha * inverse),
            skyboxes: self.skyboxes,
        }
    }
}

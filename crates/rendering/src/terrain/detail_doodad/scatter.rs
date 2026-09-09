//! Original 7D3390 scatter order, face admission, tint, and shadow sampling.

use glam::Vec3;
use solarity_asset::{GroundEffectCatalog, GroundEffectTexture, TerrainChunk};
use solarity_cpu::BlizzardRand;

use super::placement::{GroundDetailDensity, GroundDetailError, GroundDetailPlacement};

const UNIT: f32 = 4.166_666_5;
const HALF_UNIT: f32 = 2.083_333_3;
const CORNERS: [(usize, usize); 4] = [(17, 0), (0, 1), (18, 17), (1, 18)];

/// Preserves separate cell-selection and placement phases so RNG use is exact.
pub(super) fn scatter(
    chunk: &TerrainChunk,
    catalog: &GroundEffectCatalog,
    seed: u32,
    density: GroundDetailDensity,
) -> Result<Vec<GroundDetailPlacement>, GroundDetailError> {
    let mut random = BlizzardRand::new(seed);
    let cells = (0..density.cells())
        .map(|_| [(random.next_u32() & 7) as u8, (random.next_u32() & 7) as u8])
        .collect::<Vec<_>>();
    let mut placements = Vec::new();
    for (attempt, [column, row]) in cells.into_iter().enumerate() {
        let Some(effect) = chunk
            .detail_effect_at(column, row)
            .and_then(|id| catalog.texture(id))
        else {
            continue;
        };
        let models = distribution(effect);
        let planes = std::array::from_fn::<_, 4, _>(|face| plane(chunk, column, row, face));
        for instance in 0..effect.density() {
            let signed = [next_signed(&mut random), next_signed(&mut random)];
            let offsets = signed.map(|value| value * HALF_UNIT + HALF_UNIT);
            let mut position = [-offsets[1], -offsets[0], 0.0];
            let face = usize::from(position[1] - position[0] < 0.0)
                + 2 * usize::from((-position[1] - UNIT) - position[0] > 0.0);
            let model = models[(instance as usize + attempt) & 15];
            let (normal, distance) = planes[face];
            if model == 0 || normal.z < 0.4 {
                continue;
            }
            let definition = catalog
                .doodad(model)
                .ok_or(GroundDetailError::MissingDoodad(model))?;
            let mut color = if definition.flags() & 2 == 0 {
                tint(chunk, column, row, face, signed, position)
            } else {
                [255; 4]
            };
            let sample = [
                offsets[0] + f32::from(column) * UNIT,
                offsets[1] + f32::from(row) * UNIT,
            ];
            if chunk.shadow_map().is_some_and(|shadow| {
                shadow.authored_shadow_at(
                    (sample[0] * 1.92).floor() as usize,
                    (sample[1] * 1.92).floor() as usize,
                ) == Some(true)
            }) {
                color[3] = 0;
            }
            position[0] -= f32::from(row) * UNIT;
            position[1] -= f32::from(column) * UNIT;
            position[2] =
                -((position[1] * normal.y + normal.x * position[0] + distance) / normal.z);
            placements.push(GroundDetailPlacement {
                model,
                position,
                angle: (next_signed(&mut random) + 1.0) * std::f32::consts::PI,
                scale: next_signed(&mut random) * 0.33 + 1.0,
                normal: normal.to_array(),
                face: ((usize::from(column) + usize::from(row) * 8) * 4 + face) as u16,
                color,
            });
        }
    }
    Ok(placements)
}

/// Writes weighted slots with the native thirteen-step permutation and padding.
fn distribution(effect: GroundEffectTexture) -> [u32; 16] {
    let mut result = [0; 16];
    let mut slot = 0;
    let mut count = 0;
    let models = effect.models();
    for (model, weight) in models.into_iter().zip(effect.weights()) {
        for _ in 0..weight {
            result[slot & 15] = model;
            slot += 13;
            count += 1;
        }
    }
    for index in count..16 {
        result[slot & 15] = models[index & 3];
        slot += 13;
    }
    result
}

/// Returns the bit-constructed signed variate consumed by native scatter.
fn next_signed(random: &mut BlizzardRand) -> f32 {
    let word = random.next_u32();
    let magnitude = f32::from_bits(word & 0x7f_ffff | 0x3f80_0000);
    if word & 0x8000_0000 != 0 {
        2.0 - magnitude
    } else {
        magnitude - 2.0
    }
}

/// Builds the native center/corner/corner plane in chunk-local coordinates.
fn plane(chunk: &TerrainChunk, column: u8, row: u8, face: usize) -> (Vec3, f32) {
    let base = usize::from(row) * 17 + usize::from(column);
    let origin = Vec3::new(-f32::from(row) * UNIT, -f32::from(column) * UNIT, 0.0);
    let center = origin + Vec3::new(-HALF_UNIT, -HALF_UNIT, chunk.heights()[base + 9]);
    let (first, second) = CORNERS[face];
    let vertex = |index: usize| {
        origin
            + Vec3::new(
                -((index / 17) as f32) * UNIT,
                -((index % 17) as f32) * UNIT,
                chunk.heights()[base + index],
            )
    };
    let normal = (vertex(second) - center)
        .cross(vertex(first) - center)
        .normalize();
    (
        normal,
        -(center.y * normal.y + center.x * normal.x + center.z * normal.z),
    )
}

/// Applies native triangle tint interpolation and its doubled MCCV byte range.
fn tint(
    chunk: &TerrainChunk,
    column: u8,
    row: u8,
    face: usize,
    signed: [f32; 2],
    position: [f32; 3],
) -> [u8; 4] {
    let Some(colors) = chunk.vertex_colors_bgra() else {
        return [255; 4];
    };
    let base = usize::from(row) * 17 + usize::from(column);
    let (first, second) = CORNERS[face];
    let (major, minor) = if signed[1].abs() < signed[0].abs() {
        (signed[0].abs(), signed[1])
    } else {
        (signed[1].abs(), signed[0])
    };
    let mut factor = 0.5 - minor * 0.5;
    if position[1] - position[0] < 0.0 {
        factor = 1.0 - factor;
    }
    factor *= major;
    let mut result = [255; 4];
    for channel in 0..3 {
        let center = f32::from(colors[base + 9][channel]);
        let first = f32::from(colors[base + first][channel]);
        let second = f32::from(colors[base + second][channel]);
        let value = center * 2.0 + (first - center) * 2.0 * major + (second - first) * 2.0 * factor;
        result[2 - channel] = (value.min(255.0) - 0.5).round_ties_even() as u8;
    }
    result
}

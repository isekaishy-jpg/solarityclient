//! Original 7D6810 pairing, retained for the shared scene-light query bounds.

use super::types::TerrainChunkDrawPlan;
use glam::{Vec3, Vec4};

#[derive(Clone, Copy, Default)]
struct BatchLayer {
    texture: u32,
    flags: u32,
}

pub(super) fn assign(chunks: &mut [TerrainChunkDrawPlan]) {
    let mut lookup = [usize::MAX; 256];
    for (index, chunk) in chunks.iter().enumerate() {
        lookup[usize::from(chunk.chunk().y()) * 16 + usize::from(chunk.chunk().x())] = index;
    }
    for y in (0..16).step_by(2) {
        for x in (0..16).step_by(2) {
            // Native order: origin, Y neighbor, X neighbor, diagonal.
            let indices = [
                y * 16 + x,
                (y + 1) * 16 + x,
                y * 16 + x + 1,
                (y + 1) * 16 + x + 1,
            ]
            .map(|i| lookup[i]);
            if indices.contains(&usize::MAX) {
                continue;
            }
            let mut materials = [[BatchLayer::default(); 4]; 4];
            let mut counts = [0; 4];
            for (slot, index) in indices.into_iter().enumerate() {
                let layers = chunks[index].layers();
                counts[slot] = layers.len();
                for (target, layer) in materials[slot].iter_mut().zip(layers) {
                    *target = BatchLayer {
                        texture: layer.texture_index(),
                        flags: layer.flags(),
                    };
                }
            }
            let layers = std::array::from_fn(|i| &materials[i][..counts[i]]);
            let partners = pairing(chunks[indices[0]].uses_weighted_blending(), layers);
            for (slot, index) in indices.into_iter().enumerate() {
                let first = chunks[index].bounds().map(Vec3::from_array);
                let second = chunks[indices[partners[slot]]]
                    .bounds()
                    .map(Vec3::from_array);
                chunks[index].set_point_light_bounds([
                    first[0].min(second[0]).to_array(),
                    first[1].max(second[1]).to_array(),
                ]);
            }
        }
    }
}

fn pairing(weighted: bool, layers: [&[BatchLayer]; 4]) -> [usize; 4] {
    let vertical = [(0, 1), (2, 3)].map(|(a, b)| compatible(weighted, layers[a], layers[b]));
    let horizontal = [(0, 2), (1, 3)].map(|(a, b)| compatible(weighted, layers[a], layers[b]));
    // Ties select the X-neighbor pairs. Each pair still requires acceptance.
    let (pairs, accepted) = if vertical.map(usize::from).iter().sum::<usize>()
        > horizontal.map(usize::from).iter().sum::<usize>()
    {
        ([(0, 1), (2, 3)], vertical)
    } else {
        ([(0, 2), (1, 3)], horizontal)
    };
    let mut partners = [0, 1, 2, 3];
    for ((first, second), accepted) in pairs.into_iter().zip(accepted) {
        if accepted {
            partners[first] = second;
            partners[second] = first;
        }
    }
    partners
}

fn compatible(weighted: bool, first: &[BatchLayer], second: &[BatchLayer]) -> bool {
    if !weighted {
        return first.len() == second.len()
            && first
                .iter()
                .zip(second)
                .all(|(a, b)| (a.flags | b.flags) & 0xc0 == 0 && a.texture == b.texture);
    }
    let mut count = first.len();
    for layer in second {
        if layer.flags & 0x4c0 != 0 {
            return false;
        }
        let mut found = false;
        for candidate in first {
            // Native short-circuits at the first matching texture, including
            // its flag check; later first-chunk layers are not visited then.
            if candidate.flags & 0x4c0 != 0 {
                return false;
            }
            if candidate.texture == layer.texture {
                found = true;
                break;
            }
        }
        if !found {
            count += 1;
        }
    }
    count < 5
}

pub(super) fn sphere(bounds: [[f32; 3]; 2]) -> Vec4 {
    let [minimum, maximum] = bounds.map(|v| Vec3::from_array(v).as_dvec3());
    let extent = maximum - minimum;
    let radius =
        ((extent.z * extent.z + extent.y * extent.y + extent.x * extent.x).sqrt() * 0.5) as f32;
    ((minimum + maximum) * 0.5).as_vec3().extend(radius)
}

#[cfg(test)]
#[path = "../../../tests/stock_seed/terrain_light_batches.rs"]
mod tests;

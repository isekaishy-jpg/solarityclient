//! Native closest-box loading priority and CRT equal-key ordering.

use glam::Vec3;
use solarity_asset::TerrainTileIndex;

use super::TerrainStreamingError;

/// Sorts declared ADTs by stock's squared XY distance to their bounding boxes.
///
/// The caller supplies the residency owner's existing order. Equal-distance
/// tiles retain the result of the original CRT partition/short-sort algorithm;
/// replacing it with a stable sort changes native request order at boundaries.
///
/// # Errors
/// Returns [`TerrainStreamingError::InvalidCamera`] for a non-finite origin.
pub fn prioritize_terrain_tiles(
    tiles: &mut [TerrainTileIndex],
    origin: Vec3,
) -> Result<(), TerrainStreamingError> {
    if !origin.is_finite() {
        return Err(TerrainStreamingError::InvalidCamera);
    }
    let mut ordered = tiles
        .iter()
        .map(|&tile| (tile, distance_squared(tile, origin)))
        .collect::<Vec<_>>();
    if !ordered.is_empty() {
        let last = ordered.len() - 1;
        sort(&mut ordered, 0, last);
    }
    for (target, (tile, _)) in tiles.iter_mut().zip(ordered) {
        *target = tile;
    }
    Ok(())
}

/// `0x007D9A70` prepares each box; `0x007B4830` measures its closest point.
fn distance_squared(tile: TerrainTileIndex, origin: Vec3) -> f32 {
    let corner = [tile.y(), tile.x()].map(|index| {
        f64::from(i32::from(index) * 16) * f64::from(-33.333_332_f32) + f64::from(17_066.666_f32)
    });
    let maximum = corner.map(|v| v as f32);
    let minimum = corner.map(|v| (v - f64::from(533.333_3_f32)) as f32);
    let delta = [origin.x, origin.y].map(f64::from);
    let x = delta[0].clamp(f64::from(minimum[0]), f64::from(maximum[0])) - delta[0];
    let y = delta[1].clamp(f64::from(minimum[1]), f64::from(maximum[1])) - delta[1];
    (y * y + x * x) as f32
}

/// Index-based transcription of `0x0040BE50`, including its eight-item cutoff.
fn sort(values: &mut [(TerrainTileIndex, f32)], mut low: usize, mut high: usize) {
    while low < high {
        let count = high - low + 1;
        if count <= 8 {
            // Native shortsort repeatedly exchanges the first maximum with
            // the last slot. Equal keys are intentionally not stable.
            while low < high {
                let mut maximum = low;
                for index in low + 1..=high {
                    if values[index].1 > values[maximum].1 {
                        maximum = index;
                    }
                }
                values.swap(maximum, high);
                high -= 1;
            }
            return;
        }
        let mut middle = low + count / 2;
        if values[low].1 > values[middle].1 {
            values.swap(low, middle);
        }
        if values[low].1 > values[high].1 {
            values.swap(low, high);
        }
        if values[middle].1 > values[high].1 {
            values.swap(middle, high);
        }
        let mut left = low;
        let mut right = high;
        loop {
            if left < middle {
                loop {
                    left += 1;
                    if left >= middle || values[left].1 > values[middle].1 {
                        break;
                    }
                }
            }
            if left >= middle {
                loop {
                    left += 1;
                    if left > high || values[left].1 > values[middle].1 {
                        break;
                    }
                }
            }
            loop {
                right -= 1;
                if right <= middle || values[right].1 <= values[middle].1 {
                    break;
                }
            }
            if left > right {
                break;
            }
            values.swap(left, right);
            if middle == right {
                middle = left;
            }
        }
        right += 1;
        if middle < right {
            loop {
                right -= 1;
                if right <= middle || values[right].1 != values[middle].1 {
                    break;
                }
            }
        }
        if right <= middle {
            loop {
                if right == low {
                    break;
                }
                right -= 1;
                if right <= low || values[right].1 != values[middle].1 {
                    break;
                }
            }
        }
        // Recurse only on the smaller side; the larger side remains iterative.
        if right - low < high.saturating_sub(left) {
            if low < right {
                sort(values, low, right);
            }
            low = left;
        } else {
            if left < high {
                sort(values, left, high);
            }
            high = right;
        }
    }
}

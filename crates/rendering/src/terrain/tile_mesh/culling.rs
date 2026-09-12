//! Coarse rejection that encloses the existing per-chunk floating-point tests.

use glam::Vec3;

use super::TerrainChunkDrawPlan;
use crate::WorldFrustum;

#[cfg(test)]
#[path = "../../../tests/stock_seed/terrain_tile_culling.rs"]
mod tests;

#[derive(Clone, Copy)]
pub(super) struct TerrainTileCulling {
    minimum_center: Vec3,
    maximum_center: Vec3,
    maximum_half: Vec3,
}

impl TerrainTileCulling {
    pub(super) fn new(chunks: &[TerrainChunkDrawPlan]) -> Option<Self> {
        let mut result = Self {
            minimum_center: Vec3::splat(f32::INFINITY),
            maximum_center: Vec3::splat(f32::NEG_INFINITY),
            maximum_half: Vec3::ZERO,
        };
        for chunk in chunks {
            let [minimum, maximum] = chunk.bounds().map(Vec3::from_array);
            // Match TerrainChunkDrawPlan::is_visible's operation order.
            let center = (minimum + maximum) * 0.5;
            let half = (maximum - minimum) * 0.5;
            if !center.is_finite() || !half.is_finite() {
                return None;
            }
            result.minimum_center = result.minimum_center.min(center);
            result.maximum_center = result.maximum_center.max(center);
            result.maximum_half = result.maximum_half.max(half.abs());
        }
        (!chunks.is_empty()).then_some(result)
    }

    pub(super) fn may_have_visible_chunks(self, frustum: WorldFrustum) -> bool {
        !frustum.rejects_box_group(self.minimum_center, self.maximum_center, self.maximum_half)
    }
}

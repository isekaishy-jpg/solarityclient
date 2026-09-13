//! ADT surface submission follows the primary exterior camera traversal.

use solarity_rendering::{ScenePointLights, TerrainPreparedDraw, WorldCameraFrame, WorldFrustum};

use super::{RuntimeTerrainFrameError, TerrainGpuTile};

/// 799D40 admits terrain through the exterior clip installed by 790AF0.
/// Clearing the retained output also removes surfaces when the bank closes.
pub(super) fn prepare_terrain_draws(
    tiles: &[TerrainGpuTile],
    draws: &mut Vec<TerrainPreparedDraw>,
    frustum: Option<WorldFrustum>,
    camera: WorldCameraFrame,
    points: &ScenePointLights,
) -> Result<(), RuntimeTerrainFrameError> {
    draws.clear();
    let Some(frustum) = frustum else {
        return Ok(());
    };
    for tile in tiles {
        if !tile.plan.may_have_visible_chunks(frustum) {
            continue;
        }
        for (chunk, draw) in tile.plan.chunks().iter().zip(&tile.draws) {
            if !chunk.is_visible(frustum)? {
                continue;
            }
            // Animated sources publish before terrain queries their current
            // lights. Copy resident packets so retired lights cannot persist.
            let draw = if points.points().is_empty() {
                *draw
            } else {
                let (center, radius) = chunk.point_light_bounds();
                draw.with_point_lights(points.terrain_lighting(
                    center,
                    radius,
                    camera.camera().position(),
                )?)
            };
            draws.push(draw);
        }
    }
    Ok(())
}

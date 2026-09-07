//! MH2O layer rectangles, masks, and native local-grid faces (`7CE960/7CE5D0`).

use glam::{Mat4, Vec3};
use solarity_asset::{DecodedTerrainTile, TerrainChunkIndex};

use super::super::{
    MovementCollectionError, MovementCollisionBounds, terrain_square_bounds, world_model_triangle,
};
use super::admits_height;
use crate::collision::MovementCollisionTriangle;

/// Appends a resident chunk's liquid faces in authored layer and square order.
/// The caller visits chunks in `MovementCollisionBounds::terrain_chunks` order
/// and inverts the completed liquid bank's planes for swimming surface response.
///
/// # Errors
/// Rejects invalid query coordinates or generated triangles.
pub fn append_terrain_liquid_movement(
    tile: &DecodedTerrainTile,
    chunk: TerrainChunkIndex,
    bounds: MovementCollisionBounds,
    output: &mut Vec<MovementCollisionTriangle>,
) -> Result<(), MovementCollectionError> {
    let Some(liquids) = tile.liquids() else {
        return Ok(());
    };
    let index = usize::from(chunk.y()) * 16 + usize::from(chunk.x());
    let origin = [
        i32::from(tile.index().y()) * 128 + i32::from(chunk.y()) * 8,
        i32::from(tile.index().x()) * 128 + i32::from(chunk.x()) * 8,
    ];
    let [minimum, maximum] = terrain_square_bounds(bounds)?;
    let minimum = [minimum[0] - origin[0], minimum[1] - origin[1]];
    let maximum = [maximum[0] - origin[0], maximum[1] - origin[1]];
    let base = Vec3::from_array(tile.chunks()[index].position());
    let local = MovementCollisionBounds::new(bounds.minimum() - base, bounds.maximum() - base)?;
    let transform = Mat4::from_translation(base);
    for layer in liquids.chunks()[index].layers() {
        let first_row = i32::from(layer.y_offset());
        let first_column = i32::from(layer.x_offset());
        let rows = first_row + i32::from(layer.height());
        let columns = first_column + i32::from(layer.width());
        let stride = usize::from(layer.width()) + 1;
        for row in minimum[0].max(first_row)..=maximum[0].min(rows - 1) {
            for column in minimum[1].max(first_column)..=maximum[1].min(columns - 1) {
                let cell = (row - first_row) as usize * usize::from(layer.width())
                    + (column - first_column) as usize;
                if layer.exists()[cell] == 0 {
                    continue;
                }
                let first = (row - first_row) as usize * stride + (column - first_column) as usize;
                // 7CDF80 stores the local height subtraction and negative grid
                // products before 782740 applies the chunk translation.
                let vertices = [
                    (row, column, first),
                    (row + 1, column, first + stride),
                    (row + 1, column + 1, first + stride + 1),
                    (row, column + 1, first + 1),
                ]
                .map(|(row, column, index)| {
                    Vec3::new(
                        row as f32 * -4.166_666_5,
                        column as f32 * -4.166_666_5,
                        layer.heights()[index] - base.z,
                    )
                });
                for indices in [[0, 1, 2], [0, 2, 3]] {
                    let triangle = indices.map(|index| vertices[index]);
                    if admits_height(local, triangle) {
                        output.push(world_model_triangle(triangle, transform)?);
                    }
                }
            }
        }
    }
    Ok(())
}

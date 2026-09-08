//! WMO liquid-grid collection over retained local mesh positions (`7C94B0`).

use glam::Vec3;

use super::super::{
    MovementCollectionError, MovementCollisionBounds, transformed_query, world_model_triangle,
};
use super::admits_height;
use crate::collision::{MovementCollisionTriangle, PlacedWorldModelCollision};

impl PlacedWorldModelCollision {
    /// Collects 7CAB70 liquid cells from the camera's oriented local volume.
    /// Camera admission excludes MOGP 0x80; movement's 0x400000 exclusion
    /// does not apply to 7AF0F0.
    ///
    /// # Errors
    /// Rejects invalid transformed geometry.
    pub fn append_liquid_camera_volume(
        &self,
        volume: &crate::PlayerCameraVolume,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        let corners = volume
            .corners()
            .map(|point| super::super::transform_point(self.inverse_transform, point));
        let minimum = corners
            .iter()
            .copied()
            .fold(Vec3::splat(f32::INFINITY), Vec3::min);
        let maximum = corners
            .iter()
            .copied()
            .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
        self.append_liquid_selected(
            MovementCollisionBounds::new(minimum, maximum)?,
            0x80,
            output,
        )
    }

    /// Appends water-only faces in root-group and row-major cell order. Native
    /// collision uses regular MLIQ cells, including render-portal-clipped tiles.
    /// The caller inverts the completed bank's planes before swimming sweeps.
    ///
    /// # Errors
    /// Rejects invalid transformed bounds or generated triangles.
    pub fn append_liquid_movement(
        &self,
        bounds: MovementCollisionBounds,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        if !self.movement_intersects(bounds) {
            return Ok(());
        }
        let local = transformed_query(bounds, self.inverse_transform)?;
        let root = self.model.bounds();
        if (0..3).any(|axis| {
            local.minimum()[axis] > root[1][axis] || local.maximum()[axis] < root[0][axis]
        }) {
            return Ok(());
        }
        self.append_liquid_selected(local, 0x40_0080, output)
    }

    fn append_liquid_selected(
        &self,
        local: MovementCollisionBounds,
        excluded_groups: u32,
        output: &mut Vec<MovementCollisionTriangle>,
    ) -> Result<(), MovementCollectionError> {
        for (index, group) in self.model.groups().iter().enumerate() {
            if group.flags() & excluded_groups != 0
                || group.resolve_liquid_type(self.model.flags()) == 0
            {
                continue;
            }
            let selection = self.model.group_info()[index].bounds();
            if (0..3).any(|axis| {
                local.minimum()[axis] > selection[1][axis]
                    || local.maximum()[axis] < selection[0][axis]
            }) {
                continue;
            }
            let Some(grid) = group
                .liquid()
                .filter(|grid| grid.tile_width() != 0 && grid.tile_height() != 0)
            else {
                continue;
            };
            let minimum = [local.minimum().x, local.minimum().y];
            let maximum = [local.maximum().x, local.maximum().y];
            // 7C94B0 stores subtraction and multiplication before floor.
            let first: [i32; 2] = std::array::from_fn(|axis| {
                ((minimum[axis] - grid.corner()[axis]) * f32::from_bits(0x3e75_c290)).floor() as i32
            });
            let last: [i32; 2] = std::array::from_fn(|axis| {
                ((maximum[axis] - grid.corner()[axis]) * f32::from_bits(0x3e75_c290)).floor() as i32
            });
            let first = [first[0].max(0), first[1].max(0)];
            let last = [
                last[0].min(grid.tile_width() as i32 - 1),
                last[1].min(grid.tile_height() as i32 - 1),
            ];
            if first[0] > last[0] || first[1] > last[1] {
                continue;
            }
            // 7C9651 rejects queries requiring more than 8192 vertex outcodes.
            if i64::from(last[0] - first[0] + 2) * i64::from(last[1] - first[1] + 2) > 8192 {
                continue;
            }
            let stride = grid.vertex_width() as usize;
            let mut y = grid.corner()[1];
            for _ in 0..first[1] {
                y += 4.166_666_5;
            }
            let mut start_x = grid.corner()[0];
            for _ in 0..first[0] {
                start_x += 4.166_666_5;
            }
            for row in first[1]..=last[1] {
                let next_y = y + 4.166_666_5;
                let mut x = start_x;
                for column in first[0]..=last[0] {
                    let next_x = x + 4.166_666_5;
                    if grid.tiles()[row as usize * grid.tile_width() as usize + column as usize]
                        & 0xf
                        != 0xf
                    {
                        let first = row as usize * stride + column as usize;
                        let vertices = [
                            (x, y, first),
                            (next_x, y, first + 1),
                            (next_x, next_y, first + stride + 1),
                            (x, next_y, first + stride),
                        ]
                        .map(|(x, y, index)| Vec3::new(x, y, grid.vertices()[index].height()));
                        for indices in [[0, 2, 3], [0, 1, 2]] {
                            let triangle = indices.map(|index| vertices[index]);
                            if admits_height(local, triangle) {
                                output.push(world_model_triangle(triangle, self.transform)?);
                            }
                        }
                    }
                    x = next_x;
                }
                y = next_y;
            }
        }
        Ok(())
    }
}

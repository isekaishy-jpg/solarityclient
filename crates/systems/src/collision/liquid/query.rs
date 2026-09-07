//! Native submerged point admission, separate from rendered liquid triangles.

use solarity_asset::DecodedTerrainTile;

use crate::collision::{TerrainCollisionError, TerrainRegistrationPoint};

/// Authored liquid covering a point admitted by the native scene query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubmergedLiquid {
    /// Raw LiquidType identifier before area-specific behavior substitution.
    pub liquid_type: u32,
    /// Native surface height: world Z except for a registered WMO-group query,
    /// which returns local Z as 790920 expects.
    pub surface_height: f32,
    /// Surface minus query-point Z in the same coordinate basis.
    pub depth: f32,
}

impl TerrainRegistrationPoint {
    /// Runs 7A0820's liquid-cell and terrain-floor admission on its owning ADT.
    /// `terrain_height` comes from the same point's `registration_height_at`;
    /// native authored holes leave that output at -10000.
    ///
    /// # Errors
    /// Rejects an incorrect resident tile or non-finite height input.
    pub fn submerged_liquid(
        self,
        tile: &DecodedTerrainTile,
        world_height: f32,
        terrain_height: Option<f32>,
    ) -> Result<Option<SubmergedLiquid>, TerrainCollisionError> {
        if tile.index() != self.tile() {
            return Err(TerrainCollisionError::WrongRegistrationTile);
        }
        if !world_height.is_finite() || terrain_height.is_some_and(|height| !height.is_finite()) {
            return Err(TerrainCollisionError::NonFinitePoint);
        }
        let Some(liquids) = tile.liquids() else {
            return Ok(None);
        };
        // 7A0967 keeps the initialized -10000 when 7AD3B0 finds a terrain hole.
        if f64::from(terrain_height.unwrap_or(-10_000.0))
            >= f64::from(world_height) + f64::from(0.01_f32)
        {
            return Ok(None);
        }
        let chunk = usize::from(self.chunk().y()) * 16 + usize::from(self.chunk().x());
        let ([row, column], [across_rows, across_columns]) = self.liquid_square();
        for layer in liquids.chunks()[chunk].layers() {
            let Some(row) = row.checked_sub(usize::from(layer.y_offset())) else {
                continue;
            };
            let Some(column) = column.checked_sub(usize::from(layer.x_offset())) else {
                continue;
            };
            let width = usize::from(layer.width());
            if row >= usize::from(layer.height())
                || column >= width
                || layer.exists()[row * width + column] == 0
            {
                continue;
            }
            let stride = width + 1;
            let first = row * stride + column;
            let heights = layer.heights();
            let surface_height = bilinear_height(
                [
                    heights[first],
                    heights[first + 1],
                    heights[first + stride],
                    heights[first + stride + 1],
                ],
                [across_columns, across_rows],
            ) as f32;
            if f64::from(world_height) < f64::from(surface_height) + f64::from(0.01_f32) {
                return Ok(Some(SubmergedLiquid {
                    liquid_type: u32::from(layer.liquid_type()),
                    surface_height,
                    depth: surface_height - world_height,
                }));
            }
        }
        Ok(None)
    }
}

/// 7CE0B0 and 7C8360 interpolate rows in x87 before the final float store.
pub(in crate::collision) fn bilinear_height(heights: [f32; 4], fractions: [f32; 2]) -> f64 {
    let [a, b, c, d] = heights.map(f64::from);
    let [x, y] = fractions.map(f64::from);
    let first = (b - a) * x + a;
    let second = (d - c) * x + c;
    (second - first) * y + first
}

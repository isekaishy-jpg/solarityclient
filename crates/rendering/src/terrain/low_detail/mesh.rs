//! Original WDL mesh upload (`7D5150`) and face-bank indexing (`7D5240`).

use solarity_asset::TerrainLowDetailTile;

/// Immutable 545-vertex horizon tile and its two native face-culling banks.
#[derive(Debug, PartialEq)]
pub struct TerrainLowDetailMesh {
    positions: [[f32; 3]; 545],
    indices: Vec<u16>,
    unculled_index_count: u32,
    bounds: [[f32; 3]; 2],
}

impl TerrainLowDetailMesh {
    /// Builds the native fan topology without removing MAHO-marked cells.
    #[must_use]
    pub fn new(tile: &TerrainLowDetailTile) -> Self {
        let step = f64::from(33.333_332_f32);
        let origin = f64::from(17_066.666_f32);
        // 7CC310 stores the base corner as floats. 7D5150 subsequently keeps
        // each stepping accumulator in x87 through a complete row/column.
        let extended_x = origin - f64::from(tile.index().y()) * 16.0 * step;
        let extended_y = origin - f64::from(tile.index().x()) * 16.0 * step;
        let base_x = extended_x as f32;
        let base_y = extended_y as f32;
        let mut positions = [[0.0; 3]; 545];
        for (start, width, inset) in [(0, 17, 0.0), (289, 16, f64::from(16.666_666_f32))] {
            for row in 0..width {
                for column in 0..width {
                    let index = start + row * width + column;
                    positions[index] = [
                        (f64::from(base_x) - inset - row as f64 * step) as f32,
                        (f64::from(base_y) - inset - column as f64 * step) as f32,
                        f32::from(tile.heights()[index]),
                    ];
                }
            }
        }
        let mut indices = Vec::with_capacity(3072);
        append_indices(tile.face_masks(), false, &mut indices);
        let unculled_index_count = indices.len() as u32;
        append_indices(tile.face_masks(), true, &mut indices);
        let mut minimum_z = i16::MAX;
        let mut maximum_z = i16::MIN;
        for height in tile.heights() {
            minimum_z = minimum_z.min(*height);
            maximum_z = maximum_z.max(*height);
        }
        Self {
            positions,
            indices,
            unculled_index_count,
            bounds: [
                // The loader retains x87 base values for the lower bounds,
                // even after publishing rounded float corners for mesh upload.
                [
                    (extended_x - f64::from(533.333_3_f32)) as f32,
                    (extended_y - f64::from(533.333_3_f32)) as f32,
                    f32::from(minimum_z),
                ],
                [base_x, base_y, f32::from(maximum_z)],
            ],
        }
    }

    /// Returns the native corner-grid then cell-center positions.
    pub const fn positions(&self) -> &[[f32; 3]; 545] {
        &self.positions
    }

    /// Returns both contiguous face banks in native triangle order.
    pub fn indices(&self) -> &[u16] {
        &self.indices
    }

    /// Returns the prefix drawn with GX cull mode zero; the suffix uses mode one.
    pub const fn unculled_index_count(&self) -> u32 {
        self.unculled_index_count
    }

    /// Returns the native tile bounds used for horizon-frustum selection.
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }
}

/// Appends exactly the selected MAHO bank in native row-major fan order.
fn append_indices(masks: &[u16; 16], marked: bool, output: &mut Vec<u16>) {
    for (row, mask) in masks.iter().enumerate() {
        for column in 0..16 {
            if (mask & (1 << column) != 0) != marked {
                continue;
            }
            let corner = (row * 17 + column) as u16;
            let center = (289 + row * 16 + column) as u16;
            output.extend_from_slice(&[
                center,
                corner + 1,
                corner,
                center,
                corner + 18,
                corner + 1,
                center,
                corner + 17,
                corner + 18,
                center,
                corner,
                corner + 17,
            ]);
        }
    }
}

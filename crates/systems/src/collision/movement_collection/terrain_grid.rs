//! Native global square selection and X-major world chunk iteration.

use solarity_asset::{TerrainChunkIndex, TerrainTileIndex};

use super::{MovementCollectionError, MovementCollisionBounds};

const MAP_ORIGIN: f32 = 17_066.666;
const MAP_SIZE: f32 = 34_133.332;
const SQUARES_PER_UNIT: f32 = 0.24;

/// Allocation-free chunk addresses in `0x007A5F20` visitation order.
///
/// Terrain filenames transpose the two horizontal world axes. Items retain
/// that distinction as an ADT address paired with its local MCNK address.
pub struct MovementTerrainChunks {
    row: i32,
    column: i32,
    first_column: i32,
    last_row: i32,
    last_column: i32,
}

impl MovementCollisionBounds {
    /// Resolves the exact ordered resident chunks required by this terrain box.
    ///
    /// The world owner must check each declared tile's residency before using
    /// an empty triangle collection as an unobstructed movement interval.
    ///
    /// # Errors
    /// Returns [`MovementCollectionError::OutsideTerrainMap`] for a box outside
    /// stock's terrain coordinate domain.
    pub fn terrain_chunks(self) -> Result<MovementTerrainChunks, MovementCollectionError> {
        let [minimum, maximum] = terrain_square_bounds(self)?;
        Ok(MovementTerrainChunks {
            row: minimum[0] >> 3,
            column: minimum[1] >> 3,
            first_column: minimum[1] >> 3,
            last_row: maximum[0] >> 3,
            last_column: maximum[1] >> 3,
        })
    }
}

impl Iterator for MovementTerrainChunks {
    type Item = (TerrainTileIndex, TerrainChunkIndex);

    fn next(&mut self) -> Option<Self::Item> {
        if self.row > self.last_row || self.column > self.last_column {
            return None;
        }
        // These are native ADT/MCNK masks, including the rounded outer-map edge.
        let tile = TerrainTileIndex::new(
            ((self.column >> 4) & 63) as u8,
            ((self.row >> 4) & 63) as u8,
        )?;
        let chunk = TerrainChunkIndex::new((self.column & 15) as u8, (self.row & 15) as u8)?;
        self.column += 1;
        if self.column > self.last_column {
            self.column = self.first_column;
            self.row += 1;
        }
        Some((tile, chunk))
    }
}

/// Preserves the four x87 store boundaries used before FISTP grid conversion.
pub(in crate::collision) fn terrain_square_bounds(
    bounds: MovementCollisionBounds,
) -> Result<[[i32; 2]; 2], MovementCollectionError> {
    let origin = f64::from(MAP_ORIGIN);
    if (0..2).any(|axis| {
        origin - f64::from(bounds.maximum[axis]) < 0.0
            || origin - f64::from(bounds.minimum[axis]) >= f64::from(MAP_SIZE)
    }) {
        return Err(MovementCollectionError::OutsideTerrainMap);
    }
    let minimum = [bounds.maximum.x, bounds.maximum.y]
        .map(|v| square(f64::from((origin - f64::from(v)) as f32)));
    // Y-min is the first FISTP conversion: its subtraction is still on the
    // x87 stack. The other three reload previously stored float differences.
    let maximum = [
        square(f64::from((origin - f64::from(bounds.minimum.x)) as f32)),
        square(origin - f64::from(bounds.minimum.y)),
    ];
    Ok([minimum, maximum])
}

/// Stock stores the scaled coordinate as float before subtracting one half.
fn square(relative: f64) -> i32 {
    let scaled = (relative * f64::from(SQUARES_PER_UNIT)) as f32;
    (f64::from(scaled) - 0.5).round_ties_even() as i32
}

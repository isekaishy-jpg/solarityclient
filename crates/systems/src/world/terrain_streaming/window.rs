//! Chunk-aligned terrain demand from build 12340's `0x00780860`.

use glam::Vec3;
use solarity_asset::TerrainTileIndex;
use thiserror::Error;

use crate::world::WorldViewDistance;

/// Native inner loading range and outer retention range in global MCNK units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainStreamingWindow {
    required: [[i32; 2]; 2],
    retained: [[i32; 2]; 2],
}

/// An invalid camera input cannot select a terrain residency generation.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TerrainStreamingError {
    /// The world origin or resolved far-frustum corner is not finite.
    #[error("terrain streaming camera geometry is invalid")]
    InvalidCamera,
    /// The streaming origin is outside the tiled world's coordinate domain.
    #[error("terrain streaming origin is outside the terrain map")]
    OutsideTerrainMap,
}

impl TerrainStreamingWindow {
    /// Resolves stock demand from the world origin and eighth frustum corner.
    ///
    /// `far_corner` is the independently resolved corner returned by the camera
    /// owner, in the coordinate space of its inverse view matrix. Stock uses
    /// its distance from zero, not its distance from `origin`. The inner range
    /// is aligned to two MCNKs; the outer range adds stock's retention margin.
    ///
    /// # Errors
    /// Returns [`TerrainStreamingError`] for invalid camera or world geometry.
    pub fn new(
        origin: Vec3,
        view_distance: WorldViewDistance,
        far_corner: Vec3,
    ) -> Result<Self, TerrainStreamingError> {
        if !origin.is_finite() || !far_corner.is_finite() {
            return Err(TerrainStreamingError::InvalidCamera);
        }
        let map_origin = f64::from(17_066.666_f32);
        if [origin.x, origin.y].into_iter().any(|coordinate| {
            let relative = map_origin - f64::from(coordinate);
            !(0.0..f64::from(34_133.332_f32)).contains(&relative)
        }) {
            return Err(TerrainStreamingError::OutsideTerrainMap);
        }
        let far = f64::from(view_distance.value());
        let corner = far_corner.as_dvec3().length();
        let radius = if corner < far {
            far * 1.25
        } else {
            corner.min(far * 2.0)
        };
        // The native multiplier is negative and differs by one ULP from the
        // positive coordinate multiplier. Ftol truncates the wide product.
        let extent = 1 - (radius * f64::from(-0.030_000_001_f32)).trunc() as i32;
        let center = [
            chunk_coordinate(map_origin - f64::from(origin.x)),
            chunk_coordinate(f64::from((map_origin - f64::from(origin.y)) as f32)),
        ];
        let mut required = [
            center.map(|coordinate| (coordinate - extent) & !1),
            center.map(|coordinate| ((coordinate + extent) & !1) + 1),
        ];
        let mut retained = [required[0].map(|v| v - 2), required[1].map(|v| v + 2)];
        let extra = 8 - (retained[1][1] - center[1]);
        if extra > 0 {
            retained[0] = retained[0].map(|v| (v - extra) & !1);
            retained[1] = retained[1].map(|v| ((v + extra) & !1) + 1);
        }
        required[0] = required[0].map(|v| v.max(0));
        required[1] = required[1].map(|v| v.min(1023));
        retained[0] = retained[0].map(|v| v.max(0));
        retained[1] = retained[1].map(|v| v.min(1023));
        Ok(Self { required, retained })
    }

    /// Returns the inclusive inner global chunk bounds, in world X/Y order.
    #[must_use]
    pub const fn required_chunks(self) -> [[i32; 2]; 2] {
        self.required
    }

    /// Returns the inclusive outer global chunk bounds, in world X/Y order.
    #[must_use]
    pub const fn retained_chunks(self) -> [[i32; 2]; 2] {
        self.retained
    }

    /// Visits retained ADTs in native world-row then world-column order.
    pub fn tiles(self) -> impl Iterator<Item = TerrainTileIndex> {
        let minimum = self.retained[0].map(|v| v >> 4);
        let maximum = self.retained[1].map(|v| v >> 4);
        (minimum[0]..=maximum[0]).flat_map(move |row| {
            (minimum[1]..=maximum[1])
                .filter_map(move |column| TerrainTileIndex::new(column as u8, row as u8))
        })
    }

    /// Whether a resident ADT remains inside the native outer loading range.
    #[must_use]
    pub fn contains(self, tile: TerrainTileIndex) -> bool {
        let coordinates = [i32::from(tile.y()), i32::from(tile.x())];
        (0..2).all(|axis| {
            (self.retained[0][axis] >> 4) <= coordinates[axis]
                && coordinates[axis] <= (self.retained[1][axis] >> 4)
        })
    }
}

/// Preserves the native float store before its half-offset FISTP conversion.
fn chunk_coordinate(relative: f64) -> i32 {
    let scaled = (relative * f64::from(0.03_f32)) as f32;
    (f64::from(scaled) - 0.5).round_ties_even() as i32
}

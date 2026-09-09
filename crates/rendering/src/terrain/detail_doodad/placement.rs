//! Validated terrain-detail inputs and chunk-local placement records.

use solarity_asset::{DecodedTerrainTile, GroundEffectCatalog, TerrainChunkIndex};

use super::scatter::scatter;

/// Stock `groundEffectDensity` selects between sixteen and 256 cells per chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroundDetailDensity(u16);

impl GroundDetailDensity {
    /// Accepts the same integer range as native CVar callback 78DAB0.
    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        if value >= 16 && value <= 256 {
            Some(Self(value as u16))
        } else {
            None
        }
    }

    /// Returns the number of cell selections, including repeated selections.
    #[must_use]
    pub const fn cells(self) -> u16 {
        self.0
    }
}

impl Default for GroundDetailDensity {
    fn default() -> Self {
        Self(64)
    }
}

/// A complete stock placement before native texture batching and mesh expansion.
#[derive(Clone, Copy, Debug)]
pub struct GroundDetailPlacement {
    pub(super) model: u32,
    pub(super) position: [f32; 3],
    pub(super) angle: f32,
    pub(super) scale: f32,
    pub(super) normal: [f32; 3],
    pub(super) face: u16,
    pub(super) color: [u8; 4],
}

impl GroundDetailPlacement {
    /// Returns the `GroundEffectDoodad.dbc` model key.
    #[must_use]
    pub const fn model(self) -> u32 {
        self.model
    }

    /// Returns position relative to the chunk origin, including relative height.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the rotation about the selected surface axis, in radians.
    #[must_use]
    pub const fn angle(self) -> f32 {
        self.angle
    }

    /// Returns the uniform scale in stock's approximately 0.67 through 1.33 range.
    #[must_use]
    pub const fn scale(self) -> f32 {
        self.scale
    }

    /// Returns the geometric terrain-face normal used by the detail shader.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        self.normal
    }

    /// Returns the chunk-local terrain triangle, in native detail ordering.
    #[must_use]
    pub const fn face(self) -> u16 {
        self.face
    }

    /// Returns terrain tint in RGBA order; alpha encodes authored shadow visibility.
    #[must_use]
    pub const fn color(self) -> [u8; 4] {
        self.color
    }
}

/// Reproducible detail placements and their common world-space chunk origin.
pub struct TerrainDetailChunk {
    origin: [f32; 3],
    placements: Vec<GroundDetailPlacement>,
}

impl TerrainDetailChunk {
    /// Runs native 7D3390's cell selection, weighted scatter, and terrain sampling.
    ///
    /// # Errors
    /// Returns [`GroundDetailError`] when a selected model has no database row.
    pub fn prepare(
        tile: &DecodedTerrainTile,
        index: TerrainChunkIndex,
        catalog: &GroundEffectCatalog,
        density: GroundDetailDensity,
    ) -> Result<Self, GroundDetailError> {
        let chunk = &tile.chunks()[usize::from(index.y()) * 16 + usize::from(index.x())];
        let seed = ((u32::from(tile.index().y()) * 16 + u32::from(index.y())) << 16)
            | (u32::from(tile.index().x()) * 16 + u32::from(index.x()));
        Ok(Self {
            origin: chunk.position(),
            placements: scatter(chunk, catalog, seed, density)?,
        })
    }

    /// Returns the origin added to all local positions during mesh preparation.
    #[must_use]
    pub const fn origin(&self) -> [f32; 3] {
        self.origin
    }

    /// Returns placements in native generation order, before texture grouping.
    #[must_use]
    pub fn placements(&self) -> &[GroundDetailPlacement] {
        &self.placements
    }
}

/// A malformed ground-effect reference prevents deterministic mesh preparation.
#[derive(Debug, thiserror::Error)]
pub enum GroundDetailError {
    /// Native detail requires a first SKIN and first authored texture filename.
    #[error("detail model {0} lacks its first skin or texture filename")]
    ModelInput(solarity_asset::AssetPath),
    /// A malformed model exceeds the renderer's validated native index width.
    #[error("detail chunk exceeds its 16-bit index bank")]
    MeshCapacity,
    /// The selected texture definition references a missing model row.
    #[error("ground effect references missing doodad {0}")]
    MissingDoodad(u32),
}

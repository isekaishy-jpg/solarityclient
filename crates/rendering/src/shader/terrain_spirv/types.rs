//! Terrain shader permutation identity and owned module words.

use super::TerrainLayerCountError;

/// Stock's closed per-MCNK diffuse-layer domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum TerrainLayerCount {
    /// One base texture with no MCAL blend channel.
    One = 1,
    /// Base texture followed by one alpha-blended texture.
    Two = 2,
    /// Base texture followed by two alpha-blended textures.
    Three = 3,
    /// Base texture followed by all three alpha-blended textures.
    Four = 4,
}

impl TerrainLayerCount {
    /// Returns the exact number of statically consumed diffuse samplers.
    #[must_use]
    pub const fn get(self) -> u8 {
        self as u8
    }
}

impl TryFrom<usize> for TerrainLayerCount {
    type Error = TerrainLayerCountError;

    fn try_from(layer_count: usize) -> Result<Self, Self::Error> {
        match layer_count {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            3 => Ok(Self::Three),
            4 => Ok(Self::Four),
            _ => Err(TerrainLayerCountError { layer_count }),
        }
    }
}

/// Owned Vulkan shader bytecode for one terrain layer-count variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerrainSpirvProgram {
    layer_count: TerrainLayerCount,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
}

impl TerrainSpirvProgram {
    pub(super) fn new(
        layer_count: TerrainLayerCount,
        vertex_words: Vec<u32>,
        fragment_words: Vec<u32>,
    ) -> Self {
        Self {
            layer_count,
            vertex_words,
            fragment_words,
        }
    }

    /// Returns the exact number of diffuse bindings used by the fragment stage.
    #[must_use]
    pub const fn layer_count(&self) -> TerrainLayerCount {
        self.layer_count
    }

    /// Returns the vertex module as native SPIR-V words.
    #[must_use]
    pub fn vertex_words(&self) -> &[u32] {
        &self.vertex_words
    }

    /// Returns the fragment module as native SPIR-V words.
    #[must_use]
    pub fn fragment_words(&self) -> &[u32] {
        &self.fragment_words
    }
}

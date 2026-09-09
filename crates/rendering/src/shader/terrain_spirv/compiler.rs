//! Selection of build-generated terrain SPIR-V.

use super::{TerrainLayerCount, TerrainSpirvError, TerrainSpirvProgram};

const VERTICES: [&[u8]; 4] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-1.vert.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-2.vert.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-3.vert.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-4.vert.spv")),
];
const FRAGMENTS: [&[u8]; 4] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-1.frag.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-2.frag.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-3.frag.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-4.frag.spv")),
];
const SHADOW_VERTICES: [&[u8]; 4] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-1.vert.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-2.vert.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-3.vert.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-4.vert.spv")),
];
const SHADOW_FRAGMENTS: [&[u8]; 4] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-1.frag.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-2.frag.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-3.frag.spv")),
    include_bytes!(concat!(env!("OUT_DIR"), "/terrain-shadow-4.frag.spv")),
];

/// Selector for immutable terrain bytecode generated during the Cargo build.
pub struct TerrainSpirvCompiler;

impl TerrainSpirvCompiler {
    /// Selects Terrain2/Terrain3's primary dynamic-shadow receiving path.
    pub(crate) fn compile_primary_shadow(
        &self,
        layer_count: TerrainLayerCount,
    ) -> TerrainSpirvProgram {
        let index = usize::from(layer_count.get() - 1);
        TerrainSpirvProgram::new(
            layer_count,
            spirv_words(SHADOW_VERTICES[index]),
            spirv_words(SHADOW_FRAGMENTS[index]),
        )
    }
    /// Creates the build-generated shader selector.
    pub fn new() -> Result<Self, TerrainSpirvError> {
        Ok(Self)
    }

    /// Selects one exact authored layer-count pair.
    pub fn compile(
        &self,
        layer_count: TerrainLayerCount,
    ) -> Result<TerrainSpirvProgram, TerrainSpirvError> {
        let index = usize::from(layer_count.get() - 1);
        Ok(TerrainSpirvProgram::new(
            layer_count,
            spirv_words(VERTICES[index]),
            spirv_words(FRAGMENTS[index]),
        ))
    }
}

fn spirv_words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
        .collect()
}

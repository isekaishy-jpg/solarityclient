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

/// Selector for immutable terrain bytecode generated during the Cargo build.
pub struct TerrainSpirvCompiler;

impl TerrainSpirvCompiler {
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

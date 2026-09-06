//! Selection of build-generated simple-render SPIR-V.

use super::{UiShaderSource, UiSpirvError, UiSpirvProgram};

const TEXTURE_VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui-texture.vert.spv"));
const TEXTURE_FRAGMENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui-texture.frag.spv"));
const COLOR_VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui-color.vert.spv"));
const COLOR_FRAGMENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui-color.frag.spv"));
const MASKED_VERTEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui-masked.vert.spv"));
const MASKED_FRAGMENT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui-masked.frag.spv"));

/// Selector for immutable UI bytecode generated during the Cargo build.
pub struct UiSpirvCompiler;

impl UiSpirvCompiler {
    /// Creates the build-generated shader selector.
    pub fn new() -> Result<Self, UiSpirvError> {
        Ok(Self)
    }

    /// Selects the texture-backed or vertex-color pair.
    pub fn compile(&self, source: UiShaderSource) -> Result<UiSpirvProgram, UiSpirvError> {
        let (vertex, fragment) = match source {
            UiShaderSource::Texture => (TEXTURE_VERTEX, TEXTURE_FRAGMENT),
            UiShaderSource::MaskedTexture => (MASKED_VERTEX, MASKED_FRAGMENT),
            UiShaderSource::VertexColor => (COLOR_VERTEX, COLOR_FRAGMENT),
        };
        Ok(UiSpirvProgram::new(
            source,
            spirv_words(vertex),
            spirv_words(fragment),
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

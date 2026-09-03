//! Selection of build-generated ordinary and unified MapObj SPIR-V.

use solarity_asset::WorldModelShader;

use super::{WorldModelSpirvError, WorldModelSpirvKey, WorldModelSpirvProgram};

macro_rules! pair {
    ($unified:literal, $shader:literal) => {
        (
            include_bytes!(concat!(
                env!("OUT_DIR"),
                "/world-model-",
                stringify!($unified),
                "-",
                stringify!($shader),
                ".vert.spv"
            )),
            include_bytes!(concat!(
                env!("OUT_DIR"),
                "/world-model-",
                stringify!($unified),
                "-",
                stringify!($shader),
                ".frag.spv"
            )),
        )
    };
}

/// Selector for immutable MapObj bytecode generated during the Cargo build.
pub struct WorldModelSpirvCompiler;

impl WorldModelSpirvCompiler {
    /// Creates the build-generated shader selector.
    pub fn new() -> Result<Self, WorldModelSpirvError> {
        Ok(Self)
    }

    /// Selects one already-validated ordinary or unified effect pair.
    pub fn compile(
        &self,
        key: WorldModelSpirvKey,
    ) -> Result<WorldModelSpirvProgram, WorldModelSpirvError> {
        let (vertex, fragment) = embedded_pair(key);
        Ok(WorldModelSpirvProgram::new(
            key,
            spirv_words(vertex),
            spirv_words(fragment),
        ))
    }
}

fn embedded_pair(key: WorldModelSpirvKey) -> (&'static [u8], &'static [u8]) {
    match (key.is_unified(), key.shader()) {
        (false, WorldModelShader::Diffuse) => pair!(0, 0),
        (false, WorldModelShader::Specular) => pair!(0, 1),
        (false, WorldModelShader::Metal) => pair!(0, 2),
        (false, WorldModelShader::Environment) => pair!(0, 3),
        (false, WorldModelShader::Opaque) => pair!(0, 4),
        (false, WorldModelShader::EnvironmentMetal) => pair!(0, 5),
        (false, WorldModelShader::Composite) => pair!(0, 6),
        (true, WorldModelShader::Diffuse) => pair!(1, 0),
        (true, WorldModelShader::Specular) => pair!(1, 1),
        (true, WorldModelShader::Metal) => pair!(1, 2),
        (true, WorldModelShader::Environment) => pair!(1, 3),
        (true, WorldModelShader::Opaque) => pair!(1, 4),
        (true, WorldModelShader::EnvironmentMetal) => pair!(1, 5),
        (true, WorldModelShader::Composite) => pair!(1, 6),
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

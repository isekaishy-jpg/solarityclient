//! Pinned shaderc invocation for the stock terrain shader family.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

use super::source::{TERRAIN_FRAGMENT_SOURCE, TERRAIN_VERTEX_SOURCE};
use super::{TerrainLayerCount, TerrainSpirvError, TerrainSpirvProgram};

/// Reusable compiler for the four stock terrain layer-count variants.
pub struct TerrainSpirvCompiler {
    compiler: Compiler,
}

impl TerrainSpirvCompiler {
    /// Creates the compiler retained by the terrain pipeline registry.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainSpirvError::Initialization`] when shaderc cannot
    /// allocate its compiler object.
    pub fn new() -> Result<Self, TerrainSpirvError> {
        let compiler = Compiler::new().map_err(|error| TerrainSpirvError::Initialization {
            message: error.to_string(),
        })?;
        Ok(Self { compiler })
    }

    /// Compiles one exact authored layer count for the pinned backend target.
    ///
    /// # Errors
    ///
    /// Returns [`TerrainSpirvError::Compilation`] with shaderc's diagnostic
    /// when either translated stage is rejected.
    pub fn compile(
        &self,
        layer_count: TerrainLayerCount,
    ) -> Result<TerrainSpirvProgram, TerrainSpirvError> {
        let vertex_words = self.compile_stage(
            TERRAIN_VERTEX_SOURCE,
            ShaderKind::Vertex,
            "terrain.vert.glsl",
            layer_count,
        )?;
        let fragment_words = self.compile_stage(
            TERRAIN_FRAGMENT_SOURCE,
            ShaderKind::Fragment,
            "terrain.frag.glsl",
            layer_count,
        )?;
        Ok(TerrainSpirvProgram::new(
            layer_count,
            vertex_words,
            fragment_words,
        ))
    }

    fn compile_stage(
        &self,
        shader: &str,
        kind: ShaderKind,
        name: &'static str,
        layer_count: TerrainLayerCount,
    ) -> Result<Vec<u32>, TerrainSpirvError> {
        let mut options =
            CompileOptions::new().map_err(|error| TerrainSpirvError::Initialization {
                message: error.to_string(),
            })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();
        let layer_count = layer_count.get().to_string();
        options.add_macro_definition("TERRAIN_LAYER_COUNT", Some(&layer_count));
        self.compiler
            .compile_into_spirv(shader, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| TerrainSpirvError::Compilation {
                stage: match kind {
                    ShaderKind::Vertex => "vertex",
                    ShaderKind::Fragment => "fragment",
                    _ => "unknown",
                },
                message: error.to_string(),
            })
    }
}

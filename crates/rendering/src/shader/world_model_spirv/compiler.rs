//! Pinned shaderc invocation for ordinary and unified MapObj effects.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

use super::source::{WORLD_MODEL_FRAGMENT_SOURCE, WORLD_MODEL_VERTEX_SOURCE};
use super::{WorldModelSpirvError, WorldModelSpirvKey, WorldModelSpirvProgram};

/// Reusable compiler retained by the WMO pipeline registry.
pub struct WorldModelSpirvCompiler {
    compiler: Compiler,
}

impl WorldModelSpirvCompiler {
    /// Creates the compiler for the pinned Vulkan 1.3 backend.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelSpirvError::Initialization`] when shaderc cannot
    /// allocate its compiler object.
    pub fn new() -> Result<Self, WorldModelSpirvError> {
        let compiler = Compiler::new().map_err(|error| WorldModelSpirvError::Initialization {
            message: error.to_string(),
        })?;
        Ok(Self { compiler })
    }

    /// Compiles one already-validated MapObj effect for SPIR-V 1.6.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelSpirvError::Compilation`] with the rejected stage.
    pub fn compile(
        &self,
        key: WorldModelSpirvKey,
    ) -> Result<WorldModelSpirvProgram, WorldModelSpirvError> {
        let vertex_words = self.compile_stage(
            WORLD_MODEL_VERTEX_SOURCE,
            ShaderKind::Vertex,
            "world_model.vert.glsl",
            key,
        )?;
        let fragment_words = self.compile_stage(
            WORLD_MODEL_FRAGMENT_SOURCE,
            ShaderKind::Fragment,
            "world_model.frag.glsl",
            key,
        )?;
        Ok(WorldModelSpirvProgram::new(
            key,
            vertex_words,
            fragment_words,
        ))
    }

    fn compile_stage(
        &self,
        source: &str,
        kind: ShaderKind,
        name: &'static str,
        key: WorldModelSpirvKey,
    ) -> Result<Vec<u32>, WorldModelSpirvError> {
        let mut options =
            CompileOptions::new().map_err(|error| WorldModelSpirvError::Initialization {
                message: error.to_string(),
            })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();
        let shader = key.shader().index().to_string();
        let unified = u8::from(key.is_unified()).to_string();
        options.add_macro_definition("WORLD_MODEL_SHADER", Some(&shader));
        options.add_macro_definition("WORLD_MODEL_UNIFIED", Some(&unified));
        self.compiler
            .compile_into_spirv(source, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| WorldModelSpirvError::Compilation {
                stage: match kind {
                    ShaderKind::Vertex => "vertex",
                    ShaderKind::Fragment => "fragment",
                    _ => "unknown",
                },
                message: error.to_string(),
            })
    }
}

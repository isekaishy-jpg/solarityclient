//! Pinned shaderc invocation for the stock PCT0 ribbon path.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

use super::source::{RIBBON_FRAGMENT_SOURCE, RIBBON_VERTEX_SOURCE};
use super::{M2RibbonSpirvError, M2RibbonSpirvProgram};
use crate::M2MaterialState;

/// Reusable compiler for material-state-specialized ribbon shaders.
pub struct M2RibbonSpirvCompiler {
    compiler: Compiler,
}

impl M2RibbonSpirvCompiler {
    /// Creates the compiler used by the renderer's ribbon pipeline registry.
    ///
    /// # Errors
    ///
    /// Returns [`M2RibbonSpirvError::Initialization`] if shaderc cannot create
    /// its compiler object.
    pub fn new() -> Result<Self, M2RibbonSpirvError> {
        let compiler = Compiler::new().map_err(|error| M2RibbonSpirvError::Initialization {
            message: error.to_string(),
        })?;
        Ok(Self { compiler })
    }

    /// Compiles the PCT0 shader pair for one exact stock material state.
    ///
    /// # Errors
    ///
    /// Returns [`M2RibbonSpirvError::Compilation`] with shaderc diagnostics.
    pub fn compile(
        &self,
        material: M2MaterialState,
    ) -> Result<M2RibbonSpirvProgram, M2RibbonSpirvError> {
        let vertex_words = self.compile_stage(
            RIBBON_VERTEX_SOURCE,
            ShaderKind::Vertex,
            "m2_ribbon.vert.glsl",
            material,
        )?;
        let fragment_words = self.compile_stage(
            RIBBON_FRAGMENT_SOURCE,
            ShaderKind::Fragment,
            "m2_ribbon.frag.glsl",
            material,
        )?;
        Ok(M2RibbonSpirvProgram::new(
            material,
            vertex_words,
            fragment_words,
        ))
    }

    fn compile_stage(
        &self,
        shader: &str,
        kind: ShaderKind,
        name: &'static str,
        material: M2MaterialState,
    ) -> Result<Vec<u32>, M2RibbonSpirvError> {
        let mut options =
            CompileOptions::new().map_err(|error| M2RibbonSpirvError::Initialization {
                message: error.to_string(),
            })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();
        let alpha_reference = material.alpha_reference(1.0).to_string();
        options.add_macro_definition("RIBBON_ALPHA_REFERENCE", Some(&alpha_reference));
        self.compiler
            .compile_into_spirv(shader, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| M2RibbonSpirvError::Compilation {
                stage: match kind {
                    ShaderKind::Vertex => "vertex",
                    ShaderKind::Fragment => "fragment",
                    _ => "unknown",
                },
                message: error.to_string(),
            })
    }
}

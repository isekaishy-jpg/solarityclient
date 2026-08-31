//! Pinned shaderc invocation for the stock ordinary-particle PNC0T0 path.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

use super::source::{PARTICLE_FRAGMENT_SOURCE, PARTICLE_VERTEX_SOURCE};
use super::{M2ParticleSpirvError, M2ParticleSpirvProgram};
use crate::M2MaterialState;

/// Reusable compiler for material-state-specialized particle shaders.
pub struct M2ParticleSpirvCompiler {
    compiler: Compiler,
}

impl M2ParticleSpirvCompiler {
    /// Creates the compiler used by the renderer's particle pipeline registry.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleSpirvError::Initialization`] if shaderc cannot
    /// create its compiler object.
    pub fn new() -> Result<Self, M2ParticleSpirvError> {
        let compiler = Compiler::new().map_err(|error| M2ParticleSpirvError::Initialization {
            message: error.to_string(),
        })?;
        Ok(Self { compiler })
    }

    /// Compiles the PNC0T0 shader pair for one synthesized stock material.
    ///
    /// # Errors
    ///
    /// Returns [`M2ParticleSpirvError::Compilation`] with shaderc diagnostics.
    pub fn compile(
        &self,
        material: M2MaterialState,
    ) -> Result<M2ParticleSpirvProgram, M2ParticleSpirvError> {
        let vertex_words = self.compile_stage(
            PARTICLE_VERTEX_SOURCE,
            ShaderKind::Vertex,
            "m2_particle.vert.glsl",
            material,
        )?;
        let fragment_words = self.compile_stage(
            PARTICLE_FRAGMENT_SOURCE,
            ShaderKind::Fragment,
            "m2_particle.frag.glsl",
            material,
        )?;
        Ok(M2ParticleSpirvProgram::new(
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
    ) -> Result<Vec<u32>, M2ParticleSpirvError> {
        let mut options =
            CompileOptions::new().map_err(|error| M2ParticleSpirvError::Initialization {
                message: error.to_string(),
            })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();
        let alpha_reference = material.alpha_reference(1.0).to_string();
        options.add_macro_definition("PARTICLE_ALPHA_REFERENCE", Some(&alpha_reference));
        self.compiler
            .compile_into_spirv(shader, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| M2ParticleSpirvError::Compilation {
                stage: match kind {
                    ShaderKind::Vertex => "vertex",
                    ShaderKind::Fragment => "fragment",
                    _ => "unknown",
                },
                message: error.to_string(),
            })
    }
}

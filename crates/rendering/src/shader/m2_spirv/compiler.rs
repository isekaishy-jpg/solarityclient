//! Pinned shaderc invocation and stock permutation specialization.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

use super::super::{M2PixelShader, M2ShaderPermutation, M2ShaderPlan, M2VertexShader};
use super::source::{M2_FRAGMENT_SOURCE, M2_VERTEX_SOURCE};
use super::{M2SpirvError, M2SpirvKey, M2SpirvProgram};

const SHADOW_VERTEX_STRIDE: usize = 30;
const SHADOW_PIXEL_MODULUS: usize = 4;

/// Reusable shaderc frontend pinned to the renderer's Vulkan/SPIR-V contract.
pub struct M2SpirvCompiler {
    compiler: Compiler,
}

impl M2SpirvCompiler {
    /// Creates one compiler for the renderer-owned M2 pipeline cache.
    ///
    /// # Errors
    ///
    /// Returns [`M2SpirvError::Initialization`] when shaderc cannot allocate
    /// its compiler object.
    pub fn new() -> Result<Self, M2SpirvError> {
        let compiler = Compiler::new().map_err(|error| M2SpirvError::Initialization {
            message: error.to_string(),
        })?;
        Ok(Self { compiler })
    }

    /// Compiles one exact effect/permutation pair for Vulkan 1.3.
    ///
    /// Direct and PCF selectors are equivalent while shadows are disabled.
    /// Other shadow paths are rejected until their authored BLS sampling math
    /// is translated; silently compiling an approximation would be a fallback
    /// stock does not contain.
    ///
    /// # Errors
    ///
    /// Returns [`M2SpirvError::UnsupportedShadow`] for a shadowed permutation,
    /// or [`M2SpirvError::Compilation`] with shaderc's stage diagnostic.
    pub fn compile(
        &self,
        plan: M2ShaderPlan,
        permutation: M2ShaderPermutation,
    ) -> Result<M2SpirvProgram, M2SpirvError> {
        let vertex_index = permutation.vertex_index();
        let pixel_index = permutation.pixel_index();
        if vertex_index / SHADOW_VERTEX_STRIDE != 0
            || !pixel_index.is_multiple_of(SHADOW_PIXEL_MODULUS)
        {
            return Err(M2SpirvError::UnsupportedShadow {
                vertex_index,
                pixel_index,
            });
        }

        let key = M2SpirvKey::new(plan, permutation);
        let vertex_words = self.compile_stage(
            M2_VERTEX_SOURCE,
            ShaderKind::Vertex,
            "m2.vert.glsl",
            plan,
            permutation,
        )?;
        let fragment_words = self.compile_stage(
            M2_FRAGMENT_SOURCE,
            ShaderKind::Fragment,
            "m2.frag.glsl",
            plan,
            permutation,
        )?;
        Ok(M2SpirvProgram::new(key, vertex_words, fragment_words))
    }

    /// Applies identical target and specialization options to one shader stage.
    fn compile_stage(
        &self,
        source: &str,
        kind: ShaderKind,
        name: &'static str,
        plan: M2ShaderPlan,
        permutation: M2ShaderPermutation,
    ) -> Result<Vec<u32>, M2SpirvError> {
        let mut options = CompileOptions::new().map_err(|error| M2SpirvError::Initialization {
            message: error.to_string(),
        })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();

        let vertex_effect = vertex_effect_index(plan.vertex_shader()).to_string();
        let pixel_effect = pixel_effect_index(plan.pixel_shader()).to_string();
        let vertex_permutation = permutation.vertex_index().to_string();
        let pixel_permutation = permutation.pixel_index().to_string();
        let texture_count = plan.texture_count().to_string();
        options.add_macro_definition("M2_VERTEX_EFFECT", Some(&vertex_effect));
        options.add_macro_definition("M2_PIXEL_EFFECT", Some(&pixel_effect));
        options.add_macro_definition("M2_VERTEX_PERMUTATION", Some(&vertex_permutation));
        options.add_macro_definition("M2_PIXEL_PERMUTATION", Some(&pixel_permutation));
        options.add_macro_definition("M2_TEXTURE_COUNT", Some(&texture_count));

        self.compiler
            .compile_into_spirv(source, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| M2SpirvError::Compilation {
                stage: match kind {
                    ShaderKind::Vertex => "vertex",
                    ShaderKind::Fragment => "fragment",
                    _ => "unknown",
                },
                message: error.to_string(),
            })
    }
}

/// Produces the GLSL compile-time selector for the seven stock vertex effects.
const fn vertex_effect_index(effect: M2VertexShader) -> u32 {
    match effect {
        M2VertexShader::DiffuseT1 => 0,
        M2VertexShader::DiffuseT2 => 1,
        M2VertexShader::DiffuseEnv => 2,
        M2VertexShader::DiffuseT1T2 => 3,
        M2VertexShader::DiffuseEnvT2 => 4,
        M2VertexShader::DiffuseT1Env => 5,
        M2VertexShader::DiffuseEnvEnv => 6,
    }
}

/// Produces the GLSL compile-time selector for every stock pixel combiner.
const fn pixel_effect_index(effect: M2PixelShader) -> u32 {
    match effect {
        M2PixelShader::Opaque => 0,
        M2PixelShader::Mod => 5,
        M2PixelShader::Decal => 1,
        M2PixelShader::Add => 2,
        M2PixelShader::Mod2x => 3,
        M2PixelShader::Fade => 4,
        M2PixelShader::OpaqueOpaque => 6,
        M2PixelShader::OpaqueMod => 11,
        M2PixelShader::OpaqueAdd => 7,
        M2PixelShader::OpaqueMod2x => 8,
        M2PixelShader::OpaqueMod2xNoAlpha => 9,
        M2PixelShader::OpaqueAddNoAlpha => 10,
        M2PixelShader::ModOpaque => 12,
        M2PixelShader::ModMod => 17,
        M2PixelShader::ModAdd => 13,
        M2PixelShader::ModMod2x => 14,
        M2PixelShader::ModMod2xNoAlpha => 15,
        M2PixelShader::ModAddNoAlpha => 16,
        M2PixelShader::AddMod => 18,
        M2PixelShader::Mod2xMod2x => 19,
        M2PixelShader::OpaqueMod2xNoAlphaAlpha => 20,
        M2PixelShader::OpaqueAddAlpha => 21,
        M2PixelShader::OpaqueAddAlphaAlpha => 22,
    }
}

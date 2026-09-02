//! Pinned Vulkan shader compilation for the stock FFXGlow pass chain.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};
use thiserror::Error;

const VERTEX_SOURCE: &str = include_str!("source/glow.vert.glsl");
const COMPOSITE_SOURCE: &str = include_str!("source/glow.frag.glsl");
const BLUR_SOURCE: &str = include_str!("source/glow_blur.frag.glsl");
const BOX_SOURCE: &str = include_str!("source/glow_box.frag.glsl");

/// Stable identity of one fragment stage in the stock glow chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlowShaderPass {
    /// Full-resolution scene plus squared blurred light contribution and gamma.
    Composite,
    /// One horizontal or vertical four-tap gaussian pass.
    Blur,
    /// Four-tap full-to-quarter-resolution downsample.
    Box,
}

/// Owned SPIR-V modules for one glow pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlowSpirvProgram {
    pass: GlowShaderPass,
    vertex_words: Vec<u32>,
    fragment_words: Vec<u32>,
}

impl GlowSpirvProgram {
    /// Returns the fragment operation represented by this pair.
    #[must_use]
    pub const fn pass(&self) -> GlowShaderPass {
        self.pass
    }

    /// Returns the fullscreen-triangle vertex module.
    #[must_use]
    pub fn vertex_words(&self) -> &[u32] {
        &self.vertex_words
    }

    /// Returns the selected glow fragment module.
    #[must_use]
    pub fn fragment_words(&self) -> &[u32] {
        &self.fragment_words
    }
}

/// Failure to initialize or compile an exact stock glow shader.
#[derive(Debug, Error)]
pub enum GlowSpirvError {
    /// shaderc could not allocate its compiler object.
    #[error("glow SPIR-V compiler initialization failed: {message}")]
    Initialization {
        /// Compiler initialization diagnostic.
        message: String,
    },
    /// GLSL compilation rejected one stage.
    #[error("glow {stage} shader compilation failed: {message}")]
    Compilation {
        /// Rejected stage name.
        stage: &'static str,
        /// Compiler diagnostic.
        message: String,
    },
}

/// Reusable compiler for the three fixed glow pipeline pairs.
pub struct GlowSpirvCompiler {
    compiler: Compiler,
}

impl GlowSpirvCompiler {
    /// Creates the pinned compiler.
    pub fn new() -> Result<Self, GlowSpirvError> {
        Compiler::new()
            .map(|compiler| Self { compiler })
            .map_err(|error| GlowSpirvError::Initialization {
                message: error.to_string(),
            })
    }

    /// Compiles one fixed pass for Vulkan 1.3 and SPIR-V 1.6.
    pub fn compile(&self, pass: GlowShaderPass) -> Result<GlowSpirvProgram, GlowSpirvError> {
        let vertex_words = self.compile_stage(
            VERTEX_SOURCE,
            ShaderKind::Vertex,
            "glow.vert.glsl",
            "vertex",
        )?;
        let (source, name) = match pass {
            GlowShaderPass::Composite => (COMPOSITE_SOURCE, "glow.frag.glsl"),
            GlowShaderPass::Blur => (BLUR_SOURCE, "glow_blur.frag.glsl"),
            GlowShaderPass::Box => (BOX_SOURCE, "glow_box.frag.glsl"),
        };
        let fragment_words = self.compile_stage(source, ShaderKind::Fragment, name, "fragment")?;
        Ok(GlowSpirvProgram {
            pass,
            vertex_words,
            fragment_words,
        })
    }

    fn compile_stage(
        &self,
        source: &str,
        kind: ShaderKind,
        name: &'static str,
        stage: &'static str,
    ) -> Result<Vec<u32>, GlowSpirvError> {
        let mut options =
            CompileOptions::new().map_err(|error| GlowSpirvError::Initialization {
                message: error.to_string(),
            })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();
        self.compiler
            .compile_into_spirv(source, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| GlowSpirvError::Compilation {
                stage,
                message: error.to_string(),
            })
    }
}

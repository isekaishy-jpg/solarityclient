//! Pinned shaderc invocation for the stock simple-render shader pair.

use shaderc::{
    CompileOptions, Compiler, EnvVersion, OptimizationLevel, ShaderKind, SpirvVersion, TargetEnv,
};

use super::source::{UI_FRAGMENT_SOURCE, UI_VERTEX_SOURCE};
use super::{UiShaderSource, UiSpirvError, UiSpirvProgram};

/// Reusable compiler owned by the renderer's UI pipeline registry.
pub struct UiSpirvCompiler {
    compiler: Compiler,
}

impl UiSpirvCompiler {
    /// Creates one compiler for all simple-render pipeline variants.
    ///
    /// # Errors
    ///
    /// Returns [`UiSpirvError::Initialization`] when shaderc cannot allocate
    /// its compiler object.
    pub fn new() -> Result<Self, UiSpirvError> {
        let compiler = Compiler::new().map_err(|error| UiSpirvError::Initialization {
            message: error.to_string(),
        })?;
        Ok(Self { compiler })
    }

    /// Compiles the texture-backed or vertex-color pair for the pinned target.
    ///
    /// # Errors
    ///
    /// Returns [`UiSpirvError::Compilation`] with shaderc's stage diagnostic.
    pub fn compile(&self, source: UiShaderSource) -> Result<UiSpirvProgram, UiSpirvError> {
        let vertex_words =
            self.compile_stage(UI_VERTEX_SOURCE, ShaderKind::Vertex, "ui.vert.glsl", source)?;
        let fragment_words = self.compile_stage(
            UI_FRAGMENT_SOURCE,
            ShaderKind::Fragment,
            "ui.frag.glsl",
            source,
        )?;
        Ok(UiSpirvProgram::new(source, vertex_words, fragment_words))
    }

    /// Applies the same Vulkan/SPIR-V/optimization contract to both stages.
    fn compile_stage(
        &self,
        shader: &str,
        kind: ShaderKind,
        name: &'static str,
        source: UiShaderSource,
    ) -> Result<Vec<u32>, UiSpirvError> {
        let mut options = CompileOptions::new().map_err(|error| UiSpirvError::Initialization {
            message: error.to_string(),
        })?;
        options.set_target_env(TargetEnv::Vulkan, EnvVersion::Vulkan1_3 as u32);
        options.set_target_spirv(SpirvVersion::V1_6);
        options.set_optimization_level(OptimizationLevel::Performance);
        options.set_warnings_as_errors();
        options.add_macro_definition(
            "UI_TEXTURED",
            Some(match source {
                UiShaderSource::Texture => "1",
                UiShaderSource::VertexColor => "0",
            }),
        );
        self.compiler
            .compile_into_spirv(shader, kind, name, "main", Some(&options))
            .map(|artifact| artifact.as_binary().to_vec())
            .map_err(|error| UiSpirvError::Compilation {
                stage: match kind {
                    ShaderKind::Vertex => "vertex",
                    ShaderKind::Fragment => "fragment",
                    _ => "unknown",
                },
                message: error.to_string(),
            })
    }
}

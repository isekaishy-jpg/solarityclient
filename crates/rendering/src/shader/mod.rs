//! Stock shader translation, SPIR-V 1.6 compilation, reflection, and pipelines.
//!
//! `ShaderEffectManager.cpp` and the GX device family identify the source
//! responsibility. `shaderc` produces the pinned SPIR-V target consumed by the
//! Vulkan 1.3 backend; unsupported stock shader forms fail explicitly.

mod m2_effect;
mod m2_spirv;
mod ribbon_spirv;
mod shader_effect_manager;
mod terrain_spirv;
mod ui_spirv;
mod world_model_effect;
mod world_model_spirv;

pub use m2_effect::{
    M2BlendFactor, M2LocalLightCount, M2MaterialState, M2PixelShader, M2ShaderPermutation,
    M2ShaderPlan, M2ShaderPlanError, M2ShadowFiltering, M2ShadowPermutation, M2VertexShader,
};
pub use m2_spirv::{M2SpirvCompiler, M2SpirvError, M2SpirvKey, M2SpirvProgram};
pub use ribbon_spirv::{M2RibbonSpirvCompiler, M2RibbonSpirvError, M2RibbonSpirvProgram};
pub use terrain_spirv::{
    TerrainLayerCount, TerrainLayerCountError, TerrainSpirvCompiler, TerrainSpirvError,
    TerrainSpirvProgram,
};
pub use ui_spirv::{UiShaderSource, UiSpirvCompiler, UiSpirvError, UiSpirvProgram};
pub use world_model_effect::{
    WorldModelBlendFactor, WorldModelBlendState, WorldModelFogMode, WorldModelLightingMode,
    WorldModelMaterialState, WorldModelSurfacePass, WorldModelSurfacePassPlan,
};
pub use world_model_spirv::{
    WorldModelSpirvCompiler, WorldModelSpirvError, WorldModelSpirvKey, WorldModelSpirvProgram,
};

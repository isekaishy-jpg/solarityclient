//! Stock shader translation, SPIR-V 1.6 compilation, reflection, and pipelines.
//!
//! `ShaderEffectManager.cpp` and the GX device family identify the source
//! responsibility. `shaderc` produces the pinned SPIR-V target consumed by the
//! Vulkan 1.3 backend; unsupported stock shader forms fail explicitly.

mod m2_effect;
mod shader_effect_manager;

pub use m2_effect::{
    M2BlendFactor, M2MaterialState, M2PixelShader, M2ShaderPlan, M2ShaderPlanError, M2VertexShader,
};

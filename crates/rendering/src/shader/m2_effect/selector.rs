//! Stock simple-combiner substitution and specialized shader lookup.

use solarity_asset::{DecodedM2Model, M2BlendMode};

use crate::model::M2DrawCall;

use super::{M2MaterialState, M2PixelShader, M2ShaderPlanError, M2VertexShader};

const SPECIALIZED_BIT: u16 = 0x8000;
const SECOND_UV_BIT: u16 = 0x4000;
const STAGE_SHIFT: u16 = 4;
const OPERATION_MASK: u16 = 0x7;
const ENVIRONMENT_BIT: u16 = 0x8;
const STOCK_SIMPLE_FALLBACK: u16 = 0x11;

/// Resolved shader names and material state for one exact M2 batch.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct M2ShaderPlan {
    requested_shader_id: u16,
    resolved_shader_id: u16,
    texture_count: u16,
    vertex_shader: M2VertexShader,
    pixel_shader: M2PixelShader,
    material: M2MaterialState,
    used_stock_fallback: bool,
}

impl M2ShaderPlan {
    /// Replays build-12340 substitution before selecting BLS effect basenames.
    ///
    /// # Errors
    ///
    /// Returns [`M2ShaderPlanError`] for a texture count outside stock's fixed
    /// two-stage path, an absent flag-`0x8` combiner entry, or an unsupported
    /// specialized selector.
    pub fn resolve(model: &DecodedM2Model, draw: &M2DrawCall) -> Result<Self, M2ShaderPlanError> {
        let texture_count = draw.batch().texture_count;
        if !(1..=2).contains(&texture_count) {
            return Err(M2ShaderPlanError::TextureCount {
                path: model.path().clone(),
                texture_count,
            });
        }
        let requested_shader_id = draw.batch().shader_id;
        let material = M2MaterialState::from_material(draw.material());
        if requested_shader_id & SPECIALIZED_BIT != 0 {
            let (vertex_shader, pixel_shader) = specialized_effect(requested_shader_id)
                .ok_or_else(|| M2ShaderPlanError::SpecializedShader {
                    path: model.path().clone(),
                    shader_id: requested_shader_id,
                })?;
            return Ok(Self {
                requested_shader_id,
                resolved_shader_id: requested_shader_id,
                texture_count,
                vertex_shader,
                pixel_shader,
                material,
                used_stock_fallback: false,
            });
        }

        let substituted = substitute_simple_shader(model, draw)?;
        let effect = simple_effect(draw, substituted);
        let (resolved_shader_id, vertex_shader, pixel_shader, used_stock_fallback) =
            if let Some((vertex_shader, pixel_shader)) = effect {
                (substituted, vertex_shader, pixel_shader, false)
            } else {
                // This 0x11 retry is present in build 12340's `GetEffect`; it is
                // the one compatibility fallback deliberately retained here.
                let (vertex_shader, pixel_shader) = simple_effect(draw, STOCK_SIMPLE_FALLBACK)
                    .ok_or_else(|| M2ShaderPlanError::StockFallback {
                        path: model.path().clone(),
                    })?;
                (STOCK_SIMPLE_FALLBACK, vertex_shader, pixel_shader, true)
            };
        Ok(Self {
            requested_shader_id,
            resolved_shader_id,
            texture_count,
            vertex_shader,
            pixel_shader,
            material,
            used_stock_fallback,
        })
    }

    /// Applies the scene queue's runtime-alpha pipeline override.
    ///
    /// Shader names and the authored alpha-test permutation remain unchanged;
    /// stock changes only framebuffer blending and depth writes for this copy.
    #[must_use]
    pub const fn with_runtime_alpha_fade(mut self) -> Self {
        self.material = self.material.with_runtime_alpha_fade();
        self
    }

    /// Returns the untouched on-disk batch shader word.
    #[must_use]
    pub const fn requested_shader_id(self) -> u16 {
        self.requested_shader_id
    }

    /// Returns the post-substitution selector used for the effect.
    #[must_use]
    pub const fn resolved_shader_id(self) -> u16 {
        self.resolved_shader_id
    }

    /// Returns the exact number of texture stages required by this effect.
    #[must_use]
    pub const fn texture_count(self) -> u16 {
        self.texture_count
    }

    /// Returns the selected stock vertex effect basename.
    #[must_use]
    pub const fn vertex_shader(self) -> M2VertexShader {
        self.vertex_shader
    }

    /// Returns the selected stock pixel effect basename.
    #[must_use]
    pub const fn pixel_shader(self) -> M2PixelShader {
        self.pixel_shader
    }

    /// Returns the exact pass-zero material state paired with the shaders.
    #[must_use]
    pub const fn material(self) -> M2MaterialState {
        self.material
    }

    /// Reports whether stock's observed `0x11` simple-effect retry was used.
    #[must_use]
    pub const fn used_stock_fallback(self) -> bool {
        self.used_stock_fallback
    }
}

/// Replaces raw combo-table indices with stock's packed two-stage selector.
fn substitute_simple_shader(
    model: &DecodedM2Model,
    draw: &M2DrawCall,
) -> Result<u16, M2ShaderPlanError> {
    let batch = draw.batch();
    if !model.uses_texture_combiners() {
        let mut stage = if draw.material().blend_mode() == M2BlendMode::Opaque {
            0
        } else {
            1
        };
        let coordinate = draw.texture_bindings()[0].texture_coordinate();
        if !(0..=2).contains(&coordinate) {
            stage |= ENVIRONMENT_BIT;
        }
        let mut shader = stage << STAGE_SHIFT;
        if coordinate == 1 {
            shader |= SECOND_UV_BIT;
        }
        return Ok(shader);
    }

    let mut stages = [0_u16; 2];
    let mut shader = 0_u16;
    for (stage_index, binding) in draw.texture_bindings().iter().enumerate() {
        let combo_index = usize::from(batch.shader_id) + stage_index;
        let mut combiner =
            if stage_index == 0 && draw.material().blend_mode() == M2BlendMode::Opaque {
                0
            } else {
                model
                    .texture_combiner_combos()
                    .get(combo_index)
                    .copied()
                    .ok_or_else(|| M2ShaderPlanError::MissingCombiner {
                        path: model.path().clone(),
                        stage: stage_index as u16,
                        combo_index,
                    })?
            };
        if !(0..=2).contains(&binding.texture_coordinate()) {
            combiner |= ENVIRONMENT_BIT;
        }
        stages[stage_index] = combiner;
        if stage_index + 1 == usize::from(batch.texture_count) && binding.texture_coordinate() == 1
        {
            shader |= SECOND_UV_BIT;
        }
    }
    Ok(shader | (stages[0] << STAGE_SHIFT) | stages[1])
}

/// Maps one post-substitution simple selector to stock effect names.
fn simple_effect(draw: &M2DrawCall, shader: u16) -> Option<(M2VertexShader, M2PixelShader)> {
    let first = (shader >> STAGE_SHIFT) & OPERATION_MASK;
    let second = shader & OPERATION_MASK;
    let first_environment = (shader >> STAGE_SHIFT) & ENVIRONMENT_BIT != 0;
    let second_environment = shader & ENVIRONMENT_BIT != 0;
    if draw.batch().texture_count == 1 {
        let coordinate = draw.texture_bindings()[0].texture_coordinate();
        let vertex = if first_environment {
            M2VertexShader::DiffuseEnv
        } else if coordinate == 0 {
            M2VertexShader::DiffuseT1
        } else {
            M2VertexShader::DiffuseT2
        };
        return Some((vertex, single_pixel(first)));
    }

    let vertex = match (first_environment, second_environment) {
        (true, true) => M2VertexShader::DiffuseEnvEnv,
        (true, false) => M2VertexShader::DiffuseEnvT2,
        (false, true) => M2VertexShader::DiffuseT1Env,
        (false, false) => M2VertexShader::DiffuseT1T2,
    };
    two_stage_pixel(first, second).map(|pixel| (vertex, pixel))
}

/// Reproduces stock's total single-stage combiner switch.
const fn single_pixel(combiner: u16) -> M2PixelShader {
    match combiner {
        0 => M2PixelShader::Opaque,
        1 => M2PixelShader::Mod,
        2 => M2PixelShader::Decal,
        3 => M2PixelShader::Add,
        4 => M2PixelShader::Mod2x,
        5 => M2PixelShader::Fade,
        6 | 7 | 8..=u16::MAX => M2PixelShader::Mod,
    }
}

/// Reproduces stock's partial two-stage table before its observed fallback.
const fn two_stage_pixel(first: u16, second: u16) -> Option<M2PixelShader> {
    match first {
        0 => Some(match second {
            0 => M2PixelShader::OpaqueOpaque,
            3 => M2PixelShader::OpaqueAdd,
            4 => M2PixelShader::OpaqueMod2x,
            6 => M2PixelShader::OpaqueMod2xNoAlpha,
            7 => M2PixelShader::OpaqueAddNoAlpha,
            1 | 2 | 5 | 8..=u16::MAX => M2PixelShader::OpaqueMod,
        }),
        1 => Some(match second {
            0 => M2PixelShader::ModOpaque,
            1 => M2PixelShader::ModMod,
            3 => M2PixelShader::ModAdd,
            4 => M2PixelShader::ModMod2x,
            6 => M2PixelShader::ModMod2xNoAlpha,
            7 => M2PixelShader::ModAddNoAlpha,
            2 | 5 | 8..=u16::MAX => M2PixelShader::ModMod,
        }),
        3 if second == 1 => Some(M2PixelShader::AddMod),
        4 if second == 4 => Some(M2PixelShader::Mod2xMod2x),
        2 | 3 | 4 | 5 | 6 | 7 | 8..=u16::MAX => None,
    }
}

/// Maps the three high-bit effects implemented by build 12340.
const fn specialized_effect(shader: u16) -> Option<(M2VertexShader, M2PixelShader)> {
    match shader & !SPECIALIZED_BIT {
        1 => Some((
            M2VertexShader::DiffuseT1Env,
            M2PixelShader::OpaqueMod2xNoAlphaAlpha,
        )),
        2 => Some((M2VertexShader::DiffuseT1Env, M2PixelShader::OpaqueAddAlpha)),
        3 => Some((
            M2VertexShader::DiffuseT1Env,
            M2PixelShader::OpaqueAddAlphaAlpha,
        )),
        0 | 4..=u16::MAX => None,
    }
}

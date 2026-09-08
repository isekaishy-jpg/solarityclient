//! Distinct positioned and attached CEffect scale rules.

use glam::Vec3;
use solarity_asset::SpellVisualEffectDefinition;

/// The three authored model-scale fields, independent of area-effect size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitEffectScale {
    /// Authored model scale multiplier.
    pub multiplier: f32,
    /// Minimum final world scale.
    pub minimum: f32,
    /// Maximum final world scale.
    pub maximum: f32,
}

impl From<&SpellVisualEffectDefinition> for UnitEffectScale {
    fn from(definition: &SpellVisualEffectDefinition) -> Self {
        Self {
            multiplier: definition.scale(),
            minimum: definition.min_scale(),
            maximum: definition.max_scale(),
        }
    }
}

impl UnitEffectScale {
    /// Resolves native `0x006F8AE0`'s positioned model scale.
    ///
    /// `world_factor` is the spilled result of `unit_world_effect_factor`;
    /// `unit_scale` is the owner's virtual scale. Pass ones when the effect's
    /// scale-with-owner bit is clear or it has no owner. Nonpositive final
    /// values fall back to one only on this positioned path.
    #[must_use]
    pub fn positioned(self, world_factor: f32, unit_scale: f32) -> f32 {
        let value = f64::from(world_factor) * f64::from(unit_scale) * f64::from(self.multiplier);
        let scale = if value < f64::from(self.minimum) {
            f64::from(self.minimum)
        } else if value < f64::from(self.maximum) {
            value
        } else {
            f64::from(self.maximum)
        };
        if scale <= 0.0 { 1.0 } else { scale as f32 }
    }

    /// Resolves native `0x006F8C50`'s local scale beneath an animated attachment.
    ///
    /// `model_effect_scale` is CreatureModelData's attached-effect multiplier,
    /// or one without that model row. `attachment_scale` is the length of the
    /// attachment bone transform's X basis, including parent world scale. The
    /// clamp applies to their product while the returned factor stays local
    /// to the attachment. Near-zero products skip division and clamping.
    #[must_use]
    pub fn attached(self, model_effect_scale: f32, attachment_scale: f64) -> f32 {
        let local = f64::from(model_effect_scale * self.multiplier);
        let world = attachment_scale * local;
        if world > f64::from(f32::from_bits(0x3586_37bd)) {
            if world > f64::from(self.maximum) {
                return ((local / world) * f64::from(self.maximum)) as f32;
            }
            if world < f64::from(self.minimum) {
                return (local * (f64::from(self.minimum) / world)) as f32;
            }
        }
        local as f32
    }
}

/// Computes native `0x006F7950`'s horizontal model extent and world-effect factor.
///
/// The caller supplies the current model bounds and CreatureModelData's world
/// effect multiplier (one without that row). A model that is still loading
/// bypasses this helper and uses one. The result spills to float before the
/// positioned factory multiplies it by owner scale.
#[must_use]
pub fn unit_world_effect_factor(minimum: Vec3, maximum: Vec3, model_effect_scale: f32) -> f32 {
    let width = f64::from(maximum.x) - f64::from(minimum.x);
    let depth = f64::from(maximum.y) - f64::from(minimum.y);
    let extent = width.min(depth) * f64::from(0.3_f32);
    (extent.max(1.0) * f64::from(model_effect_scale)) as f32
}

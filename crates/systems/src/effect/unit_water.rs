//! Unit breath and shallow-water spray decisions from build 12340.

use glam::Vec3;

/// Slots in CEffect's named model bank, not SpellVisualEffectName identifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum UnitWaterEffect {
    /// Native slot zero (selected at or below twice the unit's walk speed).
    RunSpray = 0,
    /// Native slot one (selected above twice the unit's walk speed).
    WalkSpray = 1,
    /// Underwater breath, attached to the animated mouth.
    UnderwaterBreath = 2,
    /// Cold-area breath, attached to the animated mouth.
    ColdBreath = 3,
    /// Player inebriation takes precedence over the environment breath flags.
    InebriatedBubbles = 7,
}

impl UnitWaterEffect {
    /// Returns the exact name compared during native `0x006F7520` startup.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RunSpray => "HARDCODED Footstep Water Run Spray",
            Self::WalkSpray => "HARDCODED Footstep Water Walk Spray",
            Self::UnderwaterBreath => "HARDCODED Breath Underwater",
            Self::ColdBreath => "HARDCODED Breath Cold",
            Self::InebriatedBubbles => "HARDCODED Inebriated Bubbles",
        }
    }

    /// Returns the native model attachment, or none for positioned spray.
    #[must_use]
    pub const fn attachment(self) -> Option<u32> {
        match self {
            Self::RunSpray | Self::WalkSpray => None,
            Self::UnderwaterBreath | Self::ColdBreath | Self::InebriatedBubbles => Some(17),
        }
    }
}

/// Unit registration inputs sampled by native `0x0071FA90`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitBreathEnvironment {
    /// Unit model height at native offset `+0xAC`.
    pub model_height: f32,
    /// Unit scale at native offset `+0x98`.
    pub unit_scale: f32,
    /// Unit origin, independent of the animated mouth and camera.
    pub origin_z: f32,
    /// Surface from the unit's current liquid registration, if present.
    pub liquid_surface: Option<f32>,
    /// Cold-area result from the unit's registration.
    pub cold_area: bool,
}

/// Cached unit breath bits and their wrapping ten-second refresh deadline.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnitBreathState {
    effect: Option<UnitWaterEffect>,
    deadline_ms: u32,
}

impl UnitBreathState {
    /// Refreshes after model registration or when the periodic unit timer fires.
    pub fn refresh(&mut self, now_ms: u32, environment: UnitBreathEnvironment) {
        // 71FACD spills this product before the x87 depth comparison. The
        // original constant at 9EBF34 is 5.0, not a mouth-height epsilon.
        let height = environment.model_height * environment.unit_scale;
        self.effect = if environment.liquid_surface.is_some_and(|surface| {
            f64::from(height) + 5.0 < f64::from(surface) - f64::from(environment.origin_z)
        }) {
            Some(UnitWaterEffect::UnderwaterBreath)
        } else if environment.cold_area {
            Some(UnitWaterEffect::ColdBreath)
        } else {
            None
        };
        self.deadline_ms = now_ms.wrapping_add(10_000);
    }

    /// Tests native `0x0073DAB0`'s signed wrapping comparison before unit update.
    #[must_use]
    pub const fn refresh_due(self, now_ms: u32) -> bool {
        now_ms.wrapping_sub(self.deadline_ms) as i32 >= 0
    }

    /// Resolves one authored `$BTH` callback without changing its timer.
    ///
    /// `player_inebriation` is absent for non-player units. A missing model
    /// definition suppresses breath, as does its flags bit 1. Object-manager
    /// context mode one suppresses the complete callback. The inebriation
    /// result has not spilled to float at the native comparison.
    #[must_use]
    pub fn effect(
        self,
        object_context_mode: u32,
        model_flags: Option<u32>,
        player_inebriation: Option<f64>,
    ) -> Option<UnitWaterEffect> {
        if object_context_mode == 1 || model_flags? & 2 != 0 {
            return None;
        }
        if player_inebriation.is_some_and(|value| value >= 0.5) {
            Some(UnitWaterEffect::InebriatedBubbles)
        } else {
            self.effect
        }
    }
}

/// Unit foot-contact admission, surface, and model inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitWaterSprayInput {
    /// Authored foot-contact world position, not the unit origin.
    pub foot: Vec3,
    /// Current camera position used by the native 25-unit distance test.
    pub camera: Vec3,
    /// Current unit origin Z used to compute registered depth.
    pub origin_z: f32,
    /// Model height retained at native unit offset `+0x854`.
    pub model_height: f32,
    /// Exact registered liquid type (zero suppresses the effect).
    pub liquid_id: u32,
    /// Registered liquid surface Z.
    pub liquid_surface: f32,
    /// Backward and non-colliding flight movement flags suppress spray.
    pub movement_flags: u32,
    /// A nonzero mounted display suppresses foot-contact particles.
    pub mount_display_id: u32,
    /// Hovering is byte two bit one of `UNIT_FIELD_BYTES_1`.
    pub unit_bytes1: u32,
    /// Player flags, absent for creatures; ghost bit four suppresses particles.
    pub player_flags: Option<u32>,
    /// A nonzero transport GUID suppresses foot contact.
    pub transport_guid: u64,
    /// CreatureModelData flag bit 0 suppresses foot-contact particles.
    pub model_flags: u32,
    /// `showfootprintparticles`, independent of footstep audio.
    pub enabled: bool,
    /// Current native unit movement speed.
    pub speed: f64,
    /// Unit walk speed at native offset `+0x818`.
    pub walk_speed: f32,
}

impl UnitWaterSprayInput {
    /// Resolves the water branch of native `0x00723A50`.
    ///
    /// This method only handles a present registered liquid; dry ground has its
    /// own TerrainType effects. The authored slot names are intentionally
    /// retained even though their speed selection appears reversed.
    #[must_use]
    pub fn resolve(self) -> Option<(UnitWaterEffect, Vec3)> {
        if !self.enabled
            || self.movement_flags & 0x4000_0002 != 0
            || self.model_flags & 1 != 0
            || self.liquid_id == 0
            || self.mount_display_id != 0
            || self.transport_guid != 0
            || self.unit_bytes1 & 0x0002_0000 != 0
            || self.player_flags.is_some_and(|flags| flags & 0x10 != 0)
        {
            return None;
        }
        let delta = self.foot.as_dvec3() - self.camera.as_dvec3();
        let distance_squared = (delta.z * delta.z + delta.y * delta.y + delta.x * delta.x) as f32;
        let nearby = distance_squared <= 625.0;
        if !nearby {
            return None;
        }
        let depth = self.liquid_surface - self.origin_z;
        let shallow = f64::from(depth) < f64::from(self.model_height) * 0.5;
        if !shallow {
            return None;
        }
        let kind = if self.speed <= f64::from(self.walk_speed) * 2.0 {
            UnitWaterEffect::RunSpray
        } else {
            UnitWaterEffect::WalkSpray
        };
        Some((
            kind,
            Vec3::new(self.foot.x, self.foot.y, depth + self.foot.z),
        ))
    }
}

/// Resolves native `0x004F7290`'s player inebriation without a float spill.
///
/// Inputs are PLAYER_BYTES_3 byte one and PLAYER_FAKE_INEBRIATION. The latter
/// participates in a signed comparison. The original 0.01 float multiplier
/// makes a percentage of 50 slightly less than 0.5; breath therefore changes
/// to bubbles at 51, which would be lost by rounding this return to float.
#[must_use]
pub fn unit_player_inebriation(actual: u8, fake: u32) -> f64 {
    f64::from(i32::from(actual).max(fake as i32).min(100)) * f64::from(0.01_f32)
}

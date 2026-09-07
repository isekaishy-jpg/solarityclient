//! Unit_C's 0x0071CBA0 ripple notifications and shared-random draw order.

use glam::Vec3;
use thiserror::Error;

const MIN_RADIUS: f32 = f32::from_bits(0x3eaa_aaab);
const MAX_RADIUS: f32 = f32::from_bits(0x3fd5_5555);
const BASE_STRENGTH: f32 = f32::from_bits(0x3e2a_aaab);

/// Registered unit inputs sampled after its world position is published.
#[derive(Clone, Copy, Debug)]
pub struct WaterRippleUnit {
    /// Unit origin in world coordinates, before replacing Z with the surface.
    pub position: Vec3,
    /// Current registered liquid surface, if the unit has one.
    pub surface: Option<f32>,
    /// Area-resolved LiquidType flag 1, admitted by the spatial wet policy.
    pub permits_ripples: bool,
    /// Model height stored by Unit_C, in world units.
    pub height: f32,
    /// Object scale returned by the unit's virtual scale getter.
    pub scale: f32,
    /// World-facing angle in radians.
    pub yaw: f32,
    /// Primary movement flags used to select idle, turning, or directional wake.
    pub movement_flags: u32,
    /// Current movement speed used for directional wake spacing.
    pub speed: f32,
    /// Selects the native pool's 32 local-player slots.
    pub local_player: bool,
}

/// One 0x0077F400 request, before the scene pool's strength normalization.
#[derive(Clone, Copy, Debug)]
pub struct WaterRippleEmission {
    /// World-space center on the registered surface.
    pub position: Vec3,
    /// Wake direction or the idle ripple's randomized rotation.
    pub yaw: f32,
    /// Initial projected radius in world units.
    pub radius: f32,
    /// Complete envelope duration in seconds.
    pub lifetime: f32,
    /// Native pre-normalization alpha; 0x0079D460 scales values below 1/6 by six.
    pub strength: f32,
    /// Requested radial growth per second, before record rounding.
    pub growth: f32,
    /// Selects the authored wake texture rather than the circular splash.
    pub directional: bool,
    /// Retains local-player pool ownership independently of direction.
    pub local_player: bool,
}

/// The unit's wrapping millisecond deadline; zero permits immediate emission.
#[derive(Clone, Copy, Debug, Default)]
pub struct WaterRippleClock {
    /// Unit_C offset 0xA58, retained for the same unit lifetime.
    pub next_emission_ms: u32,
}

/// Invalid scalars at the unit ripple boundary.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("water ripple requires finite unit geometry and nonnegative dimensions and speed")]
pub struct WaterRippleError;

impl WaterRippleClock {
    /// Evaluates native notification zero (periodic), 0xC9 (immersion crossing),
    /// or another nonzero notification (forced idle). Draws occur only after
    /// registration, depth, and deadline admission. The caller supplies the
    /// process's shared Blizzard random stream.
    ///
    /// # Errors
    /// Rejects invalid unit scalars without advancing the deadline or random stream.
    pub fn emit(
        &mut self,
        unit: WaterRippleUnit,
        notification: u32,
        now_ms: u32,
        mut random_word: impl FnMut() -> u32,
    ) -> Result<Option<WaterRippleEmission>, WaterRippleError> {
        if !unit.position.is_finite()
            || !unit.yaw.is_finite()
            || [unit.height, unit.scale, unit.speed]
                .iter()
                .any(|value| !value.is_finite() || *value < 0.)
            || !(unit.height * 2.).is_finite()
            || unit.surface.is_some_and(|surface| !surface.is_finite())
        {
            return Err(WaterRippleError);
        }
        let Some(surface) = unit.surface.filter(|_| unit.permits_ripples) else {
            return Ok(None);
        };
        let kind = if notification != 0 {
            self.next_emission_ms = 0;
            if notification == 0xc9 { 3 } else { 0 }
        } else if unit.movement_flags & 0xf != 0 {
            2
        } else if unit.movement_flags & 0x30 != 0 {
            1
        } else {
            0
        };
        let maximum_depth = (unit.height * 2.).max(1.);
        let half_depth = maximum_depth * 0.5;
        let depth = f64::from(surface) - f64::from(unit.position.z);
        if depth >= f64::from(maximum_depth)
            || (self.next_emission_ms != 0
                && (now_ms.wrapping_sub(self.next_emission_ms) as i32) < 0)
        {
            return Ok(None);
        }
        let directional = kind == 2;
        // x87 retains the division until the reciprocal is rounded separately.
        let spacing = if directional && unit.speed > f32::from_bits(0x38d1_b717) {
            2.5 / f64::from(unit.speed.min(20.))
        } else {
            1.
        };
        let growth_speed = (1. / spacing) as f32;
        let radius_scale = unit.scale * MIN_RADIUS;
        let mut radius =
            (((random_float(&mut random_word) * f64::from(0.2_f32) - f64::from(0.1_f32)) + 1.)
                * f64::from(radius_scale))
            .clamp(f64::from(MIN_RADIUS), f64::from(MAX_RADIUS)) as f32;
        let mut lifetime = ((random_float(&mut random_word) * f64::from(0.1_f32)
            - f64::from(0.05_f32))
            + f64::from(0.65_f32)) as f32;
        let mut growth = ((random_float(&mut random_word) * 1.5 - 0.75) + 3.75)
            * 12.
            * f64::from(f32::from_bits(0x3ce3_8e39))
            * f64::from(growth_speed);
        let mut strength = f64::from(BASE_STRENGTH);
        let depth = depth as f32;
        let depth_factor = if half_depth < depth {
            0.5 + ((f64::from(maximum_depth) - f64::from(depth))
                / (f64::from(maximum_depth) - f64::from(half_depth)))
                * 0.5
        } else {
            1.
        };
        strength *= depth_factor;
        lifetime = (depth_factor * f64::from(lifetime)) as f32;
        radius = (depth_factor * f64::from(radius)) as f32;
        if kind == 0 {
            strength *= f64::from(0.8_f32);
            growth *= 0.25;
            radius *= 0.6_f32;
        }
        let yaw = if directional {
            directional_yaw(unit.yaw, unit.movement_flags)
        } else {
            (random_float(&mut random_word) * f64::from(std::f32::consts::TAU)) as f32
        };
        self.next_emission_ms = if directional {
            // 0x71CF28 switches x87 to truncation, including before clock wrap.
            let negative_delay =
                (f64::from(depth_factor as f32) * f64::from(spacing as f32) * 0.25 * -1000.) as i64;
            now_ms.wrapping_sub(negative_delay as u32)
        } else {
            now_ms
                .wrapping_add(400)
                .wrapping_add(((u64::from(random_word()) * 50) >> 32) as u32)
        };
        Ok(Some(WaterRippleEmission {
            position: Vec3::new(unit.position.x, unit.position.y, surface),
            yaw,
            radius,
            lifetime,
            strength: strength as f32,
            growth: growth as f32,
            directional,
            local_player: unit.local_player,
        }))
    }
}

/// 0x004C1590 replaces the mantissa of 1.0 and subtracts one in x87 precision.
fn random_float(random_word: &mut impl FnMut() -> u32) -> f64 {
    f64::from(f32::from_bits((random_word() & 0x7f_ffff) | 0x3f80_0000)) - 1.
}

/// Native strafe precedence includes the simultaneous forward/backward flags.
fn directional_yaw(yaw: f32, flags: u32) -> f32 {
    let strafe = if flags & 1 != 0 {
        std::f32::consts::FRAC_PI_4
    } else if flags & 2 != 0 {
        f32::from_bits(0x4016_cbe4)
    } else {
        std::f32::consts::FRAC_PI_2
    };
    let offset = if flags & 4 != 0 {
        strafe
    } else if flags & 8 != 0 {
        -strafe
    } else if flags & 2 != 0 {
        std::f32::consts::PI
    } else {
        0.
    };
    (f64::from(yaw) + f64::from(offset)) as f32
}

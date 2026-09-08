//! Executable-owned outdoor directional-light motion.

use glam::Vec3;

const DAY_HALF_MINUTES: u32 = 2_880;
const INVERSE_PI: f32 = 0.318_309_87;

/// Returns build 12340's normalized surface-to-exterior-light direction.
///
/// The executable uses a cubic periodic approximation and a fixed four-key
/// polar-angle table independently of Light.dbc. `half_minutes` wraps over
/// the client's 2,880-unit day.
#[must_use]
pub fn exterior_light_direction(half_minutes: u32) -> Vec3 {
    exterior_light_direction_at((half_minutes % DAY_HALF_MINUTES) as f32 / DAY_HALF_MINUTES as f32)
}

/// Samples the continuous native day table shared with the sky ephemeris.
#[must_use]
pub fn exterior_light_direction_at(day_fraction: f32) -> Vec3 {
    -exterior_light_ray_at(day_fraction).normalize()
}

/// Returns 7EEA90's stored ray before scene-light normalization.
/// Interior entity callbacks blend this raw palette direction before normalizing.
#[must_use]
pub fn exterior_light_ray_at(day_fraction: f32) -> Vec3 {
    const THETA: f32 = 3.926_991;
    let phi = directional_phi(day_fraction);
    let sin_phi = stock_periodic_approximation(phi * INVERSE_PI - 0.5);
    let cos_phi = stock_periodic_approximation(phi * INVERSE_PI);
    let sin_theta = stock_periodic_approximation(THETA * INVERSE_PI - 0.5);
    let cos_theta = stock_periodic_approximation(THETA * INVERSE_PI);
    // Stock shaders negate their stored ray before N.L. The shared contract
    // stores surface-to-light and performs that conversion exactly once.
    Vec3::new(sin_phi * cos_theta, sin_phi * sin_theta, cos_phi)
}

/// Reproduces the small cubic periodic approximation in the native sky path.
fn stock_periodic_approximation(value: f32) -> f32 {
    let period = if value <= 0.0 {
        !(-value as i32)
    } else {
        value as i32
    };
    let fraction = value - period as f32;
    let mut result = (fraction * 4.0 - 6.0) * fraction * fraction + 1.0;
    if period & 1 != 0 {
        result = -result;
    }
    result
}

/// Interpolates the executable's four alternating polar-angle keys.
fn directional_phi(day_fraction: f32) -> f32 {
    const VALUES: [f32; 4] = [2.216_568_2, 1.919_862_3, 2.216_568_2, 1.919_862_3];
    let cycle = day_fraction.clamp(0.0, 1.0);
    let scaled = cycle * 4.0;
    let left = scaled as usize % VALUES.len();
    let right = (left + 1) % VALUES.len();
    let amount = scaled - scaled.floor();
    VALUES[left] + (VALUES[right] - VALUES[left]) * amount
}

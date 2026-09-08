//! Build 12340's table-driven sun and two moon positions and sizes.

use super::sky::cyclic;
use glam::Vec3;

const SUN_POLAR: [[f32; 2]; 5] = [
    [0.229_166_67, 1.745_329_3],
    [0.496_527_8, 0.087_266_47],
    [0.5, 0.087_266_47],
    [0.503_472_2, 0.087_266_47],
    [0.895_833_3, 1.745_329_3],
];
const SUN_AZIMUTH: [[f32; 2]; 3] = [
    [0.229_166_67, std::f32::consts::FRAC_PI_4],
    [0.5, std::f32::consts::FRAC_PI_4],
    [0.895_833_3, std::f32::consts::FRAC_PI_4],
];
const MOON_POLAR: [[f32; 2]; 5] = [
    [0., 0.610_865_24],
    [0.003_472_222_2, 0.610_865_24],
    [0.166_666_67, 1.745_329_3],
    [0.916_666_7, 1.745_329_3],
    [0.996_527_8, 0.610_865_24],
];
const MOON_AZIMUTH: [[f32; 2]; 3] = [
    [0., std::f32::consts::FRAC_PI_4],
    [0.166_666_67, std::f32::consts::FRAC_PI_4],
    [0.916_666_7, std::f32::consts::FRAC_PI_4],
];
const SECOND_MOON_AZIMUTH: [[f32; 2]; 3] = [
    [0., 2.356_194_5],
    [0.166_666_67, 2.617_993_8],
    [0.916_666_7, 2.879_793_4],
];
const SUN_SIZE: [[f32; 2]; 4] = [[0.25, 2.], [0.28125, 1.], [0.84375, 1.], [0.875, 2.]];
const MOON_SIZE: [[f32; 2]; 4] = [
    [0.041_666_67, 1.],
    [0.166_666_67, 1.5],
    [0.916_666_7, 1.5],
    [0.999_305_6, 1.],
];

/// One native camera-relative celestial object's world position and quad size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCelestialBody {
    position: Vec3,
    size: f32,
}

impl WorldCelestialBody {
    /// Returns the native camera-centered sphere position in world space.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns the native sun/moon billboard size for this day phase.
    #[must_use]
    pub const fn size(self) -> f32 {
        self.size
    }
}

/// The original ephemeris shared by sky drawing and cloud lighting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCelestials {
    bodies: [WorldCelestialBody; 3],
    daylight: f32,
}

impl WorldCelestials {
    /// Runs 7EECC0 at a cyclic day fraction and the native calendar day index.
    /// The second moon uses the original 1.7-day period and 16-bit phase rounding.
    #[must_use]
    pub fn sample(day: f32, calendar_days: f32, eye: Vec3) -> Self {
        let second = second_moon_phase(day, calendar_days);
        Self {
            bodies: [
                body(&SUN_POLAR, &SUN_AZIMUTH, &SUN_SIZE, day, 1., eye),
                body(&MOON_POLAR, &MOON_AZIMUTH, &MOON_SIZE, day, 1.75, eye),
                body(
                    &MOON_POLAR,
                    &SECOND_MOON_AZIMUTH,
                    &MOON_SIZE,
                    second,
                    1.,
                    eye,
                ),
            ],
            daylight: daylight(day),
        }
    }

    /// Returns sun, first moon and second moon in the native draw order.
    #[must_use]
    pub const fn bodies(self) -> [WorldCelestialBody; 3] {
        self.bodies
    }

    /// Returns 7EECC0's retained day/night lighting scalar.
    #[must_use]
    pub const fn daylight(self) -> f32 {
        self.daylight
    }
}

fn body(
    polar: &[[f32; 2]],
    azimuth: &[[f32; 2]],
    size: &[[f32; 2]],
    phase: f32,
    scale: f32,
    eye: Vec3,
) -> WorldCelestialBody {
    let (sin_polar, cos_polar) = trigonometry(cyclic(polar, phase) as f32);
    let (sin_azimuth, cos_azimuth) = trigonometry(cyclic(azimuth, phase) as f32);
    let x = sin_polar * cos_azimuth;
    let y = sin_polar * sin_azimuth;
    let z = f64::from(cos_polar as f32);
    let length = (x * x + y * y + z * z).sqrt();
    let radius = 12. / length;
    WorldCelestialBody {
        position: Vec3::from_array(std::array::from_fn(|i| {
            ([x, y, z][i] * radius + f64::from(eye[i])) as f32
        })),
        size: (cyclic(size, phase) * f64::from(scale)) as f32,
    }
}

fn trigonometry(angle: f32) -> (f64, f64) {
    let phase = f64::from(angle) * f64::from(0.318_309_87_f32);
    (
        f64::from(cubic((phase - 0.5) as f32) as f32),
        cubic(phase as f32),
    )
}

fn cubic(phase: f32) -> f64 {
    let period = if phase > 0. {
        phase as i32
    } else {
        phase as i32 - 1
    };
    let fraction = f64::from(phase - period as f32);
    let value = 1. - (6. - 4. * fraction) * fraction * fraction;
    if period & 1 == 0 { value } else { -value }
}

fn second_moon_phase(day: f32, calendar_days: f32) -> f32 {
    let quantize = |value: f32| (f64::from(value) - 0.5).round_ties_even() as i32 as u32;
    let total = quantize(calendar_days * 65536.).wrapping_add(quantize(day * 65536.));
    let period = f64::from(1.7_f32);
    let cycles = ((f64::from(calendar_days) + f64::from(day)) / period).floor();
    let consumed = quantize((cycles * period * 65536.) as f32).min(total);
    (f64::from(total.wrapping_sub(consumed)) * (1. / 65536.) / period) as f32
}

fn daylight(day: f32) -> f32 {
    let day = f64::from(day);
    if day >= f64::from(0.229_166_67_f32) && day < 0.5 {
        ((day - f64::from(0.229_166_67_f32)) * f64::from(3.692_307_7_f32)) as f32
    } else if day >= 0.5 && day < f64::from(0.895_833_3_f32) {
        (1. - (day - 0.5) * f64::from(2.526_316_f32)) as f32
    } else if day >= f64::from(0.916_666_7_f32) && day < 1. {
        ((day - f64::from(0.916_666_7_f32)) * f64::from(12.000_003_f32)) as f32
    } else if day >= 0. && day < f64::from(0.166_666_67_f32) {
        (1. - day * 6.) as f32
    } else {
        0.
    }
}

#[cfg(test)]
#[path = "../../tests/stock_seed/celestial_native.rs"]
mod tests;

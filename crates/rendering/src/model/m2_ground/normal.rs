//! Unit_C's retained presentation normal at offsets `0x9D8..0x9E0`.

use glam::Vec3;

use super::M2GroundPlacementError;

const MINIMUM_NORMAL_Z: f32 = f32::from_bits(0x3eb6_e48a);
const SMOOTHING_BASE: f64 = f64::from_bits(0x3f5d_7dbf_4000_0000);

/// Surface smoothing retained across model replacements within one unit lifetime.
///
/// The movement owner supplies a world-space normal, including its passenger
/// frame rotation. The unit constructor starts upright. The scene callback at
/// `0x007197D0` smooths accepted samples without normalizing the retained vector.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2GroundNormal {
    normal: Vec3,
}

impl Default for M2GroundNormal {
    fn default() -> Self {
        Self { normal: Vec3::Z }
    }
}

impl M2GroundNormal {
    /// Advances one admitted scene callback using its elapsed seconds.
    ///
    /// Samples below stock's minimum Z leave the previous vector untouched.
    /// Subtraction stores float components before the extended decay products.
    ///
    /// # Errors
    /// Rejects nonfinite samples or negative/nonfinite intervals without mutation.
    pub fn advance(
        &mut self,
        target: Vec3,
        delta_seconds: f32,
    ) -> Result<(), M2GroundPlacementError> {
        if !target.is_finite() || !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(M2GroundPlacementError::InvalidSample);
        }
        if target.z >= MINIMUM_NORMAL_Z {
            let difference = self.normal - target;
            let decay = SMOOTHING_BASE.powf(f64::from(delta_seconds));
            let normal = (difference.as_dvec3() * decay + target.as_dvec3()).as_vec3();
            if !normal.is_finite() {
                return Err(M2GroundPlacementError::InvalidSample);
            }
            self.normal = normal;
        }
        Ok(())
    }

    /// Returns the smoothed world-space vector, which need not have unit length.
    #[must_use]
    pub const fn normal(self) -> Vec3 {
        self.normal
    }
}

//! `0x0082DD80` selects model tilt and blends toward full normal alignment.

use glam::{Mat4, Vec3};

use super::{M2GroundNormal, M2GroundPlacementError};

const NORMALIZATION_THRESHOLD: f64 = f32::from_bits(0x3480_0000) as f64;

impl M2GroundNormal {
    /// Places a unit model using its global flags and primary sequence blend.
    ///
    /// Masked global flags `1` select pitch, `3` select full normal alignment,
    /// and `0`/`2` retain heading. An admitted primary animation can blend that
    /// basis toward full alignment before scale. The caller supplies stock's
    /// sequence-dependent weight, or zero when no override applies.
    ///
    /// # Errors
    /// Rejects invalid placement inputs or nonfinite resulting matrix components.
    pub fn transform(
        self,
        position: Vec3,
        orientation: f32,
        scale: f32,
        model_flags: u32,
        full_alignment_weight: f32,
    ) -> Result<Mat4, M2GroundPlacementError> {
        if !position.is_finite()
            || !orientation.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
            || !full_alignment_weight.is_finite()
            || !(0.0..=1.0).contains(&full_alignment_weight)
        {
            return Err(M2GroundPlacementError::InvalidPlacement);
        }
        let (sin, cos) = f64::from(orientation).sin_cos();
        // 4C3380 multiplies the yaw matrix by identity; its zero additions
        // canonicalize signed zero before the optional tilt branches.
        let forward = Vec3::new(cos as f32 + 0.0, sin as f32 + 0.0, 0.0);
        let heading = [
            forward,
            Vec3::new(-sin as f32 + 0.0, cos as f32 + 0.0, 0.0),
            Vec3::Z,
        ];
        let mut basis = match model_flags & 3 {
            1 => {
                let side = normalize(Vec3::new(-forward.y, forward.x, 0.0));
                let forward = normalize(cross(side, self.normal()));
                [forward, side, cross(forward, side)]
            }
            3 => full_basis(forward, self.normal()),
            _ => heading,
        };
        let mut translation = position;
        if full_alignment_weight > 0.0 {
            let full = full_basis(forward, self.normal());
            let retained = 1.0 - full_alignment_weight;
            for (axis, full) in basis.iter_mut().zip(full) {
                // The two 4C2120 calls store scaled matrices before 4C1E60
                // adds them. Do not combine into one fused interpolation.
                *axis = *axis * retained + full * full_alignment_weight;
            }
            translation = position * retained + position * full_alignment_weight;
        }
        let transform = Mat4::from_cols(
            (basis[0] * scale).extend(0.0),
            (basis[1] * scale).extend(0.0),
            (basis[2] * scale).extend(0.0),
            translation.extend(1.0),
        );
        if !transform.is_finite() {
            return Err(M2GroundPlacementError::InvalidPlacement);
        }
        Ok(transform)
    }
}

/// 824A80 normalizes the side axis only; the retained normal is not normalized.
fn full_basis(forward: Vec3, normal: Vec3) -> [Vec3; 3] {
    let side = normalize(cross(normal, forward));
    [cross(side, normal), side, normal]
}

/// Native cross products retain the products until each destination float store.
fn cross(left: Vec3, right: Vec3) -> Vec3 {
    left.as_dvec3().cross(right.as_dvec3()).as_vec3()
}

/// 4C3600 and 824A80 preserve degenerate vectors at the same strict threshold.
fn normalize(value: Vec3) -> Vec3 {
    let extended = value.as_dvec3();
    let squared = extended.length_squared();
    if squared > NORMALIZATION_THRESHOLD {
        (extended * (1.0 / squared.sqrt())).as_vec3()
    } else {
        value
    }
}

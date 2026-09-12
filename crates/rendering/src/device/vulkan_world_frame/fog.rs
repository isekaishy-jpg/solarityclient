//! Retained 873210 fog registers, shared by successive scene submissions.

use glam::{Vec3, Vec4};

use crate::M2FogMode;

/// Native globals begin zeroed. Disabling fog changes the uploaded vertex
/// coefficients, not this retained bank or the fragment fog color.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SubmissionFog {
    coefficients: Vec4,
    color: Vec3,
}

impl SubmissionFog {
    pub(super) fn publish(&mut self, parameters: Vec4, mode: M2FogMode, color: Vec3) {
        if mode == M2FogMode::Disabled || parameters.y <= parameters.x {
            return;
        }
        let reciprocal = (parameters.y - parameters.x).recip();
        self.coefficients = Vec4::new(-reciprocal, parameters.y * reciprocal, parameters.w, 0.);
        self.color = match mode {
            M2FogMode::Disabled => unreachable!(),
            M2FogMode::SceneColor => {
                // 81FB10 converts the selected bank to packed RGB before
                // 873210 multiplies its bytes by the GX reciprocal.
                (color.clamp(Vec3::ZERO, Vec3::ONE).as_dvec3() * 255. + glam::DVec3::splat(0.5))
                    .floor()
                    .as_vec3()
                    * (1. / 255.)
            }
            M2FogMode::Black => Vec3::ZERO,
            M2FogMode::White => Vec3::ONE,
            M2FogMode::HalfWhite => Vec3::splat(128. * (1. / 255.)),
        };
    }

    pub(super) fn push_bytes(self, enabled: bool) -> [u8; 32] {
        let coefficients = if enabled {
            self.coefficients
        } else {
            Vec4::new(0., 1., 1., 0.)
        };
        let mut bytes = [0u8; 32];
        for (word, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(
            coefficients
                .to_array()
                .into_iter()
                .chain(self.color.extend(0.).to_array()),
        ) {
            *word = value.to_le_bytes();
        }
        bytes
    }
}

//! Ordinary followed-subject opacity from build-12340 CGCamera::Update (606F90).

/// Resolves the ordinary camera opacity byte using the primary constrained
/// distance, principal subject height/pitch, and camera near clip. Water eye
/// correction and final pivot pitch do not participate in these inputs.
/// Vehicle bounds, timed camera modes and the reduced-range global are separate
/// native policies and are not represented by this ordinary follow function.
#[must_use]
pub fn player_camera_opacity(distance: f32, height: f32, pitch: f32, near: f32) -> u8 {
    let mut upper = f64::from(f32::from_bits(0x3fea_6e99));
    let lower = f64::from(f32::from_bits(0x3b36_0b61));
    if distance < height && pitch < f32::from_bits(0xbf97_e9d8) {
        let factor = (f64::from(f32::from_bits(0xbfc9_0fdb)) - f64::from(pitch))
            * f64::from(f32::from_bits(0xc026_adb8));
        if factor.abs() >= f64::from(f32::from_bits(0x3480_0000)) {
            upper /= factor * factor;
        }
    }
    let effective = f64::from(distance) - f64::from(near);
    if effective >= upper {
        return 255;
    }
    if effective <= lower {
        return 0;
    }
    // The native caller stores t as f32 before 8CA080; cosine arithmetic stays
    // on x87. 607991 selects truncation before converting the result to a byte.
    let t = ((effective - lower) / (upper - lower)) as f32;
    let angle = f64::from(t) * f64::from(std::f32::consts::PI);
    ((1.0 - angle.cos()) * 0.5 * 255.0) as u8
}

#[cfg(test)]
#[path = "../../tests/camera/opacity.rs"]
mod tests;

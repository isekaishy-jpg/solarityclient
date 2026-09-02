//! External stock-compatibility tests for model and world light preparation.

use glam::Vec3;
use solarity_rendering::{
    M2DirectionalLight, WorldCamera, glue_character_sunlight, merge_wotlk_directional_lights,
};

/// SetupSunlight normalizes and reverses one D3D ray while clamping diffuse.
#[test]
fn wotlk_sunlight_merger_converts_one_directional_source() -> Result<(), &'static str> {
    let sunlight = merge_wotlk_directional_lights(&[M2DirectionalLight::new(
        Vec3::new(0.0, 0.0, -2.0),
        Vec3::splat(0.27),
        Vec3::new(1.2, 0.8, 0.4),
    )])
    .ok_or("one directional source did not produce sunlight")?;

    assert_eq!(sunlight.direction(), Vec3::Z);
    assert_eq!(sunlight.ambient(), Vec3::splat(0.27));
    assert_eq!(sunlight.diffuse(), Vec3::new(1.0, 0.8, 0.4));
    Ok(())
}

/// Opposed equal sources exercise stock's default-direction and fill terms.
#[test]
fn wotlk_sunlight_merger_handles_cancelled_luminance() -> Result<(), &'static str> {
    let sunlight = merge_wotlk_directional_lights(&[
        M2DirectionalLight::new(Vec3::X, Vec3::splat(0.1), Vec3::splat(0.4)),
        M2DirectionalLight::new(-Vec3::X, Vec3::splat(0.1), Vec3::splat(0.4)),
    ])
    .ok_or("two directional sources did not produce sunlight")?;

    assert_eq!(sunlight.direction(), Vec3::Z);
    assert_eq!(sunlight.ambient(), Vec3::splat(0.4));
    assert_eq!(sunlight.diffuse(), Vec3::ZERO);
    assert!(merge_wotlk_directional_lights(&[]).is_none());
    Ok(())
}

/// Missing ModelFFX character lights use stock's camera-relative warm profile.
#[test]
fn glue_character_fallback_tracks_the_authored_camera() {
    let camera = WorldCamera::new(Vec3::Z, Vec3::ZERO, Vec3::Y, 1.0, 0.1, 100.0);
    let sunlight = glue_character_sunlight(camera);

    assert_eq!(sunlight.direction(), Vec3::Z);
    assert_eq!(sunlight.ambient(), Vec3::splat(0.60));
    assert_eq!(sunlight.diffuse(), Vec3::new(0.40, 0.40, 0.32));
}

//! External stock-compatibility tests for model and world light preparation.

use glam::Vec3;
use solarity_rendering::{
    M2DirectionalLight, M2LightOverride, M2LocalLightCount, M2LocalLightState, M2PointLight,
    WorldCamera, glue_character_sunlight, glue_ghost_sunlight, merge_wotlk_directional_lights,
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

/// Default ghosts clear scene points; explicit directional banks preserve them.
#[test]
fn glue_ghost_default_replaces_the_entire_light_accumulator() {
    let ambient = Vec3::new(70.0, 80.0, 90.0) / 255.0;
    let diffuse = Vec3::new(30.0, 40.0, 50.0) / 255.0;
    let ghost = glue_ghost_sunlight(ambient, diffuse);
    assert_eq!(ghost.direction(), Vec3::Z);
    assert_eq!(ghost.ambient(), ambient);
    assert_eq!(ghost.diffuse(), diffuse);
    let points = [
        M2PointLight::new(Vec3::X, Vec3::ZERO, Vec3::X),
        M2PointLight::new(Vec3::Y, Vec3::ZERO, Vec3::Y),
        M2PointLight::new(Vec3::Z, Vec3::ZERO, Vec3::Z),
    ];
    let default_ghost = M2LightOverride::All(ghost);
    assert_eq!(
        default_ghost.local_light_count(points.len()),
        M2LocalLightCount::One
    );
    let default_lights = default_ghost.local_lights(&points);
    assert_eq!(default_lights[0], ghost.local_light_state());
    assert_eq!(default_lights[1..], [M2LocalLightState::disabled(); 3]);
    let explicit_bank = M2LightOverride::Directional(ghost);
    assert_eq!(
        explicit_bank.local_light_count(points.len()),
        M2LocalLightCount::Four
    );
    let explicit_lights = explicit_bank.local_lights(&points);
    assert_eq!(explicit_lights[0], default_lights[0]);
    for (index, point) in points.iter().enumerate() {
        assert_eq!(explicit_lights[index + 1], point.local_light_state());
    }
}

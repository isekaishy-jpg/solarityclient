//! Collector registration rules and full preceding-volume exclusions.

use super::{ModelShadowKind as Kind, WorldShadowAdmission};
use glam::Vec3;
use solarity_rendering::{
    WorldCamera, WorldEnvironmentShadowFrame, WorldEnvironmentShadowState, WorldShadowProjection,
    WorldShadowQuality,
};
use solarity_systems::MovementCollisionBounds;
use std::error::Error;

#[test]
fn environment_shadow_admission_preserves_native_masks_radii_and_exclusions()
-> Result<(), Box<dyn Error>> {
    let camera = WorldCamera::orthographic(
        Vec3::Z * 100.,
        Vec3::ZERO,
        Vec3::Y,
        [-1000., 1000.],
        [-1000., 1000.],
        0.1,
        200.,
    )
    .frame(1.)?;
    for quality in [
        WorldShadowQuality::EnvironmentLow,
        WorldShadowQuality::Cascaded,
    ] {
        let mut state = WorldEnvironmentShadowState::new(quality);
        let updates = state.advance(Vec3::ZERO)?;
        let primary = WorldShadowProjection::primary(
            quality,
            Vec3::ZERO,
            camera.camera().position(),
            -Vec3::Z,
        )?
        .with_camera_culling(camera);
        let frame = WorldEnvironmentShadowFrame::new(
            &state,
            updates,
            camera.camera().position(),
            -Vec3::Z,
        )?;
        let admission = WorldShadowAdmission::new(primary, frame, camera, -Vec3::Z)?;
        let cascaded = quality == WorldShadowQuality::Cascaded;
        assert_eq!(admission.active_maps(), if cascaded { 15 } else { 8 });
        for (kind, radius, cached, cascade) in [
            (Kind::Unit, 0.249, 0, 0),
            (Kind::Unit, 0.25, 0, 1),
            (Kind::Unit, 2., 0, 3),
            (Kind::Unit, 10., 0, 7),
            (Kind::Unit, 10_001., 0, 0),
            (Kind::StaticGameObject, 1., 1, 9),
            (Kind::AnimatedGameObject, 26., 0, 15),
            (Kind::StaticScenery, 0.01, 7, 15),
            (Kind::AnimatedScenery, 25., 8, 15),
            (Kind::AnimatedScenery, 26., 0, 15),
            (Kind::StaticScenery, 10_001., 0, 7),
            (Kind::MovingWorldModelDoodad, 10_001., 8, 15),
        ] {
            assert_eq!(
                admission.model_maps(kind, radius, 15),
                if cascaded { cascade } else { cached }
            );
        }
        for (x, expected) in [(0., 8), (30., 1), (80., 2), (400., 4)] {
            let center = Vec3::X * x;
            let bounds = MovementCollisionBounds::new(center - Vec3::ONE, center + Vec3::ONE)?;
            assert_eq!(
                admission.admitted_maps(bounds),
                if cascaded || x == 0. { expected } else { 0 },
                "full nearer volume excludes the caster at x={x}"
            );
        }
    }
    Ok(())
}

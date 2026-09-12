use glam::{Mat4, Vec3};

use super::{
    M2UnsupportedParticle, PARTICLE_IGNORE_DISTANCE_LOD, classify_particle_support,
    particle_emission_density, particle_lod_origin, stock_glue_character_local_transform,
};

#[test]
fn unsupported_particle_paths_are_isolated_before_frame_advance() {
    assert_eq!(classify_particle_support(1, 0), None);
    assert_eq!(classify_particle_support(2, 0), None);
    assert_eq!(
        classify_particle_support(1, 0x0000_0800),
        Some(M2UnsupportedParticle::BehaviorFlags(0x0000_0800))
    );
    assert_eq!(
        classify_particle_support(3, 0),
        Some(M2UnsupportedParticle::EmitterType(3))
    );
}

#[test]
fn glue_character_local_transform_keeps_stock_unit_scale() {
    let transform = stock_glue_character_local_transform(0.75);
    assert_eq!(transform.transform_vector3(Vec3::X).length(), 1.0);
    assert_eq!(transform.transform_point3(Vec3::ZERO), Vec3::ZERO);
    assert_ne!(transform, Mat4::IDENTITY);
}

#[test]
fn particle_distance_lod_matches_stock_threshold_and_floor() {
    let camera = Vec3::ZERO;
    assert_eq!(particle_emission_density(0, Vec3::X * 50.0, camera), 1.0);
    assert_eq!(particle_emission_density(0, Vec3::X * 75.0, camera), 0.5);
    assert_eq!(particle_emission_density(0, Vec3::X * 100.0, camera), 0.25);
    assert_eq!(
        particle_emission_density(PARTICLE_IGNORE_DISTANCE_LOD, Vec3::X * 100.0, camera),
        1.0
    );
}

#[test]
fn particle_lod_uses_model_origin_before_emitter_offsets() {
    let model = Mat4::from_translation(Vec3::new(12.0, 34.0, 56.0));
    let emitter = model * Mat4::from_translation(Vec3::new(1_000.0, 2_000.0, 3_000.0));

    assert_eq!(particle_lod_origin(model), Vec3::new(12.0, 34.0, 56.0));
    assert_ne!(
        particle_lod_origin(model),
        emitter.transform_point3(Vec3::ZERO)
    );
}

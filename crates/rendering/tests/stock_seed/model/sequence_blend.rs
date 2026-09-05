//! Native previous-pose blending across skeleton, material, and effect consumers.

use std::error::Error;

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2BonePoseError, M2MaterialPose, M2MeshPlan, M2ParticlePose,
    M2RibbonPose,
};

use crate::support::{Fixture, FixtureFile};

use super::{
    append_render_track, m2_array_offset, render_f32_values, render_i16_values, render_m2_bytes,
    render_skin_bytes, set_render_header_array,
};

fn model() -> Result<DecodedM2Model, Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Blend", 1)?;
    let name = bytes.len() as u32;
    bytes.extend_from_slice(b"Blend\0");
    bytes[8..12].copy_from_slice(&6_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&name.to_le_bytes());
    let bones = m2_array_offset(&bytes, 0x2c)?;
    append_render_track(
        &mut bytes,
        bones + 36,
        &[0, 1_000],
        &render_i16_values(&[-32_768, -32_768, -32_768, -1, -32_768, -32_768, -1, -32_768]),
        8,
    )?;
    // A global child translation must use the shared clock, not either local time.
    let globals = bytes.len();
    bytes.extend_from_slice(&1_000_u32.to_le_bytes());
    set_render_header_array(&mut bytes, 0x14, 1, globals)?;
    append_render_track(
        &mut bytes,
        bones + 88 + 16,
        &[0, 1_000],
        &render_f32_values(&[0.0, 0.0, 0.0, 0.0, 8.0, 0.0]),
        12,
    )?;
    bytes[bones + 88 + 18..bones + 88 + 20].copy_from_slice(&0_u16.to_le_bytes());
    // Continuous sequence blending must not turn a step track into a fade.
    append_render_track(
        &mut bytes,
        bones + 176 + 16,
        &[0, 1_000],
        &render_f32_values(&[0.0, 0.0, 0.0, 0.0, 0.0, 10.0]),
        12,
    )?;
    bytes[bones + 176 + 16..bones + 176 + 18].copy_from_slice(&0_u16.to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Solarity\\Blend.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Solarity\\Blend00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    Ok(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Solarity\\Blend.m2")?,
    )?)
}

/// 0x00828680 slerps rotations; 0x0082B0A0 blends local vectors before hierarchy.
#[test]
fn sequence_blend_composes_local_pose_and_preserves_global_and_step_tracks()
-> Result<(), Box<dyn Error>> {
    let model = model()?;
    let clock = M2AnimationClock::new(0, 0.0, 500.0).with_secondary_sequence(0, 1_000.0, 0.25);
    let pose = M2BonePose::compose(model.animations(), clock)?;
    let expected =
        Mat4::from_translation(Vec3::X) * Mat4::from_rotation_z(core::f32::consts::FRAC_PI_4);
    assert!(pose.transforms()[0].abs_diff_eq(expected, 0.0002));
    assert!(
        pose.transforms()[1].abs_diff_eq(expected * Mat4::from_translation(Vec3::Y * 4.0), 0.0002)
    );
    assert!(pose.transforms()[2].abs_diff_eq(pose.transforms()[1], 0.0002));
    Ok(())
}

/// Material and effect parameters share the root blend, but visibility stays discrete.
#[test]
fn sequence_blend_reaches_material_ribbon_and_particle_parameters() -> Result<(), Box<dyn Error>> {
    let model = model()?;
    let animations = model.animations();
    let clock = M2AnimationClock::new(0, 0.0, 500.0).with_secondary_sequence(0, 1_000.0, 0.25);
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let material = M2MaterialPose::sample(&model, &plan, 0, clock)?;
    assert!(
        material
            .mesh_color()
            .truncate()
            .abs_diff_eq(Vec3::splat(1.25), 0.0001)
    );
    let alpha = 0.75 + 0.25 * (16_384.0 / 32_767.0);
    assert!((material.mesh_color().w - alpha * alpha).abs() < 0.0001);
    let ribbon = M2RibbonPose::sample(animations, &animations.ribbons()[0], clock)?;
    assert_eq!(ribbon.height_above(), 1.5);
    assert_eq!(ribbon.height_below(), 0.75);
    assert_eq!(ribbon.texture_slot(), 1);
    assert!(ribbon.visible());
    let particle = M2ParticlePose::sample(animations, &animations.particles()[0], clock)?;
    assert_eq!(particle.emission_speed(), 2.5);
    assert_eq!(particle.emission_rate(), 12.5);
    assert!(particle.enabled());
    let previous_only =
        M2AnimationClock::new(0, 1_000.0, 500.0).with_secondary_sequence(0, 0.0, 1.0);
    assert!(
        !M2ParticlePose::sample(animations, &animations.particles()[0], previous_only)?.enabled()
    );
    Ok(())
}

#[test]
fn sequence_blend_rejects_invalid_secondary_clocks() -> Result<(), Box<dyn Error>> {
    let model = model()?;
    let clock = M2AnimationClock::new(0, 0.0, 0.0);
    for weight in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        assert_eq!(
            M2BonePose::compose(
                model.animations(),
                clock.with_secondary_sequence(0, 0.0, weight)
            ),
            Err(M2BonePoseError::InvalidBlendWeight)
        );
    }
    assert_eq!(
        M2BonePose::compose(
            model.animations(),
            clock.with_secondary_sequence(0, f32::NAN, 0.5)
        ),
        Err(M2BonePoseError::NonFiniteTime)
    );
    assert!(matches!(
        M2BonePose::compose(
            model.animations(),
            clock.with_secondary_sequence(1, 0.0, 0.5)
        ),
        Err(M2BonePoseError::SequenceIndex {
            requested: 1,
            available: 1
        })
    ));
    Ok(())
}

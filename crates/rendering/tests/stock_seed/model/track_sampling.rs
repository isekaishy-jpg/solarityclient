//! Property-typed track cases checked against executed build-12340 instructions.

use std::error::Error;

use glam::{Mat4, Quat, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    M2AnimationClock, M2BonePose, M2MaterialPose, M2MeshPlan, M2ParticlePose,
    sample_m2_camera_frame,
};

use super::{
    append_render_camera, append_render_particle, append_render_track, m2_array_offset,
    render_f32_values, render_i16_values, render_m2_bytes, render_skin_bytes,
    set_render_header_array,
};
use crate::support::{Fixture, FixtureFile};

/// Original 0x00828680 -> 0x00982460 -> 0x004C1C40 output for the blend fixture.
/// The non-unit result is intentional; replacing it with an ideal 45-degree
/// matrix hides stock's polynomial normalization.
pub(super) const BONE_BLEND_MATRIX: [f32; 16] = [
    0.7073086,
    0.7066043,
    -0.000021562864,
    0.0,
    -0.7066043,
    0.7073086,
    0.000052058487,
    0.0,
    0.000052058487,
    -0.000021562864,
    1.0,
    0.0,
    0.0,
    0.0,
    0.0,
    1.0,
];

/// Loads each byte fixture through the public archive/model boundary.
fn load(mut bytes: Vec<u8>) -> Result<DecodedM2Model, Box<dyn Error>> {
    // Repair the fixture writer's unresolved name offset after header patching.
    let name_offset = u32::try_from(bytes.len())?;
    bytes.extend_from_slice(b"Track\0");
    bytes[8..12].copy_from_slice(&6_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&name_offset.to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Solarity\\Track.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Solarity\\Track00.skin",
            bytes: &skin,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut assets = AssetStore::mount(catalog)?;
    Ok(DecodedM2Model::load(
        &mut assets,
        &AssetPath::new("Solarity\\Track.m2")?,
    )?)
}

/// The native fixture gives phases 166 and 218; converting the elapsed tick
/// to float before modulo would select the neighboring emission keys.
#[test]
fn model_global_particle_keys_preserve_integer_phase_and_ignore_primary_blends()
-> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Track", 1)?;
    append_render_particle(&mut bytes)?;
    let particle = m2_array_offset(&bytes, 0x128)?;
    let track = particle + 0x0b0;
    append_render_track(
        &mut bytes,
        track,
        &[0, 166, 167, 218, 219],
        &render_f32_values(&[1.0, 2.0, 3.0, 4.0, 5.0]),
        4,
    )?;
    bytes[track + 2..track + 4].copy_from_slice(&0_i16.to_le_bytes());
    let duration = bytes.len();
    bytes.extend_from_slice(&667_u32.to_le_bytes());
    set_render_header_array(&mut bytes, 0x14, 1, duration)?;
    let model = load(bytes)?;
    for (elapsed, expected) in [(0, 1.0), (16_777_217, 2.0), (u32::MAX, 4.0)] {
        let clock = M2AnimationClock::new_with_global_tick(0, 500.0, elapsed)
            .with_secondary_sequence(0, 250.0, 1.0);
        let pose = M2ParticlePose::sample(
            model.animations(),
            &model.animations().particles()[0],
            clock,
        )?;
        assert_eq!(pose.emission_rate(), expected, "elapsed {elapsed}");
    }
    Ok(())
}

/// Native 0x0082B460/0x0082B8A0 output at t=1/4, with deliberately distinct
/// value, incoming, and outgoing fields. The harness supplies only the key
/// interval; stock instructions perform every address calculation and sample.
#[test]
fn camera_keys_keep_triplets_for_every_interpolation_selector() -> Result<(), Box<dyn Error>> {
    let vector_samples = [
        Vec3::new(0.0, 1.0, 2.0),
        Vec3::new(0.75, 1.75, 2.75),
        Vec3::new(12.5625, 13.0, 13.4375),
        Vec3::new(2.484375, 3.484375, 4.484375),
    ];
    let roll_samples = [0.0, 0.125, 18.148_438, 3.03125];
    for selector in 0..4_u16 {
        let mut bytes = render_m2_bytes("Track", 1)?;
        append_render_camera(&mut bytes)?;
        let camera = m2_array_offset(&bytes, 0x110)?;
        for relative in [16, 48] {
            append_render_track(
                &mut bytes,
                camera + relative,
                &[0, 1_000],
                &render_f32_values(&[
                    0.0, 1.0, 2.0, 21.0, 21.0, 21.0, 22.0, 22.0, 22.0, 3.0, 4.0, 5.0, 23.0, 23.0,
                    23.0, 24.0, 24.0, 24.0,
                ]),
                36,
            )?;
            bytes[camera + relative..camera + relative + 2]
                .copy_from_slice(&selector.to_le_bytes());
        }
        append_render_track(
            &mut bytes,
            camera + 80,
            &[0, 1_000],
            &render_f32_values(&[0.0, 31.0, 32.0, 0.5, 33.0, 34.0]),
            12,
        )?;
        bytes[camera + 80..camera + 82].copy_from_slice(&selector.to_le_bytes());
        let model = load(bytes)?;
        for (time_ms, offset, roll) in [
            (
                250.0,
                vector_samples[usize::from(selector)],
                roll_samples[usize::from(selector)],
            ),
            (1_000.0, Vec3::new(3.0, 4.0, 5.0), 0.5),
        ] {
            let frame = sample_m2_camera_frame(
                model.animations(),
                0,
                M2AnimationClock::new(0, time_ms, 0.0),
                16.0 / 9.0,
                Mat4::IDENTITY,
            )?;
            assert!(
                frame
                    .camera()
                    .position()
                    .abs_diff_eq(Vec3::new(11.0, 0.0, 2.0) + offset, 0.000001)
            );
            assert!(
                frame
                    .camera()
                    .target()
                    .abs_diff_eq(Vec3::new(-10.0, 0.0, 2.0) + offset, 0.000001)
            );
            let up = Quat::from_axis_angle(Vec3::NEG_X, roll) * Vec3::Z;
            assert!(frame.up().abs_diff_eq(up, 0.000002));
        }
    }
    Ok(())
}

/// Stock 0x0082B8A0 returns pi halfway between authored 2*pi and zero.
#[test]
fn camera_linear_roll_preserves_authored_full_turns() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Track", 1)?;
    append_render_camera(&mut bytes)?;
    let camera = m2_array_offset(&bytes, 0x110)?;
    append_render_track(
        &mut bytes,
        camera + 80,
        &[0, 1_000],
        &render_f32_values(&[core::f32::consts::TAU, 0.0, 0.0, 0.0, 0.0, 0.0]),
        12,
    )?;
    let model = load(bytes)?;
    let frame = sample_m2_camera_frame(
        model.animations(),
        0,
        M2AnimationClock::new(0, 500.0, 0.0),
        16.0 / 9.0,
        Mat4::IDENTITY,
    )?;
    assert!(frame.up().abs_diff_eq(Vec3::NEG_Z, 0.000001));
    Ok(())
}

/// Stock 0x0082B0A0 retains a 12-byte stride and linear behavior for selectors 1..3.
#[test]
fn ordinary_vector_keys_do_not_acquire_spline_controls() -> Result<(), Box<dyn Error>> {
    for selector in 0..4_u16 {
        let mut bytes = render_m2_bytes("Track", 1)?;
        let bone = m2_array_offset(&bytes, 0x2c)?;
        append_render_track(
            &mut bytes,
            bone + 16,
            &[0, 1_000],
            &render_f32_values(&[0.0, 0.0, 0.0, 2.0, 4.0, 6.0]),
            12,
        )?;
        bytes[bone + 16..bone + 18].copy_from_slice(&selector.to_le_bytes());
        let model = load(bytes)?;
        let pose = M2BonePose::compose(model.animations(), M2AnimationClock::new(0, 250.0, 0.0))?;
        let expected = if selector == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(0.5, 1.0, 1.5)
        };
        assert_eq!(pose.transforms()[0], Mat4::from_translation(expected));
    }
    Ok(())
}

/// Original 0x0082AD50 -> 0x00982630 -> 0x004C1C40 results. The endpoints
/// have a negative dot product, exposing an incorrect within-track hemisphere
/// flip; the step endpoint also exposes eager exact normalization.
#[test]
fn texture_rotation_keys_use_float_storage_and_stock_normalization() -> Result<(), Box<dyn Error>> {
    let step_matrix = Mat4::from_cols_array(&[
        0.59999996,
        0.28,
        0.44,
        0.0,
        -0.52000004,
        0.49999997,
        0.14,
        0.0,
        0.040000007,
        -0.46000004,
        0.74,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]);
    let linear_matrix = Mat4::from_cols_array(&[
        0.12673315,
        0.59382147,
        0.6636828,
        0.0,
        -0.78943324,
        -0.18764296,
        0.28643155,
        0.0,
        0.41218197,
        -0.5938215,
        0.6297348,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]);
    for selector in 0..4_u16 {
        let mut bytes = render_m2_bytes("Track", 1)?;
        let transform = m2_array_offset(&bytes, 0x60)?;
        append_render_track(
            &mut bytes,
            transform + 20,
            &[0, 1_000],
            &render_f32_values(&[0.3, -0.2, 0.4, 0.5, -0.2, 0.4, -0.1, -0.6]),
            16,
        )?;
        bytes[transform + 20..transform + 22].copy_from_slice(&selector.to_le_bytes());
        let model = load(bytes)?;
        let plan = M2MeshPlan::prepare(&model, 0)?;
        let material =
            M2MaterialPose::sample(&model, &plan, 0, M2AnimationClock::new(0, 250.0, 0.0))?;
        let pivot = Vec3::new(0.5, 0.5, 0.0);
        let expected = Mat4::from_translation(pivot)
            * if selector == 0 {
                step_matrix
            } else {
                linear_matrix
            }
            * Mat4::from_translation(-pivot)
            * Mat4::from_translation(Vec3::X * 0.125);
        assert!(material.texture_transforms()[0].abs_diff_eq(expected, 0.000001));
    }
    Ok(())
}

/// Original 0x00828680 reads unsigned compressed components and normalizes
/// only interpolated keys. All nonzero selectors use the same ordinary stride.
#[test]
fn bone_rotation_keys_keep_stock_compression_and_matrix_components() -> Result<(), Box<dyn Error>> {
    let step_matrix = Mat4::from_cols_array(&[
        1.0,
        0.000030518044,
        -0.000030517112,
        0.0,
        -0.000030517112,
        1.0,
        0.000030518044,
        0.0,
        0.000030518044,
        -0.000030517112,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]);
    let linear_matrix = Mat4::from_cols_array(&[
        -0.000_293_667_5,
        1.0002936,
        0.0,
        0.0,
        -1.0002936,
        -0.0002936134,
        0.000061052146,
        0.0,
        0.000061052146,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]);
    for selector in 0..4_u16 {
        let mut bytes = render_m2_bytes("Track", 1)?;
        let bone = m2_array_offset(&bytes, 0x2c)?;
        append_render_track(
            &mut bytes,
            bone + 36,
            &[0, 1_000],
            &render_i16_values(&[-32_768, -32_768, -32_768, -1, -32_768, -32_768, -1, -32_768]),
            8,
        )?;
        bytes[bone + 36..bone + 38].copy_from_slice(&selector.to_le_bytes());
        let model = load(bytes)?;
        let pose = M2BonePose::compose(model.animations(), M2AnimationClock::new(0, 500.0, 0.0))?;
        let expected = Mat4::from_translation(Vec3::X * 2.0)
            * if selector == 0 {
                step_matrix
            } else {
                linear_matrix
            };
        assert!(pose.transforms()[0].abs_diff_eq(expected, 0.000001));
    }
    Ok(())
}

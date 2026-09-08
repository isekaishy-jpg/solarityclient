//! Original per-bone clocks constrain wound fades and descendant coverage.

use super::*;
use solarity_asset::M2ModelAnimationMode;
use solarity_rendering::{
    M2BonePoseOverrides, M2ModelSequenceBlend, M2ModelSequenceTimer, M2SequenceStartPhase,
};

fn model() -> Result<DecodedM2Model, Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Wound", 1)?;
    let name = bytes.len();
    bytes.extend_from_slice(b"Wound\0");
    set_render_header_array(&mut bytes, 8, 6, name)?;
    let old_sequence = m2_array_offset(&bytes, 0x1c)?;
    let template = bytes[old_sequence..old_sequence + 64].to_vec();
    let sequences = bytes.len();
    for (id, duration) in [(0_u16, 2000_u32), (8, 800)] {
        let mut sequence = template.clone();
        sequence[..2].copy_from_slice(&id.to_le_bytes());
        sequence[4..8].copy_from_slice(&duration.to_le_bytes());
        sequence[12..16].copy_from_slice(&0x20_u32.to_le_bytes());
        sequence[20..24].copy_from_slice(&1_u32.to_le_bytes());
        sequence[24..28].copy_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&sequence);
    }
    set_render_header_array(&mut bytes, 0x1c, 2, sequences)?;
    let old_bones = m2_array_offset(&bytes, 0x2c)?;
    let template = bytes[old_bones..old_bones + 88].to_vec();
    let bones = bytes.len();
    for parent in [-1_i16, 0, 0, 2] {
        let mut bone = template.clone();
        bone[..4].copy_from_slice(&(-1_i32).to_le_bytes());
        bone[8..10].copy_from_slice(&parent.to_le_bytes());
        bytes.extend_from_slice(&bone);
    }
    set_render_header_array(&mut bytes, 0x2c, 4, bones)?;
    for index in 0..4 {
        append_two_sequence_vec3_track(
            &mut bytes,
            bones + index * 88 + 16,
            [Vec3::X * (index + 1) as f32, Vec3::Y * (index + 1) as f32],
        )?;
    }
    let lookup = bytes.len();
    for index in [-1_i16, -1, -1, -1, 2, -1, 3] {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    set_render_header_array(&mut bytes, 0x34, 7, lookup)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Solarity\\Wound.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Solarity\\Wound00.skin",
            bytes: &skin,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Solarity\\Wound.m2")?,
    )?)
}

#[test]
fn wound_secondary_bone_clocks_and_poses_match_native() -> Result<(), Box<dyn Error>> {
    let model = model()?;
    let rows = include_str!("../../fixtures/unit_wound_native.bones.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 112);
    for chain in rows.as_chunks::<4>().0 {
        let wound_bone = chain[0][0].parse::<usize>()?;
        let now = chain[0][1].parse::<u32>()?;
        let elapsed = chain[0][2].parse::<u32>()?;
        let time = now.wrapping_add(elapsed);
        let primary = M2ModelSequenceTimer::new(
            &model.animations().sequences()[0],
            M2ModelAnimationMode::Forward,
            now.wrapping_sub(100),
            0,
            12345,
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        let secondary = M2ModelSequenceTimer::new(
            &model.animations().sequences()[1],
            M2ModelAnimationMode::Forward,
            now,
            0,
            12345,
            M2SequenceStartPhase::BeforeSceneUpdate,
        );
        let blend = M2ModelSequenceBlend::wound(1, secondary, now, 800);
        let clock =
            M2AnimationClock::new_with_global_tick(0, primary.animation_time_ms(time) as f32, time);
        let mixed = blend.apply_to_clock(clock, time);
        let sequences = [(4, mixed)];
        let mut pose = M2BonePose::default();
        pose.recompose_with_overrides(
            model.animations(),
            if wound_bone == 0 { mixed } else { clock },
            Mat4::IDENTITY,
            M2BonePoseOverrides {
                bone_sequences: if wound_bone == 0 { &[] } else { &sequences },
                ..Default::default()
            },
        )?;
        let mut expected = [Vec3::ZERO; 4];
        for (bone, row) in chain.iter().enumerate() {
            let weight = f32::from_bits(u32::from_str_radix(row[8], 16)?);
            assert_eq!(primary.animation_time_ms(time), row[5].parse::<u32>()?);
            if elapsed < 800 && (wound_bone == 0 || bone >= 2) {
                assert_eq!(
                    secondary.secondary_animation_time_ms(time),
                    row[7].parse::<u32>()?
                );
                assert!((blend.weight(time) - weight).abs() <= 1e-7);
            }
            let local = Vec3::new(1.0 - weight, weight, 0.0) * (bone + 1) as f32;
            expected[bone] = local
                + match bone {
                    1 | 2 => expected[0],
                    3 => expected[2],
                    _ => Vec3::ZERO,
                };
            assert!(
                pose.transforms()[bone]
                    .transform_point3(Vec3::ZERO)
                    .abs_diff_eq(expected[bone], 1e-6),
                "native row {row:?}"
            );
        }
    }
    Ok(())
}

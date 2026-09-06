//! Original executable matrices verify noncommuting overrides through a hierarchy.

use super::*;
use solarity_rendering::M2BonePoseOverrides;

#[test]
fn unit_bone_overrides_match_original_composition() -> Result<(), Box<dyn Error>> {
    let rows = include_str!("../../fixtures/unit-bone-pose-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 15);
    for chain in rows.as_chunks::<3>().0 {
        let mut bytes = render_m2_bytes("BodyPose", 1)?;
        let bone_offset = m2_array_offset(&bytes, 0x2c)?;
        let mut overrides = Vec::new();
        let mut expected = Vec::new();
        for (index, row) in chain.iter().enumerate() {
            let values = row[6..]
                .iter()
                .map(|value| u32::from_str_radix(value, 16).map(f32::from_bits))
                .collect::<Result<Vec<_>, _>>()?;
            let bone = bone_offset + index * 88;
            bytes[bone + 76..bone + 88].copy_from_slice(&render_f32_values(&values[..3]));
            append_render_track(
                &mut bytes,
                bone + 16,
                &[0],
                &render_f32_values(&values[3..6]),
                12,
            )?;
            append_render_track(
                &mut bytes,
                bone + 56,
                &[0],
                &render_f32_values(&values[6..9]),
                12,
            )?;
            let quaternion = row[2..6]
                .iter()
                .map(|value| value.parse::<u16>())
                .collect::<Result<Vec<_>, _>>()?;
            let quaternion = quaternion
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect::<Vec<_>>();
            append_render_track(&mut bytes, bone + 36, &[0], &quaternion, 8)?;
            // Oracle starts with sampled components; step keys retain them.
            bytes[bone + 36..bone + 38].copy_from_slice(&0_u16.to_le_bytes());
            if index > 0 {
                overrides.push((
                    if index == 1 { 4 } else { 6 },
                    Mat4::from_cols_array(values[10..26].try_into()?),
                ));
            }
            expected.push(Mat4::from_cols_array(values[26..42].try_into()?));
        }
        // The native setter resolves the semantic lookup, independently of the
        // bone record's key ID. Root -> spine -> head tests inherited overrides.
        let lookup = bytes.len();
        for index in [-1_i16, -1, -1, -1, 1, -1, 2] {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        set_render_header_array(&mut bytes, 0x34, 7, lookup)?;
        let skin = render_skin_bytes()?;
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "Creature\\Solarity\\BodyPose.m2",
                bytes: &bytes,
            },
            FixtureFile {
                path: "Creature\\Solarity\\BodyPose00.skin",
                bytes: &skin,
            },
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut store = AssetStore::mount(catalog)?;
        let model = DecodedM2Model::load(
            &mut store,
            &AssetPath::new("Creature\\Solarity\\BodyPose.m2")?,
        )?;
        let mut pose = M2BonePose::default();
        let clock = M2AnimationClock::new(0, 0., 0.);
        pose.recompose_with_overrides(
            model.animations(),
            clock,
            Mat4::IDENTITY,
            M2BonePoseOverrides {
                bone_transforms: &overrides,
                ..Default::default()
            },
        )?;
        for (index, (actual, expected)) in pose.transforms().iter().zip(&expected).enumerate() {
            for (component, (actual, expected)) in actual
                .to_cols_array()
                .into_iter()
                .zip(expected.to_cols_array())
                .enumerate()
            {
                assert!(
                    (actual - expected).abs() <= 3e-6 * expected.abs().max(1.),
                    "case {} bone {index} component {component}: {actual} != native {expected}",
                    chain[0][0]
                );
            }
        }
        let baseline = M2BonePose::compose(model.animations(), clock)?;
        pose.recompose_with_overrides(
            model.animations(),
            clock,
            Mat4::IDENTITY,
            M2BonePoseOverrides {
                bone_transforms: &[(u16::MAX, Mat4::from_rotation_x(1.))],
                ..Default::default()
            },
        )?;
        assert_eq!(
            pose, baseline,
            "cleared and missing semantic overrides leave no retained transform"
        );
    }
    Ok(())
}

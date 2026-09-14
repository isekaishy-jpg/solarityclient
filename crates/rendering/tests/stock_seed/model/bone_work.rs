//! Empty local tracks still inherit animated parents and instance overrides.

use super::*;
use solarity_rendering::{M2BonePoseOverrides, M2BoneSamples, M2BoneTransforms};

#[test]
fn empty_bone_locals_preserve_out_of_order_parents_and_changing_overrides()
-> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("BoneWork.blp", 1)?;
    let offset = m2_array_offset(&bytes, 0x2c)?;
    // Reverse the fixture hierarchy: child 0 -> parent 1 -> animated root 2.
    let root = bytes[offset..offset + 88].to_vec();
    let child = bytes[offset + 176..offset + 264].to_vec();
    bytes[offset..offset + 88].copy_from_slice(&child);
    bytes[offset + 176..offset + 264].copy_from_slice(&root);
    for (index, parent) in [1_i16, 2, -1].into_iter().enumerate() {
        bytes[offset + index * 88 + 8..offset + index * 88 + 10]
            .copy_from_slice(&parent.to_le_bytes());
    }
    let lookup = bytes.len();
    bytes.extend_from_slice(&1_i16.to_le_bytes());
    set_render_header_array(&mut bytes, 0x34, 1, lookup)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature\\Solarity\\BoneWork.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature\\Solarity\\BoneWork00.skin",
            bytes: &skin,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Creature\\Solarity\\BoneWork.m2")?,
    )?;
    assert_eq!(model.animations().bone_parent_order(), &[2, 1, 0]);
    assert!(model.animations().has_identity_bone_local(0));
    assert!(model.animations().has_identity_bone_local(1));
    assert!(!model.animations().has_identity_bone_local(2));
    let mut pose = M2BonePose::default();
    let mut samples = M2BoneSamples::default();
    for (time, extra) in [(500., 0.), (500., 3.), (750., 0.)] {
        let overrides = [(0, Mat4::from_translation(Vec3::Y * extra))];
        pose.recompose_with_overrides(
            model.animations(),
            M2AnimationClock::new(0, time, time),
            Mat4::IDENTITY,
            M2BonePoseOverrides {
                bone_transforms: if extra == 0. { &[] } else { &overrides },
                ..Default::default()
            },
        )?;
        for index in 0..3 {
            assert_eq!(
                pose.transforms()[index],
                Mat4::from_translation(Vec3::new(
                    time / 250.,
                    if index < 2 { extra } else { 0. },
                    0.
                ))
            );
        }
        // CPU demands can shrink, disappear, and grow while time/overrides
        // change. Unrequested storage must never leak a previous frame's pose.
        for requested in [&[0][..], &[1][..], &[], &[2][..]] {
            samples.recompose(
                model.animations(),
                M2AnimationClock::new(0, time, time),
                Mat4::IDENTITY,
                M2BonePoseOverrides {
                    bone_transforms: if extra == 0. { &[] } else { &overrides },
                    ..Default::default()
                },
                requested,
            )?;
            assert_eq!(samples.bone_count(), 3);
            for index in 0..3 {
                let demanded = requested.first().is_some_and(|first| index >= *first);
                assert_eq!(
                    samples.bone_transform(index),
                    demanded.then_some(pose.transforms()[index]),
                );
            }
        }
    }
    assert!(
        samples
            .recompose(
                model.animations(),
                M2AnimationClock::new(0, 0., 0.),
                Mat4::IDENTITY,
                M2BonePoseOverrides::default(),
                &[3],
            )
            .is_err()
    );
    assert_eq!(
        samples.bone_transform(2),
        None,
        "failed sampling invalidates old results"
    );
    Ok(())
}

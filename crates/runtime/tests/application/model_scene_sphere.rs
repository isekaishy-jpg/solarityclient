//! Decoded header bounds must retain native scene-sphere threshold and transforms.

use std::{collections::HashMap, error::Error};

use glam::Mat4;
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};

use super::UnitSceneRegistration;
use crate::test_support::{ClientFixture, game_object_models};

#[test]
fn registered_model_spheres_match_original_header_and_placement_math() -> Result<(), Box<dyn Error>>
{
    let cases = include_str!("../fixtures/model_scene_sphere_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|word| u32::from_str_radix(word, 16))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut identities = HashMap::new();
    let mut files = Vec::new();
    for case in &cases {
        if identities.contains_key(&case[..7]) {
            continue;
        }
        let index = identities.len();
        let mut bytes = game_object_models::model()?;
        for (field, word) in case[..7].iter().enumerate() {
            bytes[0xa0 + field * 4..0xa4 + field * 4].copy_from_slice(&word.to_le_bytes());
        }
        files.push((format!("Sphere{index}.m2"), bytes));
        files.push((format!("Sphere{index}00.skin"), game_object_models::skin()?));
        identities.insert(case[..7].to_vec(), index);
    }
    let entries = files
        .iter()
        .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
        .collect::<Vec<_>>();
    let fixture = ClientFixture::with_common_files(&entries)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let models = (0..identities.len())
        .map(|index| {
            DecodedM2Model::load(&mut store, &AssetPath::new(format!("Sphere{index}.m2"))?)
        })
        .collect::<Result<Vec<_>, _>>()?;
    for (index, case) in cases.iter().enumerate() {
        let model = &models[*identities.get(&case[..7]).ok_or("model")?];
        let matrix = Mat4::from_cols_array(&std::array::from_fn(|column| {
            f32::from_bits(case[7 + column])
        }));
        let (center, radius) = UnitSceneRegistration::model_sphere(model, matrix);
        assert_eq!(
            [
                center.x.to_bits(),
                center.y.to_bits(),
                center.z.to_bits(),
                radius.to_bits()
            ],
            case[23..27],
            "native sphere {index}",
        );
    }
    assert_eq!(cases.len(), 120);
    Ok(())
}

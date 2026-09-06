//! Complete synthetic character/creature catalogs for live unit residency tests.

use super::{ClientFixture, game_object_models};
use std::error::Error;

pub fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    let ids = [0, 91, 96, 97, 98, 99, 100, 101];
    let mut model = game_object_models::model_with_animations(&ids)?;
    let sequences = u32::from_le_bytes(model[0x20..0x24].try_into()?) as usize;
    for (index, id) in ids.iter().enumerate() {
        if matches!(id, 96 | 98 | 99 | 101) {
            model[sequences + index * 64 + 12..sequences + index * 64 + 16]
                .copy_from_slice(&0x21_u32.to_le_bytes());
        }
    }
    let animations: Vec<_> = ids
        .iter()
        .flat_map(|id| [u32::from(*id), 0, 0, 0, 0, 0, u32::from(*id), 0])
        .collect();
    let mut display = [0; 16];
    display[0] = 100;
    display[1] = 7;
    display[4] = 1.0_f32.to_bits();
    display[5] = u32::MAX;
    let mut model_data = [0; 28];
    model_data[0] = 7;
    model_data[2] = 1;
    model_data[4] = 1.0_f32.to_bits();
    let sections: Vec<_> = (0..5)
        .flat_map(|section| {
            [
                10 + section,
                1,
                0,
                section,
                u32::from(section == 0),
                0,
                0,
                if section == 0 { 9 } else { 1 },
                0,
                0,
            ]
        })
        .collect();
    let mut race = [0; 69];
    race[0] = 1;
    race[4] = 100;
    race[5] = 101;
    race[6] = 1;
    race[11] = 4;
    race[14] = 4;
    ClientFixture::with_common_files(&[
        ("Character\\Human\\Male\\HumanMale.m2", &model),
        (
            "Character\\Human\\Male\\HumanMale00.skin",
            &game_object_models::skin()?,
        ),
        ("Character\\Human\\Male\\Skin.blp", &skin_texture()),
        (
            "DBFilesClient\\CreatureDisplayInfo.dbc",
            &dbc(16, &display, b"\0"),
        ),
        (
            "DBFilesClient\\CreatureModelData.dbc",
            &dbc(28, &model_data, b"\0Character\\Human\\Male\\HumanMale.m2\0"),
        ),
        (
            "DBFilesClient\\AnimationData.dbc",
            &dbc(8, &animations, b"\0"),
        ),
        (
            "DBFilesClient\\CharSections.dbc",
            &dbc(10, &sections, b"\0Character\\Human\\Male\\Skin.blp\0"),
        ),
        ("DBFilesClient\\CharHairGeosets.dbc", &dbc(6, &[], b"\0")),
        (
            "DBFilesClient\\CharacterFacialHairStyles.dbc",
            &dbc(8, &[], b"\0"),
        ),
        (
            "DBFilesClient\\ChrRaces.dbc",
            &dbc(69, &race, b"\0Hu\0Human\0"),
        ),
    ])
}

fn dbc(width: usize, fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        (fields.len() / width) as u32,
        width as u32,
        (width * 4) as u32,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn skin_texture() -> Vec<u8> {
    let offset = 148 + 256 * 4;
    let mut bytes = b"BLP2".to_vec();
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, 0]);
    bytes.extend_from_slice(&256_u32.to_le_bytes());
    bytes.extend_from_slice(&256_u32.to_le_bytes());
    bytes.extend_from_slice(&(offset as u32).to_le_bytes());
    bytes.resize(84, 0);
    bytes.extend_from_slice(&(256_u32 * 256 * 4).to_le_bytes());
    bytes.resize(offset, 0);
    bytes.resize(offset + 256 * 256 * 4, 255);
    bytes
}

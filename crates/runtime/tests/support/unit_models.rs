//! Complete synthetic character/creature catalogs for live unit residency tests.

use super::{ClientFixture, game_object_models};
use std::error::Error;

pub fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, false)
}

pub fn fixture_with_effects() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(true, false)
}

pub fn fixture_with_equipment() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, true)
}

fn build_fixture(effects: bool, equipment: bool) -> Result<ClientFixture, Box<dyn Error>> {
    let ids = [0, 91, 96, 97, 98, 99, 100, 101];
    let mut model = game_object_models::model_with_animations(&ids)?;
    let sequences = u32::from_le_bytes(model[0x20..0x24].try_into()?) as usize;
    for (index, id) in ids.iter().enumerate() {
        if matches!(id, 96 | 98 | 99 | 101) {
            model[sequences + index * 64 + 12..sequences + index * 64 + 16]
                .copy_from_slice(&0x21_u32.to_le_bytes());
        }
    }
    if effects {
        append_effects(&mut model, ids.len());
    }
    if equipment {
        append_attachments(&mut model, &[1, 5, 6, 11, 26], ids.len());
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
    let mut displays = display.to_vec();
    display[0] = 101;
    displays.extend_from_slice(&display);
    display[0] = 102;
    display[1] = 8;
    displays.extend_from_slice(&display);
    let mut model_data = [0; 28];
    model_data[0] = 7;
    model_data[2] = 1;
    model_data[4] = 1.0_f32.to_bits();
    let mut models = model_data.to_vec();
    let mut model_paths = b"\0Character\\Human\\Male\\HumanMale.m2\0".to_vec();
    model_data[0] = 8;
    model_data[2] = model_paths.len() as u32;
    models.extend_from_slice(&model_data);
    model_paths.extend_from_slice(b"Creature\\Alternate.m2\0");
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
    let mut files: Vec<(String, Vec<u8>)> = [
        ("Character\\Human\\Male\\HumanMale.m2", &model),
        (
            "Character\\Human\\Male\\HumanMale00.skin",
            &game_object_models::skin()?,
        ),
        ("Character\\Human\\Male\\Skin.blp", &skin_texture()),
        ("Creature\\Alternate.m2", &model),
        ("Creature\\Alternate00.skin", &game_object_models::skin()?),
        (
            "DBFilesClient\\CreatureDisplayInfo.dbc",
            &dbc(16, &displays, b"\0"),
        ),
        (
            "DBFilesClient\\CreatureModelData.dbc",
            &dbc(28, &models, &model_paths),
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
    ]
    .into_iter()
    .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
    .collect();
    if equipment {
        append_equipment_files(&mut files, &ids)?;
    }
    ClientFixture::with_common_files(
        &files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect::<Vec<_>>(),
    )
}

fn append_equipment_files(
    files: &mut Vec<(String, Vec<u8>)>,
    ids: &[u16],
) -> Result<(), Box<dyn Error>> {
    // Native default selection must skip zero-weight variation zero. Keep
    // every component's tracks complete for both authored Stand variations.
    let ids = [vec![0], ids.to_vec()].concat();
    let mut model = game_object_models::model_with_animations(&ids)?;
    let sequences = u32::from_le_bytes(model[0x20..0x24].try_into()?) as usize;
    model[sequences + 16..sequences + 20].copy_from_slice(&0_u32.to_le_bytes());
    model[sequences + 60..sequences + 62].copy_from_slice(&1_u16.to_le_bytes());
    model[sequences + 66..sequences + 68].copy_from_slice(&1_u16.to_le_bytes());
    append_effects(&mut model, ids.len());
    append_attachments(&mut model, &[0], ids.len());
    for path in [
        "Item\\ObjectComponents\\Head\\Helm_HuM",
        "Item\\ObjectComponents\\Shoulder\\Right",
        "Item\\ObjectComponents\\Shoulder\\Right2",
        "Item\\ObjectComponents\\Shoulder\\Left",
        "Item\\ObjectComponents\\Weapon\\Weapon",
        "Spells\\Effect1",
        "Spells\\Effect2",
    ] {
        files.push((format!("{path}.m2"), model.clone()));
        files.push((format!("{path}00.skin"), game_object_models::skin()?));
    }
    let mut strings = vec![0];
    let mut displays = Vec::new();
    for (id, right, left, visual, flags) in [
        (500, "Helm", "", 700, 0x1c0),
        (501, "Helm", "", 701, 0),
        (600, "Right.m2", "Left.m2", 700, 0x1c0),
        (601, "Right.m2", "Left.m2", 701, 0),
        (602, "Right2.m2", "Left.m2", 700, 0),
        (603, "Right2.m2", "", 700, 0),
        (800, "Weapon.m2", "", 0, 0x1c0),
    ] {
        let mut row = [0; 25];
        row[0] = id;
        row[1] = strings.len() as u32;
        strings.extend_from_slice(right.as_bytes());
        strings.push(0);
        row[2] = strings.len() as u32;
        strings.extend_from_slice(left.as_bytes());
        strings.push(0);
        row[10] = flags;
        row[23] = visual;
        row[24] = u32::MAX;
        displays.extend_from_slice(&row);
    }
    let items: Vec<_> = [
        (1000, 500, 1),
        (1001, 501, 1),
        (2000, 600, 3),
        (2001, 601, 3),
        (2002, 602, 3),
        (2003, 603, 3),
        (3000, 800, 13),
        (3001, 800, 13),
    ]
    .into_iter()
    .flat_map(|(id, display, inventory)| [id, 2, 0, u32::MAX, 0, display, inventory, 1])
    .collect();
    let enchants: Vec<_> = [(900, 700), (901, 701), (902, 700)]
        .into_iter()
        .flat_map(|(id, visual)| {
            let mut row = [0; 38];
            row[0] = id;
            row[31] = visual;
            row
        })
        .collect();
    files.extend([
        ("DBFilesClient\\Item.dbc".into(), dbc(8, &items, b"\0")),
        (
            "DBFilesClient\\ItemDisplayInfo.dbc".into(),
            dbc(25, &displays, &strings),
        ),
        (
            "DBFilesClient\\ItemVisuals.dbc".into(),
            dbc(6, &[700, 710, 0, 0, 0, 0, 701, 711, 0, 0, 0, 0], b"\0"),
        ),
        (
            "DBFilesClient\\ItemVisualEffects.dbc".into(),
            dbc(
                2,
                &[710, 1, 711, 19],
                b"\0Spells\\Effect1.m2\0Spells\\Effect2.m2\0",
            ),
        ),
        (
            "DBFilesClient\\SpellItemEnchantment.dbc".into(),
            dbc(38, &enchants, b"\0"),
        ),
    ]);
    Ok(())
}

fn append_attachments(bytes: &mut Vec<u8>, ids: &[u32], sequences: usize) {
    let records = bytes.len();
    bytes.resize(records + ids.len() * 40, 0);
    for (index, id) in ids.iter().enumerate() {
        let record = records + index * 40;
        bytes[record..record + 4].copy_from_slice(&id.to_le_bytes());
        constant_track(bytes, record + 20, sequences, &[1]);
    }
    array(bytes, 0xf0, ids.len(), records);
    let lookup = bytes.len();
    for id in 0..=ids.iter().copied().max().unwrap_or(0) {
        let index = ids
            .iter()
            .position(|candidate| *candidate == id)
            .map_or(u16::MAX, |index| index as u16);
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    array(
        bytes,
        0xf8,
        ids.iter().copied().max().unwrap_or(0) as usize + 1,
        lookup,
    );
}

/// One ordinary emitter and one ribbon with constant tracks in every pose.
fn append_effects(bytes: &mut Vec<u8>, sequences: usize) {
    let particle = bytes.len();
    bytes.resize(particle + 476, 0);
    bytes[particle + 40] = 2; // Alpha blend.
    bytes[particle + 41] = 1; // Planar emitter.
    bytes[particle + 48..particle + 50].copy_from_slice(&1_u16.to_le_bytes());
    bytes[particle + 50..particle + 52].copy_from_slice(&1_u16.to_le_bytes());
    for (offset, value) in [
        (0x34, 1.0_f32),
        (0x48, 0.0),
        (0x5c, 0.0),
        (0x70, 0.0),
        (0x84, 0.0),
        (0x98, 5.0),
        (0xb0, 20.0),
        (0xc8, 0.0),
        (0xdc, 0.0),
        (0xf0, 0.0),
    ] {
        constant_track(bytes, particle + offset, sequences, &value.to_le_bytes());
    }
    constant_track(bytes, particle + 0x1c8, sequences, &[1]);
    array(bytes, 0x128, 1, particle);

    let ribbon = bytes.len();
    bytes.resize(ribbon + 176, 0);
    let index = bytes.len();
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    array(bytes, ribbon + 20, 1, index);
    array(bytes, ribbon + 28, 1, index);
    let white: Vec<_> = [1.0_f32; 3]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    constant_track(bytes, ribbon + 36, sequences, &white);
    constant_track(bytes, ribbon + 56, sequences, &i16::MAX.to_le_bytes());
    constant_track(bytes, ribbon + 76, sequences, &0.25_f32.to_le_bytes());
    constant_track(bytes, ribbon + 96, sequences, &0.25_f32.to_le_bytes());
    bytes[ribbon + 116..ribbon + 120].copy_from_slice(&20.0_f32.to_le_bytes());
    bytes[ribbon + 120..ribbon + 124].copy_from_slice(&5.0_f32.to_le_bytes());
    bytes[ribbon + 128..ribbon + 130].copy_from_slice(&1_u16.to_le_bytes());
    bytes[ribbon + 130..ribbon + 132].copy_from_slice(&1_u16.to_le_bytes());
    constant_track(bytes, ribbon + 132, sequences, &0_u16.to_le_bytes());
    constant_track(bytes, ribbon + 152, sequences, &[1]);
    bytes[ribbon + 174] = u8::MAX;
    bytes[ribbon + 175] = u8::MAX;
    array(bytes, 0x120, 1, ribbon);
}

fn constant_track(bytes: &mut Vec<u8>, track: usize, sequences: usize, value: &[u8]) {
    let time = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let data = bytes.len();
    bytes.extend_from_slice(value);
    let times = bytes.len();
    for _ in 0..sequences {
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&(time as u32).to_le_bytes());
    }
    let values = bytes.len();
    for _ in 0..sequences {
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&(data as u32).to_le_bytes());
    }
    bytes[track + 2..track + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    array(bytes, track + 4, sequences, times);
    array(bytes, track + 12, sequences, values);
}

fn array(bytes: &mut [u8], offset: usize, count: usize, data: usize) {
    bytes[offset..offset + 4].copy_from_slice(&(count as u32).to_le_bytes());
    bytes[offset + 4..offset + 8].copy_from_slice(&(data as u32).to_le_bytes());
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

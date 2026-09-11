//! Complete synthetic character/creature catalogs for live unit residency tests.

use super::{ClientFixture, game_object_models};
use std::error::Error;

pub fn fixture() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, false, None, None, None, None, None)
}

pub fn fixture_with_effects() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(true, false, None, None, None, None, None)
}

pub fn fixture_with_equipment() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, true, None, None, None, None, None)
}

pub fn fixture_with_equipped_npc() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, true, Some(1), None, None, None, None)
}

pub fn fixture_with_hairless_npc() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, false, Some(9), None, None, None, None)
}

pub fn fixture_with_water_effects(attachment: u32) -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, false, None, Some(attachment), None, None, None)
}

/// Mount events have a distinct point and the opposite breath attachment, so
/// consumers must resolve their target model through the unit's body.
pub fn fixture_with_mount_water_effects(attachment: u32) -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(
        false,
        false,
        None,
        Some(attachment),
        Some((0.4, 1.25)),
        Some((1.6, 3.5)),
        None,
    )
}

/// Distinct display/model scales and a family interval for live scale updates.
pub fn fixture_with_body_scale() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(false, false, None, None, Some((0.4, 1.25)), None, None)
}

/// Display 102 is a mount with independently authored display/model scales.
pub fn fixture_with_mount_scale() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(
        false,
        false,
        None,
        None,
        Some((0.4, 1.25)),
        Some((1.6, 3.5)),
        None,
    )
}

/// Live mount emitters and a saddle exercise independent component lifetime.
pub fn fixture_with_mount_effects() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(
        true,
        false,
        None,
        None,
        Some((0.4, 1.25)),
        Some((1.6, 3.5)),
        None,
    )
}

/// Animated vehicle bones, a passenger anchor, and authored seat offsets.
pub fn fixture_with_vehicle_seats() -> Result<ClientFixture, Box<dyn Error>> {
    fixture_with_vehicle_entry([0.25, 8., 20., 2., 2., 0., 20.])
}

pub fn fixture_with_vehicle_entry(parameters: [f32; 7]) -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture(true, false, None, None, None, None, Some(parameters))
}

pub fn fixture_with_vehicle_seated() -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture_options(
        true,
        false,
        None,
        None,
        None,
        None,
        Some([0.25, 8., 20., 2., 2., 0., 20.]),
        true,
        None,
    )
}

pub fn fixture_with_vehicle_ride_animation(
    animation: i32,
    key: i32,
) -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture_options(
        true,
        false,
        None,
        None,
        None,
        None,
        Some([0.25, 8., 20., 2., 2., 0., 20.]),
        true,
        Some((animation, key)),
    )
}

fn build_fixture(
    effects: bool,
    equipment: bool,
    npc_race: Option<u32>,
    water_attachment: Option<u32>,
    body_scale: Option<(f32, f32)>,
    mount_scale: Option<(f32, f32)>,
    vehicle_entry: Option<[f32; 7]>,
) -> Result<ClientFixture, Box<dyn Error>> {
    build_fixture_options(
        effects,
        equipment,
        npc_race,
        water_attachment,
        body_scale,
        mount_scale,
        vehicle_entry,
        false,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_fixture_options(
    effects: bool,
    equipment: bool,
    npc_race: Option<u32>,
    water_attachment: Option<u32>,
    body_scale: Option<(f32, f32)>,
    mount_scale: Option<(f32, f32)>,
    vehicle_entry: Option<[f32; 7]>,
    seated_animations: bool,
    vehicle_animation: Option<(i32, i32)>,
) -> Result<ClientFixture, Box<dyn Error>> {
    let vehicle_seats = vehicle_entry.is_some();
    let mut ids = vec![0, 91, 96, 97, 98, 99, 100, 101];
    if mount_scale.is_some() {
        ids.extend([4, 5, 37, 38, 39, 40, 187]);
    }
    if seated_animations {
        ids.extend([115, 116, 117, 118]);
    }
    let mut model = game_object_models::model_with_animations(&ids)?;
    let sequences = u32::from_le_bytes(model[0x20..0x24].try_into()?) as usize;
    for (index, id) in ids.iter().enumerate() {
        // The visible triangle's Stand bounds also define camera-less portraits.
        for (axis, value) in [0.0_f32, -1.0, -1.0, 0.0, 1.0, 1.0, 2.0]
            .into_iter()
            .enumerate()
        {
            let offset = sequences + index * 64 + 32 + axis * 4;
            model[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        if matches!(id, 37 | 39 | 96 | 98 | 99 | 101) {
            model[sequences + index * 64 + 12..sequences + index * 64 + 16]
                .copy_from_slice(&0x21_u32.to_le_bytes());
        }
        if *id >= 115 {
            let duration = match id {
                115 => 200_u32,
                116 => 800,
                117 => 300,
                _ => 900,
            };
            model[sequences + index * 64 + 4..sequences + index * 64 + 8]
                .copy_from_slice(&duration.to_le_bytes());
            model[sequences + index * 64 + 12..sequences + index * 64 + 16].copy_from_slice(
                &(if matches!(id, 115 | 117) {
                    0x21_u32
                } else {
                    0x20
                })
                .to_le_bytes(),
            );
        }
    }
    if effects {
        append_effects(&mut model, ids.len());
    }
    if equipment {
        append_attachments(
            &mut model,
            if npc_race.is_some() {
                &[0, 1, 2, 5, 6, 11, 26, 27, 28, 30, 31, 32, 33]
            } else {
                &[1, 5, 6, 11, 26]
            },
            ids.len(),
        );
    }
    if mount_scale.is_some() {
        append_attachments(&mut model, &[0], ids.len());
    }
    if let Some(attachment) = water_attachment {
        append_attachments(&mut model, &[attachment], ids.len());
        let record = u32::from_le_bytes(model[0xf4..0xf8].try_into()?) as usize;
        for (axis, value) in [0.25_f32, 0.5, 1.].into_iter().enumerate() {
            model[record + 8 + axis * 4..record + 12 + axis * 4]
                .copy_from_slice(&value.to_le_bytes());
        }
        let timestamp = model.len();
        model.extend_from_slice(&100_u32.to_le_bytes());
        model.extend_from_slice(&200_u32.to_le_bytes());
        let channels = model.len();
        for _ in &ids {
            model.extend_from_slice(&2_u32.to_le_bytes());
            model.extend_from_slice(&(timestamp as u32).to_le_bytes());
        }
        let event = model.len();
        model.resize(event + 36, 0);
        model[event..event + 4].copy_from_slice(b"$BTH");
        model[event + 26..event + 28].copy_from_slice(&u16::MAX.to_le_bytes());
        array(&mut model, event + 28, ids.len(), channels);
        array(&mut model, 0x100, 1, event);
    }
    if vehicle_seats {
        model[0x44..0x48].copy_from_slice(&2_u32.to_le_bytes());
        append_attachments(&mut model, &[0, 20], ids.len());
        let records = u32::from_le_bytes(model[0xf4..0xf8].try_into()?) as usize;
        for (index, position) in [[0.25_f32, 0.5, 1.], [2., -1., 3.]].into_iter().enumerate() {
            for (axis, value) in position.into_iter().enumerate() {
                model[records + index * 40 + 8 + axis * 4..records + index * 40 + 12 + axis * 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        let bone = u32::from_le_bytes(model[0x30..0x34].try_into()?) as usize;
        animated_vec3(&mut model, bone + 16, ids.len(), [0., 0., 0.], [0., 0., 2.]);
        if seated_animations {
            let root = model[bone..bone + 88].to_vec();
            let bones = model.len();
            model.extend_from_slice(&root);
            let mut upper = [0_u8; 88];
            upper[..4].copy_from_slice(&4_i32.to_le_bytes());
            for offset in [18, 38, 58] {
                upper[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
            }
            model.extend_from_slice(&upper);
            array(&mut model, 0x2c, 2, bones);
            let lookup = model.len();
            for bone in [u16::MAX, u16::MAX, u16::MAX, u16::MAX, 1] {
                model.extend_from_slice(&bone.to_le_bytes());
            }
            array(&mut model, 0x34, 5, lookup);
            animated_vec3(
                &mut model,
                bones + 88 + 16,
                ids.len(),
                [0., 0., 0.],
                [10., 0., 0.],
            );
        }
        animated_vec3(&mut model, bone + 56, ids.len(), [1., 1., 1.], [2., 2., 2.]);
    }
    let animations: Vec<_> = ids
        .iter()
        .flat_map(|id| {
            [
                u32::from(*id),
                0,
                if equipment && npc_race.is_some() && *id == 97 {
                    4
                } else {
                    0
                },
                0,
                0,
                0,
                u32::from(*id),
                0,
            ]
        })
        .collect();
    let mut display = [0; 16];
    display[0] = 100;
    display[1] = 7;
    display[4] = body_scale.map_or(1.0, |scale| scale.0).to_bits();
    display[5] = u32::MAX;
    let mut displays = display.to_vec();
    display[0] = 101;
    displays.extend_from_slice(&display);
    display[0] = 102;
    display[1] = 8;
    if let Some((scale, _)) = mount_scale {
        display[4] = scale.to_bits();
    }
    display[3] = u32::from(npc_race.is_some());
    if npc_race.is_some() {
        display[7] = 1;
        display[8] = 13;
    }
    displays.extend_from_slice(&display);
    if npc_race.is_some() {
        display[0] = 103;
        display[3] = 0;
        displays.extend_from_slice(&display);
        if equipment {
            display[0] = 104;
            display[3] = 2;
            displays.extend_from_slice(&display);
            display[0] = 105;
            display[1] = 9;
            display[3] = 0;
            display[7] = 0;
            display[8] = 0;
            displays.extend_from_slice(&display);
        }
    }
    let mut model_data = [0; 28];
    model_data[0] = 7;
    model_data[2] = 1;
    model_data[4] = body_scale.map_or(1.0, |scale| scale.1).to_bits();
    let mut models = model_data.to_vec();
    let mut model_paths = b"\0Character\\Human\\Male\\HumanMale.m2\0".to_vec();
    model_data[0] = 8;
    if let Some((_, scale)) = mount_scale {
        model_data[4] = scale.to_bits();
    }
    model_data[2] = model_paths.len() as u32;
    models.extend_from_slice(&model_data);
    model_paths.extend_from_slice(b"Creature\\Alternate.m2\0");
    if equipment && npc_race.is_some() {
        model_data[0] = 9;
        model_data[2] = model_paths.len() as u32;
        models.extend_from_slice(&model_data);
        model_paths.extend_from_slice(b"Creature\\NoHands.m2\0");
    }
    let mut sections: Vec<_> = (0..5)
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
    if let Some(race) = npc_race {
        sections.extend_from_slice(&[20, race, 0, 0, 1, 0, 0, 8, 0, 0]);
    }
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
    if equipment && npc_race.is_some() {
        let mut no_hands = model.clone();
        no_hands[0xf0..0x100].fill(0);
        files.extend([
            ("Creature\\NoHands.m2".to_owned(), no_hands),
            (
                "Creature\\NoHands00.skin".to_owned(),
                game_object_models::skin()?,
            ),
        ]);
    }
    if vehicle_seats {
        files.extend([
            (
                "Character\\Human\\Male\\HumanMale01.skin".to_owned(),
                game_object_models::skin()?,
            ),
            (
                "Creature\\Alternate01.skin".to_owned(),
                game_object_models::skin()?,
            ),
        ]);
    }
    // Native vehicle-seat fixture deliberately separates the flags sign bit
    // from the signed attachment ID used by passenger entry interpolation.
    let mut vehicle = [0_u32; 40];
    vehicle[0] = 1;
    vehicle[6..14].copy_from_slice(&[10, 11, 12, 0, 9, 13, 10, 12]);
    let mut seats = [0_u32; 116];
    seats[..3].copy_from_slice(&[10, 0, u32::MAX]);
    seats[58..61].copy_from_slice(&[12, 0x8000_0000, 21]);
    if let Some(parameters) = vehicle_entry {
        seats[59] |= 0x8001;
        seats[64..71].copy_from_slice(&parameters.map(f32::to_bits));
        seats[71..73].copy_from_slice(&[96, 91]);
        if seated_animations {
            seats[59] |= 6;
            seats[73..77].copy_from_slice(&[115, 116, 117, 118]);
        }
        if let Some((animation, key)) = vehicle_animation {
            seats[59] |= 0x20000;
            seats[93] = animation as u32;
            seats[96] = key as u32;
        }
        seats[77..84].copy_from_slice(&[0.125, 8., 20., 0.5, 0.5, 0., 20.].map(f32::to_bits));
        seats[84..86].copy_from_slice(&[99, 100]);
        seats[60] = 0; // Vehicle seat enum zero maps to M2 attachment 20.
        seats[61..64].copy_from_slice(&[
            0.5_f32.to_bits(),
            0.25_f32.to_bits(),
            (-0.5_f32).to_bits(),
        ]);
        seats[87..90].copy_from_slice(&[
            0.3_f32.to_bits(),
            0.2_f32.to_bits(),
            (-0.4_f32).to_bits(),
        ]);
        seats[90] = 0; // Passenger attachment 0 has a distinct static offset.
    }
    files.push((
        "DBFilesClient\\Vehicle.dbc".to_owned(),
        dbc(40, &vehicle, b"\0"),
    ));
    files.push((
        "DBFilesClient\\VehicleSeat.dbc".to_owned(),
        dbc(58, &seats, b"\0"),
    ));
    if water_attachment.is_some() {
        let mut effect = game_object_models::model_with_animations(&[0])?;
        append_effects(&mut effect, 1);
        let mut strings = b"\0World\\WaterEffect.m2\0".to_vec();
        let mut fields = Vec::new();
        for (index, kind) in [
            solarity_systems::UnitWaterEffect::RunSpray,
            solarity_systems::UnitWaterEffect::WalkSpray,
            solarity_systems::UnitWaterEffect::UnderwaterBreath,
            solarity_systems::UnitWaterEffect::ColdBreath,
            solarity_systems::UnitWaterEffect::InebriatedBubbles,
        ]
        .into_iter()
        .enumerate()
        {
            let name = strings.len() as u32;
            strings.extend_from_slice(kind.name().as_bytes());
            strings.push(0);
            fields.extend_from_slice(&[
                index as u32 + 1,
                name,
                1,
                0,
                1_f32.to_bits(),
                0,
                10_f32.to_bits(),
            ]);
        }
        files.extend([
            ("World\\WaterEffect.m2".to_owned(), effect),
            (
                "World\\WaterEffect00.skin".to_owned(),
                game_object_models::skin()?,
            ),
            (
                "DBFilesClient\\SpellVisualEffectName.dbc".to_owned(),
                dbc(7, &fields, &strings),
            ),
        ]);
    }
    if let Some(race) = npc_race {
        // Body plus three independent monster stages: empty, resident, missing.
        // Every slot has a visible batch, so GPU preparation must resolve all.
        let mut npc_model = model.clone();
        let texture_offset = npc_model.len() as u32;
        for kind in [1_u32, 11, 12, 13] {
            npc_model.extend_from_slice(&kind.to_le_bytes());
            npc_model.extend_from_slice(&[0; 12]);
        }
        npc_model[0x50..0x54].copy_from_slice(&4_u32.to_le_bytes());
        npc_model[0x54..0x58].copy_from_slice(&texture_offset.to_le_bytes());
        let lookup_offset = npc_model.len() as u32;
        for index in 0_u16..4 {
            npc_model.extend_from_slice(&index.to_le_bytes());
        }
        npc_model[0x80..0x84].copy_from_slice(&4_u32.to_le_bytes());
        npc_model[0x84..0x88].copy_from_slice(&lookup_offset.to_le_bytes());
        let mut npc_skin = game_object_models::skin()?;
        let batch_start = npc_skin.len() - 24;
        let batch = npc_skin[batch_start..].to_vec();
        for index in 1_u16..4 {
            let mut next = batch.clone();
            next[16..18].copy_from_slice(&index.to_le_bytes());
            npc_skin.extend_from_slice(&next);
        }
        npc_skin[36..40].copy_from_slice(&4_u32.to_le_bytes());
        for (path, bytes) in &mut files {
            match path.as_str() {
                "Creature\\Alternate.m2" => *bytes = npc_model.clone(),
                "Creature\\Alternate00.skin" => *bytes = npc_skin.clone(),
                "DBFilesClient\\CreatureDisplayInfo.dbc" => {
                    *bytes = dbc(16, &displays, b"\0MonsterSkin\0MissingSkin\0");
                }
                _ => {}
            }
        }
        files.push(("Creature\\MonsterSkin.blp".to_owned(), skin_texture()));
        let mut extra = [0; 21];
        extra[0] = 1;
        extra[1] = race;
        if equipment {
            extra[8] = 500;
            extra[9] = 600;
        }
        extra[20] = 1;
        let mut extras = extra.to_vec();
        if equipment {
            extra[0] = 2;
            extra[9] = 603;
            extras.extend_from_slice(&extra);
        }
        files.push((
            "DBFilesClient\\CreatureDisplayInfoExtra.dbc".to_owned(),
            dbc(21, &extras, b"\0HairlessNpc\0"),
        ));
        files.push((
            "Textures\\BakedNpcTextures\\HairlessNpc.blp".to_owned(),
            skin_texture(),
        ));
    }
    if body_scale.is_some() {
        let mut family = [0; 28];
        family[..5].copy_from_slice(&[1, 0.25_f32.to_bits(), 1, 1.25_f32.to_bits(), 5]);
        files.push((
            "DBFilesClient\\CreatureFamily.dbc".to_owned(),
            dbc(28, &family, b"\0"),
        ));
    }
    if mount_scale.is_some() {
        // Give the mount a distinct model box as well as a distinct scale.
        let alternate = files
            .iter_mut()
            .find(|(path, _)| path == "Creature\\Alternate.m2")
            .ok_or("mount model")?;
        if let Some(attachment) = water_attachment {
            append_attachments(
                &mut alternate.1,
                &[0, if attachment == 17 { 19 } else { 17 }],
                ids.len(),
            );
            let event = u32::from_le_bytes(alternate.1[0x104..0x108].try_into()?) as usize;
            alternate.1[event + 12..event + 16].copy_from_slice(&4.0_f32.to_le_bytes());
        }
        alternate.1[0xa0..0xa4].copy_from_slice(&(-2.0_f32).to_le_bytes());
        // Distinct mount strides prevent renderer tests from accidentally
        // using the rider's sequence metadata to scale locomotion.
        for (index, id) in ids.iter().enumerate() {
            if matches!(id, 4 | 5) {
                let record = sequences + index * 64;
                alternate.1[record + 4..record + 8].copy_from_slice(&2000_u32.to_le_bytes());
                alternate.1[record + 8..record + 12].copy_from_slice(&3.5_f32.to_le_bytes());
            }
        }
        alternate.1[0xac..0xb0].copy_from_slice(&2.0_f32.to_le_bytes());
        // Full ground alignment and an offset saddle distinguish the mount's
        // basis from its upright rider model and exercise attached translation.
        alternate.1[0x10..0x14].copy_from_slice(&3_u32.to_le_bytes());
        let attachment = u32::from_le_bytes(alternate.1[0xf4..0xf8].try_into()?) as usize;
        for (axis, value) in [0.25_f32, 0.5, 1.].into_iter().enumerate() {
            alternate.1[attachment + 8 + axis * 4..attachment + 12 + axis * 4]
                .copy_from_slice(&value.to_le_bytes());
        }
        append_mount_terrain(&mut files)?;
    }
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

/// A real resident outdoor tile lets mounted players enter scene depth lists.
fn append_mount_terrain(files: &mut Vec<(String, Vec<u8>)>) -> Result<(), Box<dyn Error>> {
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    manifest
        .main
        .get_mut(32, 32)
        .ok_or("tile")?
        .set_has_adt(true);
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let adt = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut std::io::Cursor::new(adt))?
    else {
        return Err("root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        chunk.header.position = [
            17_066.666_f32 - (512 + chunk.header.index_y) as f32 * 33.333_332,
            17_066.666_f32 - (512 + chunk.header.index_x) as f32 * 33.333_332,
            0.,
        ];
        chunk.heights.as_mut().ok_or("heights")?.heights.fill(0.);
    }
    let adt = wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    let mut map = [0; 66];
    map[1] = 1;
    map[5] = 1;
    map[59] = u32::MAX;
    files.extend([
        (
            "DBFilesClient\\Map.dbc".to_owned(),
            dbc(66, &map, b"\0Mount\0"),
        ),
        ("World\\Maps\\Mount\\Mount.wdt".to_owned(), wdt),
        ("World\\Maps\\Mount\\Mount_32_32.adt".to_owned(), adt),
        (
            "tileset\\fixture\\grass.blp".to_owned(),
            super::bootstrap_texture_blp(),
        ),
    ]);
    Ok(())
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
        "Item\\ObjectComponents\\Shield\\Weapon",
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
        (801, "Weapon.m2", "", 700, 0x1c0),
        (802, "MissingWeapon.m2", "", 0, 0),
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
    let mut items: Vec<_> = [
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
    for (id, class, subclass, inventory, sheath) in [
        (3100, 2, 7, 13, 1),
        (3101, 4, 6, 14, 4),
        (3102, 2, 2, 15, 2),
        (3103, 2, 8, 17, 2),
        (3104, 2, 3, 26, 1),
        (3105, 15, 0, 23, 0),
        (3106, 2, 7, 13, 1),
    ] {
        items.extend_from_slice(&[id, class, subclass, u32::MAX, 0, 801, inventory, sheath]);
    }
    items.extend_from_slice(&[3107, 2, 7, u32::MAX, 0, 802, 13, 1]);
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
pub(crate) fn append_effects(bytes: &mut Vec<u8>, sequences: usize) {
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

fn animated_vec3(
    bytes: &mut Vec<u8>,
    track: usize,
    sequences: usize,
    first: [f32; 3],
    last: [f32; 3],
) {
    let time = bytes.len();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&1000_u32.to_le_bytes());
    let data = bytes.len();
    for value in first.into_iter().chain(last) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let times = bytes.len();
    for _ in 0..sequences {
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&(time as u32).to_le_bytes());
    }
    let values = bytes.len();
    for _ in 0..sequences {
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&(data as u32).to_le_bytes());
    }
    bytes[track..track + 2].copy_from_slice(&1_u16.to_le_bytes());
    bytes[track + 2..track + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    array(bytes, track + 4, sequences, times);
    array(bytes, track + 12, sequences, values);
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

//! External stock-layout tests for environment sound tables.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AreaTableCatalog, AssetError, AssetStore, ClientDataRoot, LiquidTypeCatalog,
    Locale, SoundEmitterCatalog, WorldModelAreaCatalog, WorldModelAreaKey, ZoneSoundCatalog,
};

use crate::support::{Fixture, FixtureFile};

/// These tables begin with coordinate/state fields, never an inferred primary ID.
#[test]
fn zone_overrides_preserve_non_id_schemas_and_duplicate_chunk_replacement()
-> Result<(), Box<dyn Error>> {
    let chunks = create_wdbc(
        2,
        9,
        &[
            530, 41, 32, 6, 9, 11, 12, 13, 14, 530, 41, 32, 6, 9, 21, 22, 23, 24,
        ],
        &[0],
    );
    let states = create_wdbc(
        2,
        8,
        &[
            1001, 1, 71, 81, 31, 32, 33, 34, 1001, 0, 72, 82, 41, 42, 43, 44,
        ],
        &[0],
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\WorldChunkSounds.dbc",
            bytes: &chunks,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\WorldStateZoneSounds.dbc",
            bytes: &states,
        },
    ])?;
    let catalog = solarity_asset::ZoneSoundOverrideCatalog::load(&mut mounted_store(&fixture)?)?;
    let selected = catalog
        .chunk(solarity_asset::WorldChunkSoundKey {
            map_id: 530,
            tile: [41, 32],
            chunk: [6, 9],
        })
        .ok_or("chunk override")?;
    assert_eq!(
        [
            selected.intro_music_id,
            selected.zone_music_id,
            selected.ambience_id,
            selected.sound_provider_id
        ],
        [21, 22, 23, 24]
    );
    assert_eq!(catalog.world_states().len(), 2);
    let state = catalog.world_states()[1];
    assert_eq!(state.state, [1001, 0]);
    assert_eq!([state.area_id, state.world_model_area_id], [72, 82]);
    assert_eq!(state.sounds.zone_music_id, 42);
    Ok(())
}

/// Native loaders 6571D0/656F80/64D190/658B40 retain separate namespaces.
#[test]
fn zone_sound_catalogs_decode_authored_relations_and_delay_units() -> Result<(), Box<dyn Error>> {
    let music = create_wdbc(
        1,
        8,
        &[17, 1, 1100, 2200, 3300, 4400, 501, 502],
        b"\0Music\0",
    );
    let intro = create_wdbc(1, 5, &[17, 1, 601, 9, 3], b"\0Intro\0");
    let ambience = create_wdbc(1, 3, &[17, 701, 702], &[0]);
    let mut area_words = [0; 36];
    area_words[0] = 81;
    area_words[2] = 80;
    area_words[5..10].copy_from_slice(&[1, 2, 17, 18, 19]);
    let area = create_wdbc(1, 36, &area_words, &[0]);
    let mut wmo_words = [0; 28];
    wmo_words[..11].copy_from_slice(&[91, 92, 93, u32::MAX, 11, 12, 27, 28, 29, 0x20, 81]);
    wmo_words[11] = 1;
    let wmo = create_wdbc(1, 28, &wmo_words, b"\0Hall\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ZoneMusic.dbc",
            bytes: &music,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\ZoneIntroMusicTable.dbc",
            bytes: &intro,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundAmbience.dbc",
            bytes: &ambience,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\AreaTable.dbc",
            bytes: &area,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\WMOAreaTable.dbc",
            bytes: &wmo,
        },
    ])?;
    let mut store = mounted_store(&fixture)?;
    let catalog = ZoneSoundCatalog::load(&mut store)?;
    let music = catalog.music(17).ok_or("missing music")?;
    assert_eq!(music.name(), "Music");
    assert_eq!(music.minimum_delay_ms(), [1100, 2200]);
    assert_eq!(music.maximum_delay_ms(), [3300, 4400]);
    assert_eq!(music.sound_entry_ids(), [501, 502]);
    let intro = catalog.intro(17).ok_or("missing intro")?;
    assert_eq!(intro.sound_entry_id(), 601);
    assert_eq!(intro.minimum_delay_minutes(), 3);
    assert_eq!(intro.priority(), 9);
    assert_eq!(
        catalog
            .ambience(17)
            .ok_or("missing ambience")?
            .sound_entry_ids(),
        [701, 702]
    );
    assert!(catalog.music(18).is_none());
    let areas = AreaTableCatalog::load(&mut store)?;
    let area = areas.area(81).ok_or("missing area")?;
    assert_eq!(area.parent_area_id(), 80);
    assert_eq!(area.sounds().ambience_id, 17);
    assert_eq!(area.sounds().zone_music_id, 18);
    assert_eq!(area.sounds().intro_music_id, 19);
    let areas = WorldModelAreaCatalog::load(&mut store)?;
    let area = areas
        .area(WorldModelAreaKey {
            root_id: 92,
            name_set: 93,
            group_id: -1,
        })
        .ok_or("missing WMO area")?;
    assert_eq!(area.id(), 91);
    assert_eq!(area.name(), "Hall");
    assert_eq!(area.area_id(), 81);
    assert_eq!(area.sounds().sound_provider_id, 11);
    assert_eq!(area.sounds().underwater_sound_provider_id, 12);
    assert_eq!(area.sounds().ambience_id, 27);
    assert_eq!(area.sounds().zone_music_id, 28);
    assert_eq!(area.sounds().intro_music_id, 29);
    Ok(())
}

/// LiquidType decodes the complete 45-word WotLK row without MCSE semantics.
#[test]
fn liquid_type_catalog_decodes_sound_and_presentation_fields() -> Result<(), Box<dyn Error>> {
    let table = liquid_type_fixture();
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\LiquidType.dbc",
        bytes: &table,
    }])?;
    let mut store = mounted_store(&fixture)?;
    let catalog = LiquidTypeCatalog::load(&mut store)?;
    let entry = catalog.entry(7).ok_or("missing liquid type fixture")?;

    assert_eq!(entry.name(), "River");
    assert_eq!(entry.flags(), 0x80);
    assert_eq!(entry.sound_bank(), 2);
    assert_eq!(entry.sound_entry_id(), 42);
    assert_eq!(entry.spell_id(), 99);
    assert_eq!(entry.darken_parameters(), [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(entry.light_id(), 4);
    assert_eq!(entry.particle_scale(), 0.5);
    assert_eq!(entry.particle_movement(), 6);
    assert_eq!(entry.particle_texture_slots(), 7);
    assert_eq!(entry.material_id(), 8);
    assert_eq!(entry.textures()[0], "XTextures\\river.1.blp");
    assert_eq!(entry.textures()[5], "XTextures\\river.6.blp");
    assert_eq!(entry.colors(), [0x1122_3344, 0x5566_7788]);
    assert_eq!(entry.float_parameters()[0], 1.0);
    assert_eq!(entry.float_parameters()[17], 18.0);
    assert_eq!(entry.integer_parameters(), [21, 22, 23, 24]);
    Ok(())
}

/// SoundEmitters remains a distinct global table with an advanced sound key.
#[test]
fn sound_emitter_catalog_decodes_global_rows() -> Result<(), Box<dyn Error>> {
    let table = sound_emitter_fixture();
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\SoundEmitters.dbc",
        bytes: &table,
    }])?;
    let mut store = mounted_store(&fixture)?;
    let catalog = SoundEmitterCatalog::load(&mut store)?;
    let entry = catalog.entry(9).ok_or("missing sound emitter fixture")?;

    assert_eq!(entry.position(), [1.0, 2.0, 3.0]);
    assert_eq!(entry.direction(), [0.0, 1.0, 0.0]);
    assert_eq!(entry.advanced_sound_entry_id(), 90);
    assert_eq!(entry.map_id(), 571);
    assert_eq!(entry.name(), "Howling Fjord emitter");
    Ok(())
}

/// Both tables reject another expansion's record layout without fallback.
#[test]
fn environment_sound_catalogs_reject_non_stock_layouts() -> Result<(), Box<dyn Error>> {
    let wrong_liquid = create_wdbc(0, 44, &[], &[0]);
    let wrong_emitter = create_wdbc(0, 11, &[], &[0]);
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\LiquidType.dbc",
            bytes: &wrong_liquid,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\SoundEmitters.dbc",
            bytes: &wrong_emitter,
        },
    ])?;
    let mut store = mounted_store(&fixture)?;
    assert!(matches!(
        LiquidTypeCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    assert!(matches!(
        SoundEmitterCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

/// Presentation floats remain finite at the typed database boundary.
#[test]
fn liquid_type_catalog_rejects_nonfinite_parameters() -> Result<(), Box<dyn Error>> {
    let mut table = liquid_type_fixture();
    set_record_field(&mut table, 40, f32::NAN.to_bits());
    let fixture = Fixture::new(&[FixtureFile {
        archive: "common.MPQ",
        path: "DBFilesClient\\LiquidType.dbc",
        bytes: &table,
    }])?;
    let mut store = mounted_store(&fixture)?;

    assert!(matches!(
        LiquidTypeCatalog::load(&mut store),
        Err(AssetError::DatabaseDecode { .. })
    ));
    Ok(())
}

/// Builds one complete liquid-type row.
fn liquid_type_fixture() -> Vec<u8> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, "River");
    let textures: [u32; 6] = std::array::from_fn(|index| {
        append_string(&mut strings, &format!("XTextures\\river.{}.blp", index + 1))
    });
    let mut fields = vec![0; 45];
    fields[0] = 7;
    fields[1] = name;
    fields[2] = 0x80;
    fields[3] = 2;
    fields[4] = 42;
    fields[5] = 99;
    for (field, value) in fields[6..10].iter_mut().zip([0.1_f32, 0.2, 0.3, 0.4]) {
        *field = value.to_bits();
    }
    fields[10] = 4;
    fields[11] = 0.5_f32.to_bits();
    fields[12] = 6;
    fields[13] = 7;
    fields[14] = 8;
    fields[15..21].copy_from_slice(&textures);
    fields[21] = 0x1122_3344;
    fields[22] = 0x5566_7788;
    for (index, field) in fields[23..41].iter_mut().enumerate() {
        *field = (index as f32 + 1.0).to_bits();
    }
    fields[41..45].copy_from_slice(&[21, 22, 23, 24]);
    create_wdbc(1, 45, &fields, &strings)
}

/// Builds one complete global sound-emitter row.
fn sound_emitter_fixture() -> Vec<u8> {
    let mut strings = vec![0];
    let name = append_string(&mut strings, "Howling Fjord emitter");
    let fields = [
        9,
        1.0_f32.to_bits(),
        2.0_f32.to_bits(),
        3.0_f32.to_bits(),
        0.0_f32.to_bits(),
        1.0_f32.to_bits(),
        0.0_f32.to_bits(),
        90,
        571,
        name,
    ];
    create_wdbc(1, 10, &fields, &strings)
}

/// Mounts the exact generated consolidated archive layout.
fn mounted_store(fixture: &Fixture) -> Result<AssetStore, AssetError> {
    AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)
}

/// Serializes one fixed-field WDBC table.
fn create_wdbc(record_count: u32, field_count: u32, fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + strings.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Adds one NUL-terminated fixture string and returns its table offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

/// Replaces one word in the first serialized WDBC record.
fn set_record_field(bytes: &mut [u8], field: usize, value: u32) {
    const WDBC_HEADER_SIZE: usize = 20;
    let offset = WDBC_HEADER_SIZE + field * 4;
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

//! External stock-compatibility tests for class-aware character sections.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetStore, CharacterAppearanceCatalog, CharacterCustomization, ClientDataRoot,
    Locale,
};

use crate::support::{Fixture, FixtureFile};

/// Duplicate player rows select DK textures only for class six.
#[test]
fn character_sections_do_not_leak_death_knight_rows_into_other_classes()
-> Result<(), Box<dyn Error>> {
    let mut strings = vec![0];
    let normal = append_string(&mut strings, "Character\\Human\\Male\\Normal.blp");
    let death_knight = append_string(&mut strings, "Character\\Human\\Male\\DeathKnight.blp");
    let mut fields = Vec::new();
    for (base_section, variation, color) in [(0, 0, 0), (1, 0, 0), (3, 0, 0), (4, 0, 0)] {
        fields.extend_from_slice(&[
            100 + base_section,
            1,
            0,
            base_section,
            normal,
            0,
            0,
            0x01,
            variation,
            color,
        ]);
        fields.extend_from_slice(&[
            200 + base_section,
            1,
            0,
            base_section,
            death_knight,
            0,
            0,
            0x05,
            variation,
            color,
        ]);
    }
    let sections = create_wdbc(8, 10, &fields, &strings);
    let hair = create_wdbc(1, 6, &[1, 1, 0, 0, 1, 1], b"\0");
    let facial = create_wdbc(0, 8, &[], b"\0");
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &sections,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let customization = CharacterCustomization::new(0, 0, 0, 0, 0);

    let ordinary = catalog.resolve_player_for_class(1, 0, 1, customization)?;
    let death_knight_character = catalog.resolve_player_for_class(1, 0, 6, customization)?;

    assert_eq!(
        ordinary.skin().texture_names()[0],
        "Character\\Human\\Male\\Normal.blp"
    );
    assert_eq!(
        death_knight_character.skin().texture_names()[0],
        "Character\\Human\\Male\\DeathKnight.blp"
    );
    assert_eq!(ordinary.face().map(|section| section.id()), Some(101));
    assert_eq!(
        death_knight_character.face().map(|section| section.id()),
        Some(201)
    );
    Ok(())
}

/// 0x004F39A0 distinguishes ordinary, shared-DK, exclusive-DK, and NPC rows.
#[test]
fn creation_selectors_use_stock_class_flags_and_color_dependent_features()
-> Result<(), Box<dyn Error>> {
    let mut fields = Vec::new();
    for (color, flags) in [0x01, 0x11, 0x05, 0x09, 0x19, 0x15].into_iter().enumerate() {
        fields.extend_from_slice(&[color as u32 + 1, 1, 0, 0, 0, 0, 0, flags, 0, color as u32]);
    }
    for (style, flags) in [0x11, 0x01, 0x05, 0x09].into_iter().enumerate() {
        fields.extend_from_slice(&[style as u32 + 10, 1, 0, 2, 0, 0, 0, flags, style as u32, 0]);
    }
    // No underwear is required to count authored skin choices. Facial texture
    // rows exist for male/color zero only; females exercise the geometry branch.
    let sections = create_wdbc(10, 10, &fields, b"\0");
    let hair = create_wdbc(0, 6, &[], b"\0");
    let facial = create_wdbc(
        2,
        8,
        &[1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0],
        b"\0",
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &sections,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &hair,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &facial,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = CharacterAppearanceCatalog::load(&mut store)?;
    assert_eq!(catalog.player_skin_colors_for_class(1, 0, 1), [0, 1]);
    assert_eq!(catalog.player_skin_colors_for_class(1, 0, 6), [1, 2, 5]);
    assert_eq!(
        catalog.player_facial_hair_styles_for_class(1, 0, 0, 1),
        [0, 1]
    );
    assert_eq!(
        catalog.player_facial_hair_styles_for_class(1, 0, 0, 6),
        [0, 2]
    );
    assert!(
        catalog
            .player_facial_hair_styles_for_class(1, 0, 1, 1)
            .is_empty()
    );
    assert_eq!(
        catalog.player_facial_hair_styles_for_class(1, 1, 0, 1),
        [0, 1]
    );
    Ok(())
}

fn create_wdbc(record_count: u32, field_count: u32, fields: &[u32], strings: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
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

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

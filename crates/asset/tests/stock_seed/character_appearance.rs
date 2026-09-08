//! External stock-compatibility tests for character sections and creation eligibility.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetStore, CharacterAppearanceCatalog, CharacterCustomization, ClientDataRoot,
    Locale,
};

use crate::support::{Fixture, FixtureFile};

/// 0x004F3DD0 fills the render bank in physical order without class filtering.
#[test]
fn character_render_sections_use_last_physical_row_regardless_of_creation_flags()
-> Result<(), Box<dyn Error>> {
    let mut fields = Vec::new();
    let cases = include_str!("../fixtures/character_section_bank_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;
    for row in &cases {
        let [race, gender, base, variation, color, flags, _expected] = row.as_slice() else {
            return Err("invalid native section fixture".into());
        };
        for (id, flags) in [
            (1000 + base * 32 + color, 1),
            (2000 + base * 32 + color, *flags),
        ] {
            fields.extend_from_slice(&[
                id, *race, *gender, *base, 0, 0, 0, flags, *variation, *color,
            ]);
        }
    }
    let sections = create_wdbc((cases.len() * 2) as u32, 10, &fields, b"\0");
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
    for color in 0..32 {
        let customization = CharacterCustomization::new(color, 0, 0, color, 0);
        let character = catalog.resolve_player(1, 0, customization)?;
        let expected = |base: usize| cases[base * 32 + color as usize][6];
        assert_eq!(character.skin().id(), expected(0));
        assert_eq!(character.hair().map(|row| row.id()), Some(expected(3)));
        assert_eq!(
            character.facial_hair().map(|row| row.id()),
            Some(expected(2))
        );
        if color & 8 == 0 {
            assert_eq!(character.face().map(|row| row.id()), Some(expected(1)));
            assert_eq!(character.underwear().map(|row| row.id()), Some(expected(4)));
        }
    }
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

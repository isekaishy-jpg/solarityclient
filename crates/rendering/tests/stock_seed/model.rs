//! External stock-compatibility tests for character-model render preparation.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetStore, CharacterAppearanceCatalog, CharacterCustomization, ClientDataRoot,
    Locale,
};
use solarity_rendering::{CharacterAtlasLayerKind, CharacterAtlasRegion, CharacterTexturePlan};

use crate::support::{Fixture, FixtureFile};

/// Base player customization produces stock atlas and M2 replacement bindings.
#[test]
fn character_texture_plan_preserves_stock_regions_and_layer_order() -> Result<(), Box<dyn Error>> {
    let tables = character_tables(0);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &tables.sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &tables.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &tables.facial_hair,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let characters = CharacterAppearanceCatalog::load(&mut store)?;
    let appearance = characters.resolve_player(1, 0, CharacterCustomization::new(2, 3, 4, 5, 6))?;

    let plan = CharacterTexturePlan::base(&appearance)?;

    assert_eq!(plan.atlas_size(), 256);
    assert_eq!(plan.atlas_layers().len(), 16);
    let visible_order = plan
        .atlas_layers()
        .iter()
        .map(|layer| (layer.region(), layer.kind()))
        .collect::<Vec<_>>();
    assert_eq!(
        visible_order,
        [
            (
                CharacterAtlasRegion::ArmUpper,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::ArmLower,
                CharacterAtlasLayerKind::Skin
            ),
            (CharacterAtlasRegion::Hand, CharacterAtlasLayerKind::Skin),
            (
                CharacterAtlasRegion::TorsoUpper,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::TorsoUpper,
                CharacterAtlasLayerKind::Underwear,
            ),
            (
                CharacterAtlasRegion::TorsoLower,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::LegUpper,
                CharacterAtlasLayerKind::Skin
            ),
            (
                CharacterAtlasRegion::LegUpper,
                CharacterAtlasLayerKind::Underwear,
            ),
            (
                CharacterAtlasRegion::LegLower,
                CharacterAtlasLayerKind::Skin
            ),
            (CharacterAtlasRegion::Foot, CharacterAtlasLayerKind::Skin),
            (
                CharacterAtlasRegion::HeadUpper,
                CharacterAtlasLayerKind::Face
            ),
            (
                CharacterAtlasRegion::HeadUpper,
                CharacterAtlasLayerKind::FacialHair,
            ),
            (
                CharacterAtlasRegion::HeadUpper,
                CharacterAtlasLayerKind::Hair
            ),
            (
                CharacterAtlasRegion::HeadLower,
                CharacterAtlasLayerKind::Face
            ),
            (
                CharacterAtlasRegion::HeadLower,
                CharacterAtlasLayerKind::FacialHair,
            ),
            (
                CharacterAtlasRegion::HeadLower,
                CharacterAtlasLayerKind::Hair
            ),
        ]
    );
    let head_lower = CharacterAtlasRegion::HeadLower.rect();
    assert_eq!(
        (
            head_lower.x(),
            head_lower.y(),
            head_lower.width(),
            head_lower.height(),
        ),
        (0, 192, 128, 64)
    );
    assert_eq!(
        plan.hair().map(|path| path.as_str()),
        Some("CHARACTER\\HUMAN\\MALE\\HAIR.BLP")
    );
    assert_eq!(
        plan.extra_skin().map(|path| path.as_str()),
        Some("CHARACTER\\HUMAN\\MALE\\SKINEXTRA.BLP")
    );
    Ok(())
}

/// Texture table bytes needed by one exact appearance lookup.
struct CharacterTables {
    sections: Vec<u8>,
    hair_geosets: Vec<u8>,
    facial_hair: Vec<u8>,
}

/// Builds character DBC rows with a caller-selected skin flag word.
fn character_tables(skin_flags: u32) -> CharacterTables {
    let mut strings = vec![0];
    let skin = append_string(&mut strings, "Character\\Human\\Male\\Skin.blp");
    let extra = append_string(&mut strings, "Character\\Human\\Male\\SkinExtra.blp");
    let face_lower = append_string(&mut strings, "Character\\Human\\Male\\FaceLower.blp");
    let face_upper = append_string(&mut strings, "Character\\Human\\Male\\FaceUpper.blp");
    let facial_lower = append_string(&mut strings, "Character\\Human\\Male\\FacialLower.blp");
    let facial_upper = append_string(&mut strings, "Character\\Human\\Male\\FacialUpper.blp");
    let hair = append_string(&mut strings, "Character\\Human\\Male\\Hair.blp");
    let hair_lower = append_string(&mut strings, "Character\\Human\\Male\\HairLower.blp");
    let hair_upper = append_string(&mut strings, "Character\\Human\\Male\\HairUpper.blp");
    let underwear_lower = append_string(&mut strings, "Character\\Human\\Male\\UnderwearLower.blp");
    let underwear_upper = append_string(&mut strings, "Character\\Human\\Male\\UnderwearUpper.blp");
    let fields = [
        10,
        1,
        0,
        0,
        skin,
        extra,
        0,
        skin_flags,
        0,
        2,
        11,
        1,
        0,
        1,
        face_lower,
        face_upper,
        0,
        0,
        3,
        2,
        12,
        1,
        0,
        2,
        facial_lower,
        facial_upper,
        0,
        0,
        6,
        5,
        13,
        1,
        0,
        3,
        hair,
        hair_lower,
        hair_upper,
        0,
        4,
        5,
        14,
        1,
        0,
        4,
        underwear_lower,
        underwear_upper,
        0,
        0,
        0,
        2,
    ];
    CharacterTables {
        sections: create_wdbc(5, 10, &fields, &strings),
        hair_geosets: create_wdbc(0, 6, &[], b"\0"),
        facial_hair: create_wdbc(0, 8, &[], b"\0"),
    }
}

/// Generates one fixed-layout WDBC table.
fn create_wdbc(
    record_count: u32,
    field_count: u32,
    fields: &[u32],
    string_block: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + fields.len() * 4 + string_block.len());
    bytes.extend_from_slice(b"WDBC");
    bytes.extend_from_slice(&record_count.to_le_bytes());
    bytes.extend_from_slice(&field_count.to_le_bytes());
    bytes.extend_from_slice(&(field_count * 4).to_le_bytes());
    bytes.extend_from_slice(&(string_block.len() as u32).to_le_bytes());
    for field in fields {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    bytes.extend_from_slice(string_block);
    bytes
}

/// Appends one NUL-terminated DBC string and returns its offset.
fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

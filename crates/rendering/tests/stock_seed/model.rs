//! External stock-compatibility tests for character-model render preparation.

use std::error::Error;

use solarity_asset::{
    ArchiveCatalog, AssetStore, BlpTextureCache, CharacterAppearanceCatalog,
    CharacterCustomization, ClientDataRoot, ItemDefinitionCatalog, ItemDisplayCatalog, Locale,
};
use solarity_ecs::PlayerEquipmentSlot;
use solarity_rendering::{
    CharacterAtlasLayerKind, CharacterAtlasRegion, CharacterEquipmentItem, CharacterTexturePlan,
};

use crate::support::{Fixture, FixtureFile};

/// Base player customization produces stock atlas and M2 replacement bindings.
#[test]
fn character_texture_plan_preserves_stock_regions_and_layer_order() -> Result<(), Box<dyn Error>> {
    let tables = character_tables(0, 0);
    let skin = solid_raw3_blp(
        512,
        512,
        &[
            0xFFFF_0000,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
            0xFF00_00FF,
        ],
    );
    let overlay = solid_raw3_blp(
        256,
        128,
        &[
            0x80FF_0000,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
            0x8000_FF00,
        ],
    );
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
        FixtureFile {
            path: "Character\\Human\\Male\\Skin.blp",
            bytes: &skin,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FaceLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FaceUpper.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FacialLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\FacialUpper.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\HairLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\HairUpper.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\UnderwearLower.blp",
            bytes: &overlay,
        },
        FixtureFile {
            path: "Character\\Human\\Male\\UnderwearUpper.blp",
            bytes: &overlay,
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

    let mut texture_cache = BlpTextureCache::new();
    let atlas = plan.compose(&mut store, &mut texture_cache)?;
    assert_eq!(atlas.mips().len(), 9);
    assert_eq!(atlas.mip(0).map(|mip| mip.width()), Some(256));
    assert_eq!(atlas.mip(8).map(|mip| mip.width()), Some(1));
    let top = atlas.mip(0).ok_or("top character atlas mip is absent")?;
    // The 512-pixel HD skin selects authored mip one instead of resampling mip
    // zero. Underwear and head overlays likewise select their authored mip one.
    assert_eq!(
        rgba8_pixel(top.rgba8(), top.width(), 10, 10),
        [0, 0, 255, 255]
    );
    assert_eq!(
        rgba8_pixel(top.rgba8(), top.width(), 130, 10),
        [0, 128, 127, 255]
    );
    assert_eq!(
        rgba8_pixel(top.rgba8(), top.width(), 10, 170),
        [0, 223, 0, 255]
    );
    assert_eq!(texture_cache.len(), 9);
    Ok(())
}

/// Equipped textures follow stock priorities and universal-first suffix lookup.
#[test]
fn equipped_character_plan_orders_item_components() -> Result<(), Box<dyn Error>> {
    let characters = character_tables(0, 1);
    let items = equipment_tables();
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\CharSections.dbc",
            bytes: &characters.sections,
        },
        FixtureFile {
            path: "DBFilesClient\\CharHairGeosets.dbc",
            bytes: &characters.hair_geosets,
        },
        FixtureFile {
            path: "DBFilesClient\\CharacterFacialHairStyles.dbc",
            bytes: &characters.facial_hair,
        },
        FixtureFile {
            path: "DBFilesClient\\Item.dbc",
            bytes: &items.definitions,
        },
        FixtureFile {
            path: "DBFilesClient\\ItemDisplayInfo.dbc",
            bytes: &items.displays,
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmUpperTexture\\ShirtAU_U.blp",
            bytes: b"universal shirt",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmLowerTexture\\ShirtAL_U.blp",
            bytes: b"universal shirt",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\TorsoUpperTexture\\ShirtTU_U.blp",
            bytes: b"universal shirt",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmUpperTexture\\ChestAU_U.blp",
            bytes: b"universal chest",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmUpperTexture\\ChestAU_F.blp",
            bytes: b"female chest must lose to universal",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmLowerTexture\\ChestAL_U.blp",
            bytes: b"universal chest",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\TorsoUpperTexture\\ChestTU_U.blp",
            bytes: b"universal chest",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\ArmLowerTexture\\GloveAL_F.blp",
            bytes: b"female gloves",
        },
        FixtureFile {
            path: "Item\\TextureComponents\\HandTexture\\GloveHA_F.blp",
            bytes: b"female gloves",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let character_catalog = CharacterAppearanceCatalog::load(&mut store)?;
    let definitions = ItemDefinitionCatalog::load(&mut store)?;
    let displays = ItemDisplayCatalog::load(&mut store)?;
    let appearance =
        character_catalog.resolve_player(1, 1, CharacterCustomization::new(2, 3, 4, 5, 6))?;
    let shirt_definition = definitions.item(50_001).ok_or("shirt item is absent")?;
    let chest_definition = definitions.item(50_002).ok_or("chest item is absent")?;
    let glove_definition = definitions.item(50_003).ok_or("glove item is absent")?;
    let shirt_display = displays.display(55_001).ok_or("shirt display is absent")?;
    let chest_display = displays.display(55_002).ok_or("chest display is absent")?;
    let glove_display = displays.display(55_003).ok_or("glove display is absent")?;

    let plan = CharacterTexturePlan::equipped(
        &appearance,
        &store,
        [
            CharacterEquipmentItem::new(
                PlayerEquipmentSlot::Shirt,
                shirt_definition,
                shirt_display,
            ),
            CharacterEquipmentItem::new(
                PlayerEquipmentSlot::Chest,
                chest_definition,
                chest_display,
            ),
            CharacterEquipmentItem::new(
                PlayerEquipmentSlot::Hands,
                glove_definition,
                glove_display,
            ),
        ],
    )?;

    let item_layers = plan
        .atlas_layers()
        .iter()
        .filter(|layer| layer.kind() == CharacterAtlasLayerKind::Item)
        .map(|layer| (layer.region(), layer.path().as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        item_layers,
        [
            (
                CharacterAtlasRegion::ArmUpper,
                "ITEM\\TEXTURECOMPONENTS\\ARMUPPERTEXTURE\\SHIRTAU_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmUpper,
                "ITEM\\TEXTURECOMPONENTS\\ARMUPPERTEXTURE\\CHESTAU_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmLower,
                "ITEM\\TEXTURECOMPONENTS\\ARMLOWERTEXTURE\\SHIRTAL_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmLower,
                "ITEM\\TEXTURECOMPONENTS\\ARMLOWERTEXTURE\\CHESTAL_U.BLP",
            ),
            (
                CharacterAtlasRegion::ArmLower,
                "ITEM\\TEXTURECOMPONENTS\\ARMLOWERTEXTURE\\GLOVEAL_F.BLP",
            ),
            (
                CharacterAtlasRegion::Hand,
                "ITEM\\TEXTURECOMPONENTS\\HANDTEXTURE\\GLOVEHA_F.BLP",
            ),
            (
                CharacterAtlasRegion::TorsoUpper,
                "ITEM\\TEXTURECOMPONENTS\\TORSOUPPERTEXTURE\\SHIRTTU_U.BLP",
            ),
            (
                CharacterAtlasRegion::TorsoUpper,
                "ITEM\\TEXTURECOMPONENTS\\TORSOUPPERTEXTURE\\CHESTTU_U.BLP",
            ),
        ]
    );
    assert!(!plan.atlas_layers().iter().any(|layer| {
        layer.region() == CharacterAtlasRegion::TorsoUpper
            && layer.kind() == CharacterAtlasLayerKind::Underwear
    }));
    assert!(plan.atlas_layers().iter().any(|layer| {
        layer.region() == CharacterAtlasRegion::LegUpper
            && layer.kind() == CharacterAtlasLayerKind::Underwear
    }));
    Ok(())
}

/// Texture table bytes needed by one exact appearance lookup.
struct CharacterTables {
    sections: Vec<u8>,
    hair_geosets: Vec<u8>,
    facial_hair: Vec<u8>,
}

/// Builds character DBC rows with a caller-selected skin flag word.
fn character_tables(skin_flags: u32, gender_id: u32) -> CharacterTables {
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
        gender_id,
        0,
        skin,
        extra,
        0,
        skin_flags,
        0,
        2,
        11,
        1,
        gender_id,
        1,
        face_lower,
        face_upper,
        0,
        0,
        3,
        2,
        12,
        1,
        gender_id,
        2,
        facial_lower,
        facial_upper,
        0,
        0,
        6,
        5,
        13,
        1,
        gender_id,
        3,
        hair,
        hair_lower,
        hair_upper,
        0,
        4,
        5,
        14,
        1,
        gender_id,
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

/// Exact item and item-display rows used by equipped texture planning.
struct EquipmentTables {
    definitions: Vec<u8>,
    displays: Vec<u8>,
}

/// Builds shirt, chest, and glove rows with overlapping component regions.
fn equipment_tables() -> EquipmentTables {
    let definitions = create_wdbc(
        3,
        8,
        &[
            50_001,
            4,
            0,
            u32::MAX,
            1,
            55_001,
            4,
            0,
            50_002,
            4,
            0,
            u32::MAX,
            1,
            55_002,
            5,
            0,
            50_003,
            4,
            0,
            u32::MAX,
            1,
            55_003,
            10,
            0,
        ],
        b"\0",
    );

    let mut strings = vec![0];
    let shirt_au = append_string(&mut strings, "ShirtAU");
    let shirt_al = append_string(&mut strings, "ShirtAL");
    let shirt_tu = append_string(&mut strings, "ShirtTU");
    let chest_au = append_string(&mut strings, "ChestAU");
    let chest_al = append_string(&mut strings, "ChestAL");
    let chest_tu = append_string(&mut strings, "ChestTU");
    let glove_al = append_string(&mut strings, "GloveAL");
    let glove_ha = append_string(&mut strings, "GloveHA");
    let mut display_fields = Vec::with_capacity(75);
    display_fields.extend(item_display_fields(
        55_001,
        [0, 0, 0],
        [shirt_au, shirt_al, 0, shirt_tu, 0, 0, 0, 0],
    ));
    display_fields.extend(item_display_fields(
        55_002,
        [1, 0, 0],
        [chest_au, chest_al, 0, chest_tu, 0, 0, 0, 0],
    ));
    display_fields.extend(item_display_fields(
        55_003,
        [1, 0, 0],
        [0, glove_al, glove_ha, 0, 0, 0, 0, 0],
    ));
    EquipmentTables {
        definitions,
        displays: create_wdbc(3, 25, &display_fields, &strings),
    }
}

/// Produces one complete 25-field item-display row.
fn item_display_fields(id: u32, geosets: [u32; 3], components: [u32; 8]) -> [u32; 25] {
    [
        id,
        0,
        0,
        0,
        0,
        0,
        0,
        geosets[0],
        geosets[1],
        geosets[2],
        0,
        0,
        0,
        0,
        0,
        components[0],
        components[1],
        components[2],
        components[3],
        components[4],
        components[5],
        components[6],
        components[7],
        0,
        0,
    ]
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

/// Builds a BLP2/RAW3 authored mip chain with one solid color per level.
fn solid_raw3_blp(width: u32, height: u32, colors: &[u32]) -> Vec<u8> {
    const HEADER_SIZE: u32 = 148;
    const PALETTE_SIZE: u32 = 256 * 4;
    const PIXEL_OFFSET: u32 = HEADER_SIZE + PALETTE_SIZE;

    let mut offsets = [0_u32; 16];
    let mut sizes = [0_u32; 16];
    let mut next_offset = PIXEL_OFFSET;
    for (level, _color) in colors.iter().take(16).enumerate() {
        let mip_width = (width >> level).max(1);
        let mip_height = (height >> level).max(1);
        let byte_size = mip_width.saturating_mul(mip_height).saturating_mul(4);
        offsets[level] = next_offset;
        sizes[level] = byte_size;
        next_offset = next_offset.saturating_add(byte_size);
    }

    let mut bytes = Vec::with_capacity(next_offset as usize);
    bytes.extend_from_slice(b"BLP2");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&[3, 8, 8, u8::from(colors.len() > 1)]);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for size in sizes {
        bytes.extend_from_slice(&size.to_le_bytes());
    }
    bytes.resize(PIXEL_OFFSET as usize, 0);
    for (level, color) in colors.iter().take(16).enumerate() {
        let pixel_count = (width >> level).max(1) * (height >> level).max(1);
        for _pixel in 0..pixel_count {
            bytes.extend_from_slice(&color.to_le_bytes());
        }
    }
    bytes
}

/// Reads one tightly packed RGBA8 fixture pixel.
fn rgba8_pixel(rgba8: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * width + x) * 4) as usize;
    [
        rgba8[index],
        rgba8[index + 1],
        rgba8[index + 2],
        rgba8[index + 3],
    ]
}

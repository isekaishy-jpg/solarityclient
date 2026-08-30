//! External stock-compatibility tests for WDT map manifests and tile paths.

use std::error::Error;
use std::io::Cursor;
use std::path::Path;

use solarity_asset::{
    ArchiveCatalog, AssetError, AssetStore, ClientDataRoot, Locale, MapCatalog, TerrainMap,
    TerrainShadowMap, TerrainTileIndex,
};
use wow_adt::builder::{AdtBuilder, BuiltAdt};
use wow_adt::chunks::MtxfChunk;
use wow_adt::{
    AdtVersion, DoodadPlacement, McalChunk, MclyChunk, MclyFlags, MclyLayer, ParsedAdt,
    WmoPlacement, parse_adt,
};
use wow_wdt::chunks::MphdFlags;
use wow_wdt::chunks::MwmoChunk;
use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtWriter};

use crate::support::{Fixture, FixtureFile};

/// WDT selection obeys archive precedence and preserves exact grid/path data.
#[test]
fn terrain_map_loads_patched_stock_manifest() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let base_wdt = terrain_wdt(None, false)?;
    let patch_wdt = terrain_wdt(Some((32, 31, 4395)), false)?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &base_wdt,
        },
        FixtureFile {
            archive: "patch-2.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &patch_wdt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;

    let terrain = TerrainMap::load(&mut store, definition)?;
    let index = TerrainTileIndex::new(32, 31).ok_or("fixture tile is invalid")?;
    assert_eq!(terrain.map_id(), 571);
    assert_eq!(terrain.directory(), "Northrend");
    assert_eq!(terrain.source().relative_path(), Path::new("patch-2.MPQ"));
    assert_eq!(terrain.existing_tiles().count(), 1);
    assert!(terrain.tile(index).exists());
    assert_eq!(terrain.tile(index).area_id(), 4395);
    assert_eq!(
        terrain.adt_path(index)?.as_str(),
        "WORLD\\MAPS\\NORTHREND\\NORTHREND_32_31.ADT"
    );
    assert_eq!(
        terrain.wdl_path()?.as_str(),
        "WORLD\\MAPS\\NORTHREND\\NORTHREND.WDL"
    );
    assert_eq!(
        TerrainMap::tile_at_world_position(0.0, 0.0),
        TerrainTileIndex::new(32, 32).ok_or("center tile is invalid")?
    );
    assert_eq!(
        TerrainMap::tile_at_world_position(1_000.0, 5_800.0),
        TerrainTileIndex::new(30, 21).ok_or("fixture world tile is invalid")?
    );
    Ok(())
}

/// Later or invented WDT chunks fail instead of being silently skipped.
#[test]
fn terrain_map_rejects_non_build_12340_chunks() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let mut wdt = terrain_wdt(None, false)?;
    wdt.extend_from_slice(b"DIAM");
    wdt.extend_from_slice(&0_u32.to_le_bytes());
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;

    assert!(matches!(
        TerrainMap::load(&mut store, definition),
        Err(AssetError::TerrainDecode { message, .. })
            if message.contains("unknown chunk")
    ));
    Ok(())
}

/// A monolithic WotLK ADT becomes an owned row-major render-data tile.
#[test]
fn terrain_tile_decodes_stock_chunk_geometry() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let wdt = terrain_wdt(Some((32, 32, 1)), true)?;
    let adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .add_texture_flags(MtxfChunk { flags: vec![1] })
        .add_model("world/fixture/tree.m2")
        .add_wmo("world/fixture/house.wmo")
        .add_doodad_placement(DoodadPlacement {
            name_id: 0,
            unique_id: 7,
            position: [16_000.0, 250.0, 12_000.0],
            rotation: [10.0, 20.0, 30.0],
            scale: 1_024,
            flags: 0,
        })
        .add_wmo_placement(WmoPlacement {
            name_id: 0,
            unique_id: 8,
            position: [16_100.0, 300.0, 12_100.0],
            rotation: [15.0, 25.0, 35.0],
            extents_min: [16_000.0, 200.0, 12_000.0],
            extents_max: [16_200.0, 400.0, 12_200.0],
            flags: 0,
            doodad_set: 1,
            name_set: 2,
            scale: 1_024,
        })
        .build()?
        .to_bytes()?;
    let adt = asymmetric_terrain_adt(adt)?;
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;
    let terrain = TerrainMap::load(&mut store, definition)?;
    let index = TerrainTileIndex::new(32, 32).ok_or("fixture tile is invalid")?;

    let tile = terrain.load_tile(&mut store, index)?;
    assert_eq!(tile.index(), index);
    assert_eq!(tile.chunks().len(), 256);
    assert_eq!(tile.textures().len(), 1);
    assert_eq!(tile.textures()[0].as_str(), "TILESET\\FIXTURE\\GRASS.BLP");
    assert_eq!(tile.texture_flags(), Some([1_u32].as_slice()));
    assert_eq!(tile.chunks()[0].index().x(), 0);
    assert_eq!(tile.chunks()[0].index().y(), 0);
    assert_eq!(tile.chunks()[0].heights().len(), 145);
    assert_eq!(tile.chunks()[0].normals().len(), 145);
    assert_eq!(tile.chunks()[0].position(), [1_000.0, 6_000.0, 200.0]);
    assert_eq!(tile.chunks()[0].normals()[0], [1.0, 0.0, 0.0]);
    let alpha = tile.chunks()[0]
        .alpha_map()
        .ok_or("fixture blend layer has no decoded alpha map")?
        .rgba();
    assert_eq!(&alpha[0..8], &[0, 42, 0, 255, 1, 42, 0, 255]);
    assert_eq!(&alpha[alpha.len() - 4..], &[255, 42, 0, 255]);
    assert_eq!(tile.chunks()[255].index().x(), 15);
    assert_eq!(tile.chunks()[255].index().y(), 15);
    assert_eq!(tile.doodads().len(), 1);
    assert_eq!(tile.world_models().len(), 1);
    assert_position(tile.doodads()[0].position(), [1_066.666, 5_066.666, 250.0]);
    assert_position(
        tile.world_models()[0].position(),
        [966.666, 4_966.666, 300.0],
    );
    let [minimum, maximum] = tile.world_models()[0].bounds();
    assert_position(minimum, [866.666, 4_866.666, 200.0]);
    assert_position(maximum, [1_066.666, 5_066.666, 400.0]);
    Ok(())
}

/// MH2O layers retain stock planar arrays, masks, and null-vertex ocean rules.
#[test]
fn terrain_tile_decodes_stock_liquid_layers() -> Result<(), Box<dyn Error>> {
    let map_table = map_table();
    let wdt = terrain_wdt(Some((32, 32, 1)), false)?;
    let adt = append_liquid_fixture(
        AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("tileset/fixture/waterbed.blp")
            .build()?
            .to_bytes()?,
    );
    let fixture = Fixture::new(&[
        FixtureFile {
            archive: "common.MPQ",
            path: "DBFilesClient\\Map.dbc",
            bytes: &map_table,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            archive: "common.MPQ",
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let root = ClientDataRoot::new(fixture.data_root())?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let definition = maps.map(571).ok_or("Northrend map is absent")?;
    let terrain = TerrainMap::load(&mut store, definition)?;
    let index = TerrainTileIndex::new(32, 32).ok_or("fixture tile is invalid")?;

    let tile = terrain.load_tile(&mut store, index)?;
    assert!(tile.has_liquid_table());
    let liquids = tile.liquids().ok_or("fixture MH2O was not retained")?;
    assert_eq!(liquids.layer_count(), 2);
    let chunk = &liquids.chunks()[0];
    assert_eq!(chunk.fishable_mask(), 0x5);
    assert_eq!(chunk.deep_mask(), 0x2);
    let sloped = &chunk.layers()[0];
    assert_eq!(sloped.liquid_type(), 2);
    assert_eq!(sloped.vertex_format(), 3);
    assert_eq!(sloped.minimum_height(), 10.0);
    assert_eq!(sloped.maximum_height(), 15.0);
    assert_eq!((sloped.x_offset(), sloped.y_offset()), (1, 2));
    assert_eq!((sloped.width(), sloped.height()), (2, 1));
    assert_eq!(sloped.exists(), &[1, 0]);
    assert_eq!(sloped.heights(), &[10.0, 11.0, 12.0, 13.0, 14.0, 15.0]);
    assert_eq!(
        sloped.texture_coordinates(),
        Some(
            [
                [100, 200],
                [101, 201],
                [102, 202],
                [103, 203],
                [104, 204],
                [105, 205],
            ]
            .as_slice()
        )
    );
    assert_eq!(sloped.depths(), &[10, 20, 30, 40, 50, 60]);

    let ocean = &chunk.layers()[1];
    assert_eq!(ocean.liquid_type(), 1);
    assert_eq!(ocean.vertex_format(), 2);
    assert_eq!(ocean.heights(), &[0.0; 4]);
    assert_eq!(ocean.depths(), &[u8::MAX; 4]);
    assert_eq!(ocean.texture_coordinates(), None);
    Ok(())
}

fn assert_position(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
    }
}

/// Gives the first generated chunk distinct values in every stored axis.
///
/// The dependency's fixture writer otherwise emits symmetric zero origins,
/// which cannot detect a transposed stock coordinate basis.
fn asymmetric_terrain_adt(bytes: Vec<u8>) -> Result<Vec<u8>, Box<dyn Error>> {
    let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(bytes))? else {
        return Err("fixture did not decode as a root ADT".into());
    };
    // wow-adt 0.7 reads MTXF beyond its declared chunk extent. Restore the
    // fixture's authored word before its builder serializes the edited ADT.
    root.texture_flags = Some(MtxfChunk { flags: vec![1] });
    let first = root
        .mcnk_chunks
        .first_mut()
        .ok_or("fixture root ADT contains no MCNK chunks")?;
    // Stored `[zpos, xpos, ypos]` becomes ECS `[X, Y, Z]`.
    first.header.position = [6_000.0, 1_000.0, 200.0];
    let first_normal = first
        .normals
        .as_mut()
        .and_then(|normals| normals.normals.first_mut())
        .ok_or("fixture MCNK contains no normal")?;
    first_normal.x = 127;
    first_normal.y = 0;
    first_normal.z = 0;
    first.header.n_layers = 3;
    first.header.flags.value |= 0x8000;
    first.layers = Some(MclyChunk {
        layers: vec![
            MclyLayer::default(),
            MclyLayer {
                texture_id: 0,
                flags: MclyFlags { value: 0x100 },
                offset_in_mcal: 0,
                effect_id: 0,
            },
            MclyLayer {
                texture_id: 0,
                flags: MclyFlags { value: 0x300 },
                offset_in_mcal: 4_096,
                effect_id: 0,
            },
        ],
    });
    let mut alpha = (0_u8..=u8::MAX).cycle().take(64 * 64).collect::<Vec<_>>();
    for _run in 0..32 {
        alpha.extend_from_slice(&[0x80 | 127, 42]);
    }
    alpha.extend_from_slice(&[0x80 | 32, 42]);
    first.alpha = Some(McalChunk::new(alpha));
    Ok(BuiltAdt::from_root_adt(*root, None).to_bytes()?)
}

/// Appends one direct build-12340 MH2O fixture.
///
/// This deliberately avoids the dependency's higher-level vertex model: the
/// client stores each optional component as a complete planar array.
fn append_liquid_fixture(mut adt: Vec<u8>) -> Vec<u8> {
    const HEADER_BYTES: usize = 256 * 12;
    const INSTANCE_OFFSET: usize = HEADER_BYTES;
    const ATTRIBUTES_OFFSET: usize = INSTANCE_OFFSET + 2 * 24;
    const EXISTS_OFFSET: usize = ATTRIBUTES_OFFSET + 16;
    const VERTEX_OFFSET: usize = EXISTS_OFFSET + 1;
    const VERTEX_COUNT: usize = 6;
    let mut payload = vec![0_u8; VERTEX_OFFSET + VERTEX_COUNT * 9];

    set_u32(&mut payload, 0, INSTANCE_OFFSET as u32);
    set_u32(&mut payload, 4, 2);
    set_u32(&mut payload, 8, ATTRIBUTES_OFFSET as u32);
    set_u16(&mut payload, INSTANCE_OFFSET, 2);
    set_u16(&mut payload, INSTANCE_OFFSET + 2, 3);
    set_f32(&mut payload, INSTANCE_OFFSET + 4, 10.0);
    set_f32(&mut payload, INSTANCE_OFFSET + 8, 15.0);
    payload[INSTANCE_OFFSET + 12..INSTANCE_OFFSET + 16].copy_from_slice(&[1, 2, 2, 1]);
    set_u32(&mut payload, INSTANCE_OFFSET + 16, EXISTS_OFFSET as u32);
    set_u32(&mut payload, INSTANCE_OFFSET + 20, VERTEX_OFFSET as u32);

    let ocean = INSTANCE_OFFSET + 24;
    set_u16(&mut payload, ocean, 1);
    set_u16(&mut payload, ocean + 2, 0);
    set_f32(&mut payload, ocean + 4, 300.0);
    set_f32(&mut payload, ocean + 8, 300.0);
    payload[ocean + 12..ocean + 16].copy_from_slice(&[0, 0, 1, 1]);

    set_u64(&mut payload, ATTRIBUTES_OFFSET, 0x5);
    set_u64(&mut payload, ATTRIBUTES_OFFSET + 8, 0x2);
    payload[EXISTS_OFFSET] = 0x1;
    for index in 0..VERTEX_COUNT {
        set_f32(&mut payload, VERTEX_OFFSET + index * 4, 10.0 + index as f32);
    }
    let uv_offset = VERTEX_OFFSET + VERTEX_COUNT * 4;
    for index in 0..VERTEX_COUNT {
        set_u16(&mut payload, uv_offset + index * 4, 100 + index as u16);
        set_u16(&mut payload, uv_offset + index * 4 + 2, 200 + index as u16);
    }
    let depth_offset = uv_offset + VERTEX_COUNT * 4;
    payload[depth_offset..depth_offset + VERTEX_COUNT].copy_from_slice(&[10, 20, 30, 40, 50, 60]);

    adt.extend_from_slice(b"O2HM");
    adt.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    adt.extend_from_slice(&payload);
    adt
}

fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn set_f32(bytes: &mut [u8], offset: usize, value: f32) {
    set_u32(bytes, offset, value.to_bits());
}

fn terrain_wdt(
    tile: Option<(usize, usize, u32)>,
    big_alpha: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mwmo = Some(MwmoChunk::new());
    if big_alpha {
        wdt.mphd.flags |= MphdFlags::ADT_HAS_BIG_ALPHA;
    }
    if let Some((x, y, area_id)) = tile {
        let entry = wdt
            .main
            .get_mut(x, y)
            .ok_or("fixture WDT tile is invalid")?;
        entry.set_has_adt(true);
        entry.area_id = area_id;
    }
    let mut bytes = Vec::new();
    WdtWriter::new(&mut bytes).write(&wdt)?;
    Ok(bytes)
}

fn map_table() -> Vec<u8> {
    let mut strings = vec![0_u8];
    let directory = append_string(&mut strings, "Northrend");
    let name = append_string(&mut strings, "Northrend");
    let mut fields = [0_u32; 66];
    fields[0] = 571;
    fields[1] = directory;
    fields[5] = name;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[63] = 2;
    create_wdbc(1, 66, &fields, &strings)
}

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

fn append_string(block: &mut Vec<u8>, value: &str) -> u32 {
    let offset = block.len() as u32;
    block.extend_from_slice(value.as_bytes());
    block.push(0);
    offset
}

/// Packed MCSH bits expand to stock opacity and honor the shared edge flag.
#[test]
fn terrain_shadow_map_expands_stock_bits_and_edges() {
    let mut packed = [0_u8; 512];
    packed[0] = 0b0000_0001;
    let penultimate_corner = 62 * 64 + 62;
    packed[penultimate_corner / 8] |= 1 << (penultimate_corner % 8);
    let final_corner = 63 * 64 + 63;
    packed[final_corner / 8] |= 1 << (final_corner % 8);

    let fixed = TerrainShadowMap::from_packed(&packed, false);
    assert_eq!(fixed.opacity()[0], 85);
    assert_eq!(fixed.opacity()[62 * 64 + 62], 85);
    assert_eq!(fixed.opacity()[62 * 64 + 63], 85);
    assert_eq!(fixed.opacity()[63 * 64 + 62], 85);
    assert_eq!(fixed.opacity()[63 * 64 + 63], 85);

    packed[penultimate_corner / 8] &= !(1 << (penultimate_corner % 8));
    let authored = TerrainShadowMap::from_packed(&packed, true);
    assert_eq!(authored.opacity()[62 * 64 + 62], 0);
    assert_eq!(authored.opacity()[63 * 64 + 63], 85);
}

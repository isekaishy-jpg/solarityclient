//! Original liquid candidate selection and complete native collision planes.

use std::{error::Error, io::Cursor, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale, MapCatalog,
    TerrainChunkIndex, TerrainMap, TerrainTileIndex,
};
use solarity_systems::{
    MovementCollisionBounds, MovementCollisionTriangle, PlacedWorldModelCollision,
    append_terrain_liquid_movement,
};
use wow_adt::{ParsedAdt, builder::BuiltAdt, parse_adt};

use super::super::{
    collision, movement_collection,
    support::{Fixture, FixtureFile},
    world_model_registration::chunk_data_mut,
};
use super::take_words;

#[test]
fn liquid_movement_faces_match_original_grids_masks_and_planes() -> Result<(), Box<dyn Error>> {
    let mut bytes = include_bytes!("../../fixtures/movement_liquid_geometry.bin").as_slice();
    let mut count = 0;
    while !bytes.is_empty() {
        let header = take_words::<31>(&mut bytes);
        let width = header[1] as usize;
        let height = header[2] as usize;
        let heights = (0..(width + 1) * (height + 1))
            .map(|_| f32::from_bits(take_words::<1>(&mut bytes)[0]))
            .collect::<Vec<_>>();
        let (mask, tail) = bytes.split_at(width * height);
        bytes = tail;
        let expected = (0..header[30])
            .map(|_| take_words::<13>(&mut bytes))
            .collect::<Vec<_>>();
        let values = header.map(f32::from_bits);
        let corner = Vec3::from_slice(&values[5..8]);
        let bounds = MovementCollisionBounds::new(
            Vec3::from_slice(&values[8..11]),
            Vec3::from_slice(&values[11..14]),
        )?;
        let mut output = Vec::new();
        if header[0] == 0 {
            terrain(&header, &heights, mask, bounds, &mut output)?;
        } else {
            world_model(&header, &heights, mask, corner, bounds, &mut output)?;
        }
        let actual = output
            .iter()
            .map(|face| {
                let normal = face.normal();
                let vertices = face.vertices();
                [
                    normal.x,
                    normal.y,
                    normal.z,
                    -normal.as_dvec3().dot(vertices[0].as_dvec3()) as f32,
                    vertices[0].x,
                    vertices[0].y,
                    vertices[0].z,
                    vertices[1].x,
                    vertices[1].y,
                    vertices[1].z,
                    vertices[2].x,
                    vertices[2].y,
                    vertices[2].z,
                ]
                .map(f32::to_bits)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual.len(),
            expected.len(),
            "case {count} kind {} face count",
            header[0]
        );
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert_eq!(
                &actual[4..],
                &expected[4..],
                "case {count}, face {index} vertices"
            );
            // RSQRTSS estimates vary between processors and Unicorn. Preserve
            // native winding/direction and the ISA estimate's error envelope.
            let actual_normal =
                Vec3::from_array(std::array::from_fn(|i| f32::from_bits(actual[i])));
            let expected_normal =
                Vec3::from_array(std::array::from_fn(|i| f32::from_bits(expected[i])));
            assert!(
                (actual_normal.normalize_or_zero() - expected_normal.normalize_or_zero()).length()
                    < 0.000_001,
                "case {count}, face {index} normal direction"
            );
            assert!(
                (actual_normal - expected_normal).length() < 0.0004,
                "case {count}, face {index} normal estimate"
            );
        }
        count += 1;
    }
    assert_eq!(count, 128);
    Ok(())
}

/// Encodes real MH2O data so the collector sees decoder-normalized masks/heights.
fn terrain(
    header: &[u32; 31],
    heights: &[f32],
    mask: &[u8],
    bounds: MovementCollisionBounds,
    output: &mut Vec<MovementCollisionTriangle>,
) -> Result<(), Box<dyn Error>> {
    let bytes = movement_collection::terrain_tile(32, 32)?;
    let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(bytes))? else {
        return Err("fixture root ADT".into());
    };
    root.mcnk_chunks[0].header.position = [0., 0., 0.];
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    let mut adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    let mut liquid = vec![0u8; 3104];
    liquid[..4].copy_from_slice(&3072u32.to_le_bytes());
    liquid[4..8].copy_from_slice(&1u32.to_le_bytes());
    liquid[3072..3074].copy_from_slice(&1u16.to_le_bytes());
    let minimum = heights.iter().copied().fold(f32::INFINITY, f32::min);
    let maximum = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    liquid[3076..3080].copy_from_slice(&minimum.to_le_bytes());
    liquid[3080..3084].copy_from_slice(&maximum.to_le_bytes());
    liquid[3084..3088].copy_from_slice(&[
        header[3] as u8,
        header[4] as u8,
        header[1] as u8,
        header[2] as u8,
    ]);
    liquid[3088..3092].copy_from_slice(&3096u32.to_le_bytes());
    liquid[3092..3096].copy_from_slice(&3104u32.to_le_bytes());
    let exists = mask.iter().enumerate().fold(0u64, |mask, (index, &value)| {
        mask | (u64::from(value != 15) << index)
    });
    liquid[3096..3104].copy_from_slice(&exists.to_le_bytes());
    for height in heights {
        liquid.extend(height.to_le_bytes());
    }
    liquid.extend(std::iter::repeat_n(255, heights.len()));
    adt.extend(b"O2HM");
    adt.extend((liquid.len() as u32).to_le_bytes());
    adt.extend(liquid);
    let map = movement_collection::map_table();
    let wdt = movement_collection::world_table()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "DBFilesClient\\Map.dbc",
            bytes: &map,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, catalog.map(571).ok_or("fixture map")?)?;
    let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
    append_terrain_liquid_movement(
        &tile,
        TerrainChunkIndex::new(0, 0).ok_or("chunk")?,
        bounds,
        output,
    )?;
    Ok(())
}

/// Places decoded MLIQ under the same retained native matrix as the capture.
fn world_model(
    header: &[u32; 31],
    heights: &[f32],
    mask: &[u8],
    corner: Vec3,
    bounds: MovementCollisionBounds,
    output: &mut Vec<MovementCollisionTriangle>,
) -> Result<(), Box<dyn Error>> {
    let mut root = collision::root_fixture();
    let mut group = collision::group_fixture(8);
    for (bytes, magic, offset) in [(&mut root, *b"DHOM", 36), (&mut group, *b"PGOM", 12)] {
        let data = chunk_data_mut(bytes, magic);
        for (index, value) in [-1000f32, -1000., -1000., 1000., 1000., 1000.]
            .into_iter()
            .enumerate()
        {
            data[offset + index * 4..offset + index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let info = chunk_data_mut(&mut root, *b"IGOM");
    for (index, value) in [-1000f32, -1000., -1000., 1000., 1000., 1000.]
        .into_iter()
        .enumerate()
    {
        info[4 + index * 4..8 + index * 4].copy_from_slice(&value.to_le_bytes());
    }
    let data = chunk_data_mut(&mut group, *b"PGOM");
    data[8..12].copy_from_slice(&0x1000u32.to_le_bytes());
    data[52..56].copy_from_slice(&21u32.to_le_bytes());
    let mut grid = Vec::new();
    for word in [header[1] + 1, header[2] + 1, header[1], header[2]] {
        grid.extend(word.to_le_bytes());
    }
    for value in corner.to_array() {
        grid.extend(value.to_le_bytes());
    }
    grid.extend(0u16.to_le_bytes());
    for height in heights {
        grid.extend(0u32.to_le_bytes());
        grid.extend(height.to_le_bytes());
    }
    grid.extend(mask);
    let size = u32::from_le_bytes(group[16..20].try_into()?);
    group.extend(b"QILM");
    group.extend((grid.len() as u32).to_le_bytes());
    group.extend(&grid);
    group[16..20].copy_from_slice(&(size + 8 + grid.len() as u32).to_le_bytes());
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "World\\Liquid.wmo",
            bytes: &root,
        },
        FixtureFile {
            path: "World\\Liquid_000.wmo",
            bytes: &group,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Liquid.wmo")?,
    )?);
    let transform = Mat4::from_cols_array(&std::array::from_fn(|i| f32::from_bits(header[14 + i])));
    let placement = PlacedWorldModelCollision::prepare_transform(model, transform)?;
    placement.append_liquid_movement(bounds, output)?;
    Ok(())
}

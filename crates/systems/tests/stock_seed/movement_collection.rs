//! Original-executable evidence for resident movement triangle collection.

use std::{error::Error, io::Cursor, str::SplitWhitespace, sync::Arc};

use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, DecodedWorldModel,
    Locale, MapCatalog, TerrainMap, TerrainTileIndex,
};
use solarity_systems::{
    MovementBspCacheMode, MovementCollectionError, MovementCollisionBounds,
    MovementCollisionTriangle, PlacedM2Collision, PlacedWorldModelCollision, TerrainCollisionMesh,
};
use wow_adt::{
    AdtVersion, ParsedAdt,
    builder::{AdtBuilder, BuiltAdt},
    chunks::MtxfChunk,
    parse_adt,
};
use wow_wdt::{WdtFile, WdtWriter, chunks::MwmoChunk, version::WowVersion};

use super::support::{Fixture, FixtureFile};

const VERTICES: [[f32; 3]; 5] = [
    [-2., -2., 0.],
    [2., -2., 0.],
    [-2., 2., 0.],
    [2., 2., 1.],
    [0., 0., 3.],
];
const FACES: [[u16; 3]; 4] = [[0, 1, 2], [1, 3, 2], [1, 4, 3], [0, 0, 0]];

/// Invalid residency queries cannot become an empty successful collision result.
#[test]
fn movement_collection_rejects_invalid_world_queries() -> Result<(), Box<dyn Error>> {
    for (minimum, maximum) in [(Vec3::splat(f32::NAN), Vec3::ONE), (Vec3::ONE, Vec3::ZERO)] {
        assert_eq!(
            MovementCollisionBounds::new(minimum, maximum),
            Err(MovementCollectionError::InvalidBounds)
        );
    }
    for position in [
        Vec3::splat(f32::MAX),
        Vec3::splat(-f32::MAX),
        Vec3::new(-17_066.666, 0., 0.),
    ] {
        assert_eq!(
            MovementCollisionBounds::new(position, position)?
                .terrain_chunks()
                .err(),
            Some(MovementCollectionError::OutsideTerrainMap)
        );
    }
    Ok(())
}

/// Original world traversal preserves terrain holes, MOPY filters, and face ties.
#[test]
fn movement_collection_matches_original_world_queries() -> Result<(), Box<dyn Error>> {
    let map = map_table();
    let wdt = world_table()?;
    let first = terrain_tile(32, 32)?;
    let second = terrain_tile(21, 30)?;
    let model_bytes = m2_fixture()?;
    let skin = super::collision::skin_fixture()?;
    let root = world_model_root();
    let mut selection_root = world_model_root();
    // MOGI begins after MVER (12), MOHD (72), and its chunk header (8).
    set_vector(&mut selection_root, 96, [1., 1., -1.]);
    let group = world_model_group();
    let cache_groups = (0..4)
        .map(world_model_cache_limit_group)
        .collect::<Vec<_>>();
    let cache_paths = [
        ("World\\Cache0.wmo", "World\\Cache0_000.wmo"),
        ("World\\Cache1.wmo", "World\\Cache1_000.wmo"),
        ("World\\Cache2.wmo", "World\\Cache2_000.wmo"),
        ("World\\Cache3.wmo", "World\\Cache3_000.wmo"),
    ];
    let mut files = vec![
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
            bytes: &first,
        },
        FixtureFile {
            path: "World\\Maps\\Northrend\\Northrend_21_30.adt",
            bytes: &second,
        },
        FixtureFile {
            path: "World\\Collection.m2",
            bytes: &model_bytes,
        },
        FixtureFile {
            path: "World\\Collection00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "World\\Collection.wmo",
            bytes: &root,
        },
        FixtureFile {
            path: "World\\Collection_000.wmo",
            bytes: &group,
        },
        FixtureFile {
            path: "World\\Selection.wmo",
            bytes: &selection_root,
        },
        FixtureFile {
            path: "World\\Selection_000.wmo",
            bytes: &group,
        },
    ];
    for (index, &(root_path, group_path)) in cache_paths.iter().enumerate() {
        files.push(FixtureFile {
            path: root_path,
            bytes: &root,
        });
        files.push(FixtureFile {
            path: group_path,
            bytes: &cache_groups[index],
        });
    }
    let fixture = Fixture::new(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let catalog = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, catalog.map(571).ok_or("missing fixture map")?)?;
    let terrain = [(32, 32), (21, 30)]
        .map(|(x, y)| -> Result<_, Box<dyn Error>> {
            let index = TerrainTileIndex::new(x, y).ok_or("bad fixture index")?;
            Ok(TerrainCollisionMesh::prepare(
                &map.load_tile(&mut store, index)?,
            )?)
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let model = Arc::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("World\\Collection.m2")?,
    )?);
    let world_model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Collection.wmo")?,
    )?);
    let selection_model = Arc::new(DecodedWorldModel::load(
        &mut store,
        &AssetPath::new("World\\Selection.wmo")?,
    )?);
    let cache_models = cache_paths
        .iter()
        .map(|(path, _)| -> Result<_, Box<dyn Error>> {
            Ok(Arc::new(DecodedWorldModel::load(
                &mut store,
                &AssetPath::new(path)?,
            )?))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut output = Vec::new();
    let mut count = 0;
    for line in include_str!("../fixtures/movement-collection-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let kind = words.next().ok_or("missing kind")?;
        let name = words.next().ok_or("missing case")?;
        output.clear();
        if kind == "T" {
            let x = words.next().ok_or("tile x")?.parse::<u8>()?;
            let y = words.next().ok_or("tile y")?.parse::<u8>()?;
            let bounds = read_bounds(&mut words)?;
            let visited = words
                .next()
                .ok_or("missing visited count")?
                .parse::<usize>()?;
            let addresses = bounds.terrain_chunks()?.collect::<Vec<_>>();
            assert_eq!(
                addresses.len(),
                visited,
                "{name}: resident chunk visitation count"
            );
            for (tile, chunk) in &addresses {
                for actual in [tile.x(), tile.y(), chunk.x(), chunk.y()] {
                    assert_eq!(
                        actual,
                        words.next().ok_or("missing address")?.parse::<u8>()?,
                        "{name}: ordered chunk address"
                    );
                }
            }
            let mesh = &terrain[usize::from((x, y) != (32, 32))];
            for (tile, chunk) in addresses {
                // Native fixture WDT declares only this one resident tile.
                if (tile.x(), tile.y()) == (x, y) {
                    mesh.append_movement_chunk(chunk, bounds, &mut output)?;
                }
            }
        } else if kind == "G" {
            let bounds = read_bounds(&mut words)?;
            PlacedWorldModelCollision::prepare_transforms(
                Arc::clone(&selection_model),
                Mat4::IDENTITY,
                Mat4::IDENTITY,
            )?
            .append_movement(bounds, MovementBspCacheMode::Enabled, &mut output)?;
        } else if kind == "C" {
            let variant = words.next().ok_or("cache variant")?.parse::<usize>()?;
            let bounds = read_bounds(&mut words)?;
            PlacedWorldModelCollision::prepare_transforms(
                Arc::clone(&cache_models[variant]),
                Mat4::IDENTITY,
                Mat4::IDENTITY,
            )?
            .append_movement(bounds, MovementBspCacheMode::Enabled, &mut output)?;
        } else {
            let mut values = [0.; 16];
            for value in &mut values {
                *value = float(&mut words)?;
            }
            let transform = Mat4::from_cols_array(&values);
            let inverse = if kind == "W" || kind == "U" {
                for value in &mut values {
                    *value = float(&mut words)?;
                }
                Mat4::from_cols_array(&values)
            } else {
                Mat4::IDENTITY
            };
            let bounds = read_bounds(&mut words)?;
            if kind == "M" {
                PlacedM2Collision::prepare_transform(Arc::clone(&model), transform)?
                    .append_movement(bounds, &mut output)?;
            } else {
                PlacedWorldModelCollision::prepare_transforms(
                    Arc::clone(&world_model),
                    transform,
                    inverse,
                )?
                .append_movement(
                    bounds,
                    if kind == "W" {
                        MovementBspCacheMode::Enabled
                    } else {
                        MovementBspCacheMode::Disabled
                    },
                    &mut output,
                )?;
            }
        }
        assert_triangles(name, &output, &mut words)?;
        assert!(words.next().is_none(), "{name}: trailing fixture data");
        count += 1;
    }
    assert_eq!(count, 2247);
    Ok(())
}

/// Checks candidate identity/order independently of CPU estimate precision.
fn assert_triangles(
    name: &str,
    actual: &[MovementCollisionTriangle],
    words: &mut SplitWhitespace<'_>,
) -> Result<(), Box<dyn Error>> {
    let count = words.next().ok_or("triangle count")?.parse::<usize>()?;
    assert_eq!(actual.len(), count, "{name}: ordered candidate count");
    for (index, triangle) in actual.iter().enumerate() {
        let normal = vector(words)?;
        assert!(
            (triangle.normal().normalize_or_zero() - normal.normalize_or_zero()).length()
                <= 0.000001,
            "{name} face {index}: facing direction differs"
        );
        // Unicorn implements RSQRTSS with an exact reciprocal square root.
        // Real x86 uses its hardware estimate (relative error <= 1.5 * 2^-12).
        // The expected face order and vertices are still checked independently.
        assert!(
            (triangle.normal() - normal).length() <= 0.0004 * normal.length().max(1.0),
            "{name} face {index}: {:?} != {normal:?}",
            triangle.normal()
        );
        for vertex in triangle.vertices() {
            let expected = vector(words)?;
            assert!(
                (*vertex - expected).abs().max_element() <= 0.00001,
                "{name} face {index}: {vertex:?} != {expected:?}"
            );
        }
    }
    Ok(())
}

fn float(words: &mut SplitWhitespace<'_>) -> Result<f32, Box<dyn Error>> {
    Ok(f32::from_bits(u32::from_str_radix(
        words.next().ok_or("missing float")?,
        16,
    )?))
}
fn vector(words: &mut SplitWhitespace<'_>) -> Result<Vec3, Box<dyn Error>> {
    Ok(Vec3::new(float(words)?, float(words)?, float(words)?))
}
fn read_bounds(words: &mut SplitWhitespace<'_>) -> Result<MovementCollisionBounds, Box<dyn Error>> {
    Ok(MovementCollisionBounds::new(
        vector(words)?,
        vector(words)?,
    )?)
}

/// Authors distinct MCNK origins, holes, and height values across both axes.
fn terrain_tile(tile_x: u8, tile_y: u8) -> Result<Vec<u8>, Box<dyn Error>> {
    let bytes = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("tileset/fixture/grass.blp")
        .build()?
        .to_bytes()?;
    let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(bytes))? else {
        return Err("not root ADT".into());
    };
    root.texture_flags = Some(MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        let x = chunk.header.index_x;
        let y = chunk.header.index_y;
        let i = y * 16 + x;
        chunk.header.position = [
            17_066.666_f32 - (u32::from(tile_y) * 16 + y) as f32 * 33.333_332,
            17_066.666_f32 - (u32::from(tile_x) * 16 + x) as f32 * 33.333_332,
            30.0 + i as f32 * 0.25,
        ];
        chunk.header.holes_low_res = if i % 3 == 0 { 0x1081 } else { 0 };
        let heights = &mut chunk.heights.as_mut().ok_or("missing MCVT")?.heights;
        for (v, height) in heights.iter_mut().enumerate() {
            *height = (((v as u32 * 7 + i * 3) % 19) as i32 - 9) as f32 * 0.125;
        }
    }
    Ok(BuiltAdt::from_root_adt(*root, None).to_bytes()?)
}

/// Declares the two test ADTs in an otherwise empty stock WDT.
fn world_table() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut wdt = WdtFile::new(WowVersion::WotLK);
    wdt.mwmo = Some(MwmoChunk::new());
    for (x, y) in [(32, 32), (21, 30)] {
        wdt.main.get_mut(x, y).ok_or("bad tile")?.set_has_adt(true);
    }
    let mut bytes = Vec::new();
    WdtWriter::new(&mut bytes).write(&wdt)?;
    Ok(bytes)
}

/// Supplies the exact 66-field Map.dbc record needed for archive tile decoding.
fn map_table() -> Vec<u8> {
    let strings = b"\0Northrend\0";
    let mut fields = [0u32; 66];
    fields[0] = 571;
    fields[1] = 1;
    fields[5] = 1;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[63] = 2;
    let mut bytes = b"WDBC".to_vec();
    for v in [1, 66, 264, strings.len() as u32].into_iter().chain(fields) {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Extends a valid M2/SKIN pair with dedicated, deliberately non-unit normals.
fn m2_fixture() -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = super::collision::m2_collision_fixture()?;
    let indices = bytes.len() as u32;
    for face in FACES {
        for index in face {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
    }
    let vertices = bytes.len() as u32;
    for vertex in VERTICES {
        for v in vertex {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    let normals = bytes.len() as u32;
    for normal in [[0f32, 0., 1.], [0., 0., 0.8], [-1., 0., 0.], [0., 0., 0.]] {
        for v in normal {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    for (offset, value) in [
        (0xd8, 12),
        (0xdc, indices),
        (0xe0, 5),
        (0xe4, vertices),
        (0xe8, 4),
        (0xec, normals),
    ] {
        set_u32(&mut bytes, offset, value);
    }
    set_vector(&mut bytes, 0xbc, [-3., -3., -1.]);
    set_vector(&mut bytes, 0xc8, [3., 3., 4.]);
    Ok(bytes)
}

/// Gives the native WMO root and group the same finite authored bounds.
fn world_model_root() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17u32.to_le_bytes());
    let mut header = vec![0u8; 64];
    set_u32(&mut header, 4, 1);
    set_vector(&mut header, 36, [-3., -3., -1.]);
    set_vector(&mut header, 48, [3., 3., 4.]);
    push_chunk(&mut bytes, *b"DHOM", &header);
    let mut group = vec![0u8; 32];
    set_vector(&mut group, 4, [-3., -3., -1.]);
    set_vector(&mut group, 16, [3., 3., 4.]);
    set_u32(&mut group, 28, u32::MAX);
    push_chunk(&mut bytes, *b"IGOM", &group);
    bytes
}

/// Duplicates face references across BSP children and exercises movement filters.
fn world_model_group() -> Vec<u8> {
    world_model_group_geometry(
        &VERTICES,
        &FACES,
        &[8, 10, 4, 8],
        &[(0, 1, 2, 0, 0), (4, -1, -1, 3, 0), (4, -1, -1, 3, 3)],
        &[0, 1, 2, 2, 1, 0],
    )
}

/// Crosses both native cache admission limits with coplanar boundary geometry.
fn world_model_cache_limit_group(variant: usize) -> Vec<u8> {
    let (positions, triangles, references) = if variant < 2 {
        (
            vec![[-2., -2., 0.], [2., -2., 0.], [-2., 2., 0.]],
            vec![[0, 1, 2]],
            vec![0; 300 + variant],
        )
    } else {
        let mut positions = (0..150)
            .flat_map(|_| [[-2., -2., 0.], [2., -2., 0.], [-2., 2., 0.]])
            .collect::<Vec<_>>();
        let mut triangles = (0..150u16)
            .map(|i| [i * 3, i * 3 + 1, i * 3 + 2])
            .collect::<Vec<_>>();
        if variant == 3 {
            positions.push([-2., 2., 0.]);
            triangles.push([0, 1, 450]);
        }
        let references = (0..triangles.len() as u16).collect::<Vec<_>>();
        (positions, triangles, references)
    };
    world_model_group_geometry(
        &positions,
        &triangles,
        &vec![8; triangles.len()],
        &[(4, -1, -1, references.len() as u16, 0)],
        &references,
    )
}

/// Serializes exact MOPY/MOVI/MOBN/MOBR arrays through one stock group container.
fn world_model_group_geometry(
    positions: &[[f32; 3]],
    triangles: &[[u16; 3]],
    polygon_flags: &[u8],
    node_defs: &[(u16, i16, i16, u16, u32)],
    references: &[u16],
) -> Vec<u8> {
    let mut nested = Vec::new();
    let polygons = polygon_flags
        .iter()
        .flat_map(|&flag| [flag, 255])
        .collect::<Vec<_>>();
    push_chunk(&mut nested, *b"YPOM", &polygons);
    let mut indices = Vec::new();
    for face in triangles {
        for i in face {
            indices.extend_from_slice(&i.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"IVOM", &indices);
    let mut vertices = Vec::new();
    for vertex in positions {
        for v in vertex {
            vertices.extend_from_slice(&v.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"TVOM", &vertices);
    let mut normals = Vec::new();
    for _ in positions {
        for v in [0f32, 0., 1.] {
            normals.extend_from_slice(&v.to_le_bytes());
        }
    }
    push_chunk(&mut nested, *b"RNOM", &normals);
    let mut nodes = Vec::new();
    for &(flags, negative, positive, count, start) in node_defs {
        nodes.extend_from_slice(&flags.to_le_bytes());
        nodes.extend_from_slice(&negative.to_le_bytes());
        nodes.extend_from_slice(&positive.to_le_bytes());
        nodes.extend_from_slice(&count.to_le_bytes());
        nodes.extend_from_slice(&start.to_le_bytes());
        nodes.extend_from_slice(&0f32.to_le_bytes());
    }
    push_chunk(&mut nested, *b"NBOM", &nodes);
    let mut faces = Vec::new();
    for face in references {
        faces.extend_from_slice(&face.to_le_bytes());
    }
    push_chunk(&mut nested, *b"RBOM", &faces);
    let mut header = vec![0u8; 68];
    set_vector(&mut header, 12, [-3., -3., -1.]);
    set_vector(&mut header, 24, [3., 3., 4.]);
    header.extend_from_slice(&nested);
    let mut bytes = Vec::new();
    push_chunk(&mut bytes, *b"REVM", &17u32.to_le_bytes());
    push_chunk(&mut bytes, *b"PGOM", &header);
    bytes
}

fn set_u32(bytes: &mut [u8], offset: usize, v: u32) {
    bytes[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}
fn set_vector(bytes: &mut [u8], offset: usize, v: [f32; 3]) {
    for (i, v) in v.into_iter().enumerate() {
        set_u32(bytes, offset + 4 * i, v.to_bits());
    }
}
fn push_chunk(bytes: &mut Vec<u8>, magic: [u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
}

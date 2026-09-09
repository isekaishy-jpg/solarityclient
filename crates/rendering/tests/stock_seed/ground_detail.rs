//! Original executable placement fixtures through archive decoding and rendering.

use std::{error::Error, io::Cursor};

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, GroundEffectCatalog,
    Locale, MapCatalog, TerrainChunkIndex, TerrainMap, TerrainTileIndex,
};
use solarity_rendering::{
    GroundDetailDensity, GroundDetailMeshPlan, GroundDetailModel, TerrainDetailChunk,
};
use wow_adt::{
    AdtVersion, ParsedAdt,
    builder::{AdtBuilder, BuiltAdt},
    chunks::{MccvChunk, McshChunk, MtxfChunk, VertexColor},
    parse_adt,
};

use crate::support::{Fixture, FixtureFile};

/// Compares complete scatter output, including rejection and later RNG effects.
#[test]
fn ground_detail_matches_original_scatter() -> Result<(), Box<dyn Error>> {
    let mut lines = include_str!("../fixtures/ground-detail-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'));
    let mut mesh_lines = include_str!("../fixtures/ground-detail-mesh-native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'));
    while let Some(line) = lines.next() {
        let fields = line.split_ascii_whitespace().collect::<Vec<_>>();
        assert_eq!(fields[0], "case");
        let seed: u32 = fields[1].parse()?;
        let density = GroundDetailDensity::new(fields[2].parse()?).ok_or("invalid density")?;
        let effect_density: u32 = fields[3].parse()?;
        let holes: u16 = fields[4].parse()?;
        let stencil: u64 = fields[5].parse()?;
        let slope: f64 = fields[6].parse()?;
        let colors = fields[7] == "1";
        let shadow = fields[8] == "1";
        let count: usize = fields[9].parse()?;
        let index = TerrainChunkIndex::new((seed & 15) as u8, ((seed >> 16) & 15) as u8)
            .ok_or("invalid chunk")?;
        let base = AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("fixture/grass.blp")
            .build()?
            .to_bytes()?;
        let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(base))? else {
            return Err("root ADT required".into());
        };
        root.texture_flags = Some(MtxfChunk { flags: vec![0] });
        let chunk = &mut root.mcnk_chunks[usize::from(index.y()) * 16 + usize::from(index.x())];
        chunk.header.holes_low_res = holes;
        chunk.header.unknown_8bytes = stencil.to_le_bytes();
        chunk.layers.as_mut().ok_or("layers missing")?.layers[0].effect_id = 1;
        let heights = &mut chunk.heights.as_mut().ok_or("heights missing")?.heights;
        for row in 0..17 {
            for column in 0..if row % 2 == 0 { 9 } else { 8 } {
                let index = (row / 2) * 17 + if row % 2 == 0 { 0 } else { 9 } + column;
                let x = -(row as f64 / 2.0) * (25.0 / 6.0);
                let y = -(column as f64 + if row % 2 == 0 { 0.0 } else { 0.5 }) * (25.0 / 6.0);
                heights[index] = (slope * x + 0.1 * y) as f32;
            }
        }
        if colors {
            chunk.header.flags.value |= 0x40;
            chunk.vertex_colors = Some(MccvChunk {
                colors: (0..145)
                    .map(|index| {
                        VertexColor::from_rgba(
                            65 + (index % 50) as u8,
                            55 + (index % 60) as u8,
                            40 + (index % 70) as u8,
                            255,
                        )
                    })
                    .collect(),
            });
        }
        if shadow {
            chunk.header.flags.value |= 1;
            chunk.shadow = Some(McshChunk {
                shadow_map: [0xaa, 0x55].repeat(256),
            });
        }
        let adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
        let map_table = super::map_table();
        let wdt = super::terrain_wdt()?;
        let doodads = table(&[[1, 1, 0], [2, 1, 1], [3, 1, 2]], b"\0grass.m2\0");
        let effects = table(&[[1, 1, 2, 0, 3, 5, 3, 0, 0, effect_density, 0]], &[0]);
        let (model_bytes, skin_bytes) = detail_model()?;
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "DBFilesClient/Map.dbc",
                bytes: &map_table,
            },
            FixtureFile {
                path: "DBFilesClient/GroundEffectDoodad.dbc",
                bytes: &doodads,
            },
            FixtureFile {
                path: "DBFilesClient/GroundEffectTexture.dbc",
                bytes: &effects,
            },
            FixtureFile {
                path: "World/Maps/Northrend/Northrend.wdt",
                bytes: &wdt,
            },
            FixtureFile {
                path: "World/Maps/Northrend/Northrend_32_32.adt",
                bytes: &adt,
            },
            FixtureFile {
                path: "World/NoDXT/Detail/grass.m2",
                bytes: &model_bytes,
            },
            FixtureFile {
                path: "World/NoDXT/Detail/grass00.skin",
                bytes: &skin_bytes,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let maps = MapCatalog::load(&mut store)?;
        let map = TerrainMap::load(&mut store, maps.map(571).ok_or("map missing")?)?;
        let tile = map.load_tile(
            &mut store,
            TerrainTileIndex::new(32, 32).ok_or("invalid tile")?,
        )?;
        let effects = GroundEffectCatalog::load(&mut store)?;
        let detail = TerrainDetailChunk::prepare(&tile, index, &effects, density)?;
        let model =
            DecodedM2Model::load(&mut store, &AssetPath::new("World/NoDXT/Detail/grass.m2")?)?;
        let model = GroundDetailModel::prepare(&model)?;
        let mesh = GroundDetailMeshPlan::prepare(&detail, &effects, density, |_| Some(&model))?;
        let expected_count: usize = mesh_lines
            .next()
            .ok_or("missing mesh case")?
            .strip_prefix("case ")
            .ok_or("mesh case")?
            .parse()?;
        assert_eq!(mesh.vertices().len(), expected_count);
        for (vertex_index, vertex) in mesh.vertices().iter().enumerate() {
            let row = mesh_lines
                .next()
                .ok_or("missing mesh vertex")?
                .split_ascii_whitespace()
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()?;
            let [a, r, g, b] = (row[6] as u32).to_be_bytes();
            assert_eq!(vertex.color(), [r, g, b, a]);
            let actual = [
                vertex.position().to_vec(),
                vertex.normal().to_vec(),
                vertex.coordinates().to_vec(),
            ]
            .concat();
            for (actual, expected) in actual.iter().zip(row[..6].iter().chain(&row[7..9])) {
                assert!(
                    (f64::from(*actual) - expected).abs() < 0.0001,
                    "{line}: mesh vertex {vertex_index}: {actual} != {expected}"
                );
            }
        }
        assert_eq!(detail.placements().len(), count, "{line}");
        for (placement_index, placement) in detail.placements().iter().enumerate() {
            let values = lines
                .next()
                .ok_or("missing placement")?
                .split_ascii_whitespace()
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(
                placement.model(),
                values[0] as u32,
                "{line}: {placement_index}"
            );
            assert_eq!(
                placement.face(),
                values[9] as u16,
                "{line}: {placement_index}"
            );
            let [a, r, g, b] = (values[10] as u32).to_be_bytes();
            assert_eq!(placement.color(), [r, g, b, a], "{line}: {placement_index}");
            let actual = [
                placement.position().to_vec(),
                vec![placement.angle(), placement.scale()],
                placement.normal().to_vec(),
            ]
            .concat();
            for (component, (actual, expected)) in actual.iter().zip(&values[1..9]).enumerate() {
                assert!(
                    (f64::from(*actual) - expected).abs() < 0.0001,
                    "{line}: {placement_index} component {component}: {actual} != {expected}"
                );
            }
        }
    }
    Ok(())
}

/// Supplies the native fixture's reordered quad through exact M2/SKIN arrays.
fn detail_model() -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
    let mut model = crate::model::render_m2_bytes("detail", 1)?;
    model[0x3c..0x40].copy_from_slice(&4_u32.to_le_bytes());
    let offset = model.len() as u32;
    model[0x40..0x44].copy_from_slice(&offset.to_le_bytes());
    for (x, y, z, u, v) in [
        (-1_f32, 0_f32, 0_f32, 0_f32, 1_f32),
        (1., 0., 0., 1., 1.),
        (-1., 0., 2., 0., 0.),
        (1., 0., 2., 1., 0.),
    ] {
        for value in [x, y, z] {
            model.extend(value.to_le_bytes());
        }
        model.extend([255, 0, 0, 0, 0, 0, 0, 0]);
        for value in [0_f32, 0., 1., u, v, u, v] {
            model.extend(value.to_le_bytes());
        }
    }
    let mut skin = vec![0; 48];
    skin[..4].copy_from_slice(b"SKIN");
    for (offset, count, start) in [(4, 4_u32, 48_u32), (12, 6, 56), (20, 4, 68)] {
        skin[offset..offset + 4].copy_from_slice(&count.to_le_bytes());
        skin[offset + 4..offset + 8].copy_from_slice(&start.to_le_bytes());
    }
    for index in [2_u16, 0, 3, 1, 0, 1, 2, 2, 1, 3] {
        skin.extend(index.to_le_bytes());
    }
    skin.extend([0; 16]);
    Ok((model, skin))
}

/// Builds a strict archive-backed database table without exposing test constructors.
pub(super) fn table<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [
        rows.len() as u32,
        N as u32,
        N as u32 * 4,
        strings.len() as u32,
    ] {
        bytes.extend(value.to_le_bytes());
    }
    for row in rows {
        for value in row {
            bytes.extend(value.to_le_bytes());
        }
    }
    bytes.extend(strings);
    bytes
}

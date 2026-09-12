//! Original DetailDoodad pixels through archive inputs and the production world pass.

use std::{collections::BTreeMap, error::Error, io::Cursor, sync::Arc};

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, DecodedM2Model,
    GroundEffectCatalog, Locale, MapCatalog, TerrainChunkIndex, TerrainMap, TerrainTileIndex,
};
use solarity_rendering::{
    BlpColorSpace, BlpTextureHandle, GroundDetailDensity, GroundDetailDraw, GroundDetailFrame,
    GroundDetailMeshPlan, GroundDetailModel, M2LocalLightState, M2SceneUniform, TerrainDetailChunk,
    TerrainSceneUniform, VulkanRenderer, WorldFrameScene, WorldModelBaseMip,
    WorldModelSceneUniform, WorldModelTextureFiltering,
};
use wow_adt::{
    AdtVersion, ParsedAdt,
    builder::{AdtBuilder, BuiltAdt},
    chunks::{MccvChunk, McshChunk, MtxfChunk, VertexColor},
    parse_adt,
};

use crate::support::{Fixture, FixtureFile};

/// A single native-selected instance fills the inspected viewport with a planar quad.
pub(super) fn compare_native_detail(renderer: &mut VulkanRenderer) -> Result<(), Box<dyn Error>> {
    let mut draws = BTreeMap::new();
    let mut textures = BTreeMap::new();
    let mut count = 0;
    for line in include_str!("../fixtures/ground-detail-shader-native.txt")
        .lines()
        .filter(|line| line.starts_with("detail "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let depth: f32 = row[1].parse()?;
        let distance = row[2].parse()?;
        let exponent = row[3].parse()?;
        let argb = u32::from_str_radix(row[4], 16)?;
        let tint = [row[5].parse::<u8>()?, row[6].parse()?, row[7].parse()?];
        let shadow: u8 = row[8].parse()?;
        let key = (argb, tint, shadow);
        if let std::collections::btree_map::Entry::Vacant(entry) = draws.entry(key) {
            let (draw, texture) = prepare_draw_at_with_texture(
                renderer,
                argb,
                tint,
                shadow,
                Vec3::ZERO,
                textures.get(&argb).copied(),
            )?;
            textures.insert(argb, texture);
            entry.insert(draw);
        }
        let vector = |start: usize| -> Result<Vec3, Box<dyn Error>> {
            Ok(Vec3::new(
                row[start].parse()?,
                row[start + 1].parse()?,
                row[start + 2].parse()?,
            ))
        };
        let camera = Vec3::new(-16., -16., depth);
        let view = Mat4::look_at_rh(camera, camera - Vec3::Z, Vec3::Y);
        let projection = Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 150.);
        let ambient = vector(9)?;
        let diffuse = vector(12)?;
        let direction = vector(15)?;
        let fog = Vec4::new(0., 150., 0., exponent);
        let fog_color = Vec3::new(32., 64., 96.) / 255.;
        let terrain = TerrainSceneUniform::new(projection, view, ambient, diffuse, direction)
            .with_fog(view, fog, fog_color);
        let world =
            WorldModelSceneUniform::new(projection, view, camera, ambient, diffuse, direction, fog);
        let m2 = M2SceneUniform::new(
            projection,
            view,
            camera,
            ambient,
            diffuse,
            direction,
            fog,
            fog_color,
            [M2LocalLightState::disabled(); 4],
        );
        let scene = WorldFrameScene::new(terrain, world, m2).with_ground_detail(
            GroundDetailFrame::new(std::slice::from_ref(&draws[&key]), distance, camera)?,
        );
        renderer.request_frame_capture()?;
        let report =
            renderer.present_world_frame(scene, &[], &[], &[], &[], &[], &[], &[], &[], &[])?;
        assert_eq!(report.ground_detail_draw_count(), 1);
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing detail capture")?;
        let expected = (0..3)
            .map(|i| u8::from_str_radix(&row[18][i * 2..i * 2 + 2], 16))
            .collect::<Result<Vec<_>, _>>()?;
        for y in [24, 32, 40] {
            for x in [24, 32, 40] {
                let actual = &capture.rgba8()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
                assert!(
                    actual
                        .iter()
                        .zip(&expected)
                        .all(|(a, b)| a.abs_diff(*b) <= 2),
                    "{line}: pixel {x}/{y}: {actual:?}, expected {expected:?}"
                );
            }
        }
        count += 1;
        // Exchange CPU mesh generations while submitted slots retain their plans.
        // Reused texture handles must keep the same native pixels across retirement.
        if count % 11 == 0 {
            draws.clear();
        }
    }
    assert_eq!(count, 120);
    Ok(())
}

/// Relocates the authored chunk to exercise local-vertex receiver precision.
pub(super) fn prepare_draw_at(
    renderer: &mut VulkanRenderer,
    argb: u32,
    tint: [u8; 3],
    shadow: u8,
    origin: Vec3,
) -> Result<GroundDetailDraw, Box<dyn Error>> {
    prepare_draw_at_with_texture(renderer, argb, tint, shadow, origin, None).map(|(draw, _)| draw)
}

/// Shares texture identity across independently prepared native scatter generations.
fn prepare_draw_at_with_texture(
    renderer: &mut VulkanRenderer,
    argb: u32,
    tint: [u8; 3],
    shadow: u8,
    origin: Vec3,
    shared_texture: Option<BlpTextureHandle>,
) -> Result<(GroundDetailDraw, BlpTextureHandle), Box<dyn Error>> {
    let path = format!("fixture/detail_{argb:08x}.blp");
    let base = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture(&path)
        .build()?
        .to_bytes()?;
    let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(base))? else {
        return Err("root ADT required".into());
    };
    root.texture_flags = Some(MtxfChunk { flags: vec![0] });
    let chunk = &mut root.mcnk_chunks[0];
    chunk.header.position = origin.to_array();
    chunk.header.unknown_8bytes = (!(1_u64 << 32)).to_le_bytes();
    chunk.layers.as_mut().ok_or("layers missing")?.layers[0].effect_id = 1;
    chunk.header.flags.value |= 0x40;
    chunk.vertex_colors = Some(MccvChunk {
        colors: vec![VertexColor::from_rgba(tint[0], tint[1], tint[2], 255); 145],
    });
    if shadow == 0 {
        chunk.header.flags.value |= 1;
        chunk.shadow = Some(McshChunk {
            shadow_map: vec![255; 512],
        });
    }
    let adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    let map_table = super::map_table();
    let wdt = super::terrain_wdt()?;
    let doodads = super::ground_detail::table(&[[1, 1, 0]], b"\0detail.m2\0");
    let effects = super::ground_detail::table(&[[1, 1, 0, 0, 0, 16, 0, 0, 0, 1, 0]], &[0]);
    let (model, skin) = planar_model(&path)?;
    let blp = super::solid_raw3_blp(2, 2, argb);
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
            path: "World/NoDXT/Detail/detail.m2",
            bytes: &model,
        },
        FixtureFile {
            path: "World/NoDXT/Detail/detail00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: &path,
            bytes: &blp,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let maps = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, maps.map(571).ok_or("map missing")?)?;
    let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
    let catalog = GroundEffectCatalog::load(&mut store)?;
    let density = GroundDetailDensity::new(16).ok_or("density")?;
    let scatter = TerrainDetailChunk::prepare(
        &tile,
        TerrainChunkIndex::new(0, 0).ok_or("chunk")?,
        &catalog,
        density,
    )?;
    assert_eq!(scatter.placements().len(), 1);
    let model = GroundDetailModel::prepare(&DecodedM2Model::load(
        &mut store,
        &AssetPath::new("World/NoDXT/Detail/detail.m2")?,
    )?)?;
    let mesh = Arc::new(GroundDetailMeshPlan::prepare(
        &scatter,
        &catalog,
        density,
        |_| Some(&model),
    )?);
    let source = BlpTextureSource::load(&mut store, &AssetPath::new(path)?)?;
    let texture = match shared_texture {
        Some(texture) => texture,
        None => renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?,
    };
    Ok((
        GroundDetailDraw::new(
            mesh,
            vec![texture],
            WorldModelTextureFiltering::Trilinear,
            WorldModelBaseMip::Zero,
        )?,
        texture,
    ))
}

/// Uses a large horizontal quad so all inspected pixels share exact shader inputs.
fn planar_model(path: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
    let mut model = crate::model::render_m2_bytes("detail", 1)?;
    let textures = u32::from_le_bytes(model[0x54..0x58].try_into()?) as usize;
    let name_offset = model.len() as u32;
    model[textures + 8..textures + 12].copy_from_slice(&(path.len() as u32 + 1).to_le_bytes());
    model[textures + 12..textures + 16].copy_from_slice(&name_offset.to_le_bytes());
    model.extend(path.as_bytes());
    model.push(0);
    model[0x3c..0x40].copy_from_slice(&4_u32.to_le_bytes());
    let offset = model.len() as u32;
    model[0x40..0x44].copy_from_slice(&offset.to_le_bytes());
    for (x, y) in [
        (-100_f32, -100_f32),
        (100., -100.),
        (-100., 100.),
        (100., 100.),
    ] {
        for value in [x, y, 0.] {
            model.extend(value.to_le_bytes());
        }
        model.extend([255, 0, 0, 0, 0, 0, 0, 0]);
        for value in [0_f32, 0., 1., 0., 0., 0., 0.] {
            model.extend(value.to_le_bytes());
        }
    }
    let mut skin = vec![0; 48];
    skin[..4].copy_from_slice(b"SKIN");
    for (offset, count, start) in [(4, 4_u32, 48_u32), (12, 6, 56), (20, 4, 68)] {
        skin[offset..offset + 4].copy_from_slice(&count.to_le_bytes());
        skin[offset + 4..offset + 8].copy_from_slice(&start.to_le_bytes());
    }
    for index in [0_u16, 1, 2, 3, 0, 1, 2, 2, 1, 3] {
        skin.extend(index.to_le_bytes());
    }
    skin.extend([0; 16]);
    Ok((model, skin))
}

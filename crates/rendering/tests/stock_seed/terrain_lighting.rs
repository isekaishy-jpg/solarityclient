//! Stock colored terrain pixels through ADT, BLP, upload, and the terrain shaders.
use crate::support::{Fixture, FixtureFile};
use glam::{Mat4, Vec3};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, Locale, MapCatalog,
    TerrainMap, TerrainTileIndex,
};
use solarity_rendering::{
    BlpColorSpace, TerrainLayerCount, TerrainPreparedDraw, TerrainSceneUniform, TerrainTextureSet,
    TerrainTileMeshPlan, VulkanRenderer,
};
use std::{collections::BTreeMap, error::Error, io::Cursor};
use wow_adt::{
    AdtVersion, ParsedAdt,
    builder::{AdtBuilder, BuiltAdt},
    chunks::{MccvChunk, McshChunk, MtxfChunk, VertexColor},
    parse_adt,
};

pub(super) fn compare_native_lighting(renderer: &mut VulkanRenderer) -> Result<(), Box<dyn Error>> {
    let mut draws = BTreeMap::<(u32, Option<u32>, u8), TerrainPreparedDraw>::new();
    let mut plans = Vec::new();
    let view = Mat4::look_at_rh(
        Vec3::new(-16., -16., 10.),
        Vec3::new(-16., -16., 0.),
        Vec3::Y,
    );
    let projection = Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.);
    let mut frames = 0;
    for line in include_str!("../fixtures/terrain_lighting_shader_native.txt")
        .lines()
        .chain(include_str!("../fixtures/terrain_specular_shader_native.txt").lines())
        .filter(|row| row.starts_with("terrain ") || row.starts_with("specular "))
    {
        let row = line.split_ascii_whitespace().collect::<Vec<_>>();
        let specular = row[0] == "specular";
        let rgb = [row[1].parse::<u8>()?, row[2].parse()?, row[3].parse()?];
        let color = (row[13] != "none")
            .then(|| u32::from_str_radix(row[13], 16))
            .transpose()?;
        let shadow: u8 = row[14].parse()?;
        // MCSH supplies binary endpoints; 170 in the native fixture also
        // records the shader's response to an interpolated visibility value.
        if shadow == 170 {
            continue;
        }
        let alpha = if specular { row[20].parse()? } else { 255 };
        let argb = u32::from_be_bytes([alpha, rgb[0], rgb[1], rgb[2]]);
        let key = (argb, color, shadow);
        if let std::collections::btree_map::Entry::Vacant(entry) = draws.entry(key) {
            let texture_path = format!("tileset/fixture/color_{argb:08x}.blp");
            let base = AdtBuilder::new()
                .with_version(AdtVersion::WotLK)
                .add_texture(&texture_path)
                .build()?
                .to_bytes()?;
            let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(&base))? else {
                return Err("root ADT required".into());
            };
            root.texture_flags = Some(MtxfChunk { flags: vec![0] });
            let first = root.mcnk_chunks.first_mut().ok_or("first MCNK")?;
            first.vertex_colors = color.map(|value| {
                let [a, r, g, b] = value.to_be_bytes();
                MccvChunk {
                    colors: vec![VertexColor::from_rgba(r, g, b, a); 145],
                }
            });
            if color.is_some() {
                first.header.flags.value |= 0x40;
            }
            if shadow != 255 {
                first.shadow = Some(McshChunk {
                    shadow_map: vec![255; 512],
                });
                first.header.flags.value |= 1;
            }
            let adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
            let map_table = super::map_table();
            let wdt = super::terrain_wdt()?;
            let blp = super::solid_raw3_blp(2, 2, argb);
            let fixture = Fixture::new(&[
                FixtureFile {
                    path: "DBFilesClient/Map.dbc",
                    bytes: &map_table,
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
                    path: &texture_path,
                    bytes: &blp,
                },
            ])?;
            let mut store = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let maps = MapCatalog::load(&mut store)?;
            let map = TerrainMap::load(&mut store, maps.map(571).ok_or("map")?)?;
            let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
            let plan = Box::new(TerrainTileMeshPlan::prepare(&tile)?);
            let source = BlpTextureSource::load(&mut store, &AssetPath::new(texture_path)?)?;
            let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
            let mesh = renderer.upload_terrain_mesh(&plan)?;
            let material = renderer.upload_terrain_material(&plan)?;
            let set = TerrainTextureSet::new(material, &[texture])?;
            let handle = renderer.prepare_terrain_texture_sets(std::slice::from_ref(&set))?[0];
            let pipeline = renderer.prepare_terrain_pipeline(TerrainLayerCount::One)?;
            entry.insert(renderer.prepare_terrain_draw(mesh, pipeline, handle, &set, &plan, 0)?);
            plans.push(plan);
        }
        let vector = |start: usize| -> Result<Vec3, Box<dyn Error>> {
            Ok(Vec3::new(
                row[start].parse()?,
                row[start + 1].parse()?,
                row[start + 2].parse()?,
            ))
        };
        let mut scene =
            TerrainSceneUniform::new(projection, view, vector(4)?, vector(7)?, vector(10)?);
        if specular {
            scene = scene.with_specular(vector(16)?, row[19] == "1");
        }
        renderer.request_frame_capture()?;
        renderer.present_terrain(scene, &[draws[&key]])?;
        let frame = renderer
            .take_captured_frame()?
            .ok_or("terrain lighting capture")?;
        for (yi, y) in [24, 32, 40].into_iter().enumerate() {
            for (xi, x) in [24, 32, 40].into_iter().enumerate() {
                let offset = if specular { (yi * 3 + xi) * 8 } else { 0 };
                let expected = (0..3)
                    .map(|i| u8::from_str_radix(&row[15][offset + i * 2..offset + i * 2 + 2], 16))
                    .collect::<Result<Vec<_>, _>>()?;
                let pixel = &frame.rgba8()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 3];
                for (actual, expected) in pixel.iter().zip(&expected) {
                    assert!(
                        actual.abs_diff(*expected) <= 2,
                        "{line}: pixel {x}/{y} {pixel:?}, expected {expected}"
                    );
                }
            }
        }
        frames += 1;
    }
    assert_eq!(frames, 96);
    Ok(())
}

//! Native terrain constants and perspective interpolation through ADT/BLP upload.
#![allow(unsafe_code)]

use super::*;
use solarity_asset::{LightCatalog, exterior_light_direction};
use std::io::Cursor;
use wow_adt::{ParsedAdt, builder::BuiltAdt, parse_adt};

#[test]
fn terrain_perspective_lighting_matches_native_shader_frames() -> Result<(), Box<dyn Error>> {
    compare_frames(false)
}

#[test]
fn terrain_world_palettes_match_native_producer_and_shader_frames() -> Result<(), Box<dyn Error>> {
    compare_frames(true)
}

fn compare_frames(world: bool) -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Terrain perspective lighting", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This live window supplied the enabled extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    renderer.configure_file_texture_sampling(
        WorldModelTextureFiltering::Bilinear,
        WorldModelBaseMip::Zero,
    )?;
    let blp = solid_raw3_blp(4, 4, 0xff336699);
    let maps = map_table();
    let wdt = terrain_wdt()?;
    let text = if world {
        include_str!("../fixtures/terrain_world_palette_native.txt")
    } else {
        include_str!("../fixtures/terrain_perspective_lighting_native.txt")
    };
    let lights = world.then(|| palette_catalog(text)).transpose()?;
    let mut frames = 0;
    for line in text
        .lines()
        .filter_map(|line| line.strip_prefix(if world { "world " } else { "perspective " }))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let palette = if let Some(lights) = &lights {
            let time = words[1].parse()?;
            Some((
                lights.sample_parameter(words[0].parse()?, time)?,
                exterior_light_direction(time),
            ))
        } else {
            None
        };
        let words = &words[if world { 2 } else { 0 }..];
        let origin = [
            words[0].parse::<f32>()?,
            words[1].parse()?,
            words[2].parse()?,
        ];
        let bytes = AdtBuilder::new()
            .with_version(AdtVersion::WotLK)
            .add_texture("tileset/fixture/pattern.blp")
            .build()?
            .to_bytes()?;
        let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(&bytes))? else {
            return Err("root ADT".into());
        };
        root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
        let chunk = root.mcnk_chunks.first_mut().ok_or("first chunk")?;
        chunk.header.position = origin;
        let normal_bytes = [
            words[3].parse::<i8>()?,
            words[4].parse()?,
            words[5].parse()?,
        ];
        for normal in &mut chunk.normals.as_mut().ok_or("normals")?.normals {
            [normal.x, normal.z, normal.y] = normal_bytes;
        }
        let adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
        let fixture = Fixture::new(&[
            FixtureFile {
                path: "DBFilesClient/Map.dbc",
                bytes: &maps,
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
                path: "tileset/fixture/pattern.blp",
                bytes: &blp,
            },
        ])?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let catalog = MapCatalog::load(&mut store)?;
        let map = TerrainMap::load(&mut store, catalog.map(571).ok_or("map")?)?;
        let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
        let plan = TerrainTileMeshPlan::prepare(&tile)?;
        let source =
            BlpTextureSource::load(&mut store, &AssetPath::new("tileset/fixture/pattern.blp")?)?;
        let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
        let mesh = renderer.upload_terrain_mesh(&plan)?;
        let material = renderer.upload_terrain_material(&plan)?;
        let set = TerrainTextureSet::new(material, &[texture])?;
        let handle = renderer.prepare_terrain_texture_sets(std::slice::from_ref(&set))?[0];
        let pipeline = renderer.prepare_terrain_pipeline(TerrainLayerCount::One)?;
        let draw = renderer.prepare_terrain_draw(mesh, pipeline, handle, &set, &plan, 0)?;
        let matrix = |offset: usize| -> Result<Mat4, Box<dyn Error>> {
            let floats = words[offset..offset + 16]
                .iter()
                .map(|v| v.parse::<f32>())
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Mat4::from_cols_array(floats.as_slice().try_into()?))
        };
        let (ambient, diffuse, specular, direction) = if let Some((light, direction)) = palette {
            (
                light.ambient_color(),
                light.diffuse_color(),
                light.specular_color(),
                direction,
            )
        } else {
            (
                Vec3::new(0.2, 0.3, 0.4),
                Vec3::new(0.3, 0.2, 0.1),
                Vec3::new(0.65, 0.35, 0.15),
                Vec3::new(words[6].parse()?, words[7].parse()?, words[8].parse()?),
            )
        };
        let scene = TerrainSceneUniform::new(matrix(25)?, matrix(9)?, ambient, diffuse, direction)
            .with_specular(specular, true);
        renderer.request_frame_capture()?;
        renderer.present_terrain(scene, &[draw])?;
        let frame = renderer.take_captured_frame()?.ok_or("terrain UV frame")?;
        let mut compared = 0;
        for (index, pixel) in frame.rgba8().as_chunks::<4>().0.iter().enumerate() {
            // Interior samples avoid native/Vulkan triangle edge coverage differences.
            let (x, y) = (index % 64, index / 64);
            if !(16..48).contains(&x)
                || !(36..56).contains(&y)
                || &words[41][index * 8 + 6..index * 8 + 8] != "ff"
            {
                continue;
            }
            compared += 1;
            for (channel, actual) in pixel[..3].iter().enumerate() {
                let hex = index * 8 + channel * 2;
                let expected = u8::from_str_radix(&words[41][hex..hex + 2], 16)?;
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "frame {frames} origin {origin:?}, normal {normal_bytes:?}, pixel {index}, channel {channel}: {actual} != {expected}"
                );
            }
        }
        assert!(compared > 100, "insufficient covered pixels {compared}");
        frames += 1;
    }
    assert_eq!(frames, if world { 72 } else { 12 });
    Ok(())
}

/// Author the exact input WDBC rows retained by the native capture.
fn palette_catalog(text: &str) -> Result<LightCatalog, Box<dyn Error>> {
    let mut payloads = [Vec::new(), Vec::new(), Vec::new()];
    for line in text
        .lines()
        .filter_map(|line| line.strip_prefix("parameter "))
    {
        let words = line.split_ascii_whitespace().collect::<Vec<_>>();
        for (payload, hex) in payloads.iter_mut().zip(&words[1..]) {
            for offset in (0..hex.len()).step_by(2) {
                payload.push(u8::from_str_radix(&hex[offset..offset + 2], 16)?);
            }
        }
    }
    let table = |fields: u32, payload: &[u8]| -> Result<Vec<u8>, Box<dyn Error>> {
        let mut bytes = b"WDBC".to_vec();
        for word in [
            u32::try_from(payload.len())? / (fields * 4),
            fields,
            fields * 4,
            1,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(payload);
        bytes.push(0);
        Ok(bytes)
    };
    let tables = [
        ("DBFilesClient/Light.dbc", table(15, &[])?),
        ("DBFilesClient/LightParams.dbc", table(9, &payloads[0])?),
        ("DBFilesClient/LightIntBand.dbc", table(34, &payloads[1])?),
        ("DBFilesClient/LightFloatBand.dbc", table(34, &payloads[2])?),
        ("DBFilesClient/LightSkybox.dbc", table(3, &[])?),
    ];
    let files = tables
        .iter()
        .map(|(path, bytes)| FixtureFile { path, bytes })
        .collect::<Vec<_>>();
    let fixture = Fixture::new(&files)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    Ok(LightCatalog::load(&mut store)?)
}

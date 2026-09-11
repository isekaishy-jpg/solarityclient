//! Patterned original terrain shader frames at local and Durotar coordinates.
#![allow(unsafe_code)]

use super::*;
use std::io::Cursor;
use wow_adt::{ParsedAdt, builder::BuiltAdt, parse_adt};

#[test]
fn terrain_texture_density_and_origin_match_native_shader_frames() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Terrain texture coordinates", 64, 64)
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
    let mut blp = solid_raw3_blp(4, 4, 0);
    let offset = u32::from_le_bytes(blp[20..24].try_into()?) as usize;
    for y in 0..4_u8 {
        for x in 0..4_u8 {
            let pixel = offset + (usize::from(y) * 4 + usize::from(x)) * 4;
            blp[pixel..pixel + 4].copy_from_slice(&[
                16 + ((x + y * 2) % 4) * 60,
                24 + y * 56,
                32 + x * 48,
                255,
            ]);
        }
    }
    let maps = map_table();
    let wdt = terrain_wdt()?;
    let mut frames = 0;
    for line in include_str!("../fixtures/terrain_texture_coordinates_native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("frame "))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
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
        root.mcnk_chunks
            .first_mut()
            .ok_or("first chunk")?
            .header
            .position = origin;
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
        let origin = Vec3::from_array(origin);
        let view = Mat4::look_at_rh(
            origin + Vec3::new(-16., -16., 10.),
            origin + Vec3::new(-16., -16., 0.),
            Vec3::Y,
        );
        let scene = TerrainSceneUniform::new(
            Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.),
            view,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
        );
        renderer.request_frame_capture()?;
        renderer.present_terrain(scene, &[draw])?;
        let frame = renderer.take_captured_frame()?.ok_or("terrain UV frame")?;
        for (index, pixel) in frame.rgba8().as_chunks::<4>().0.iter().enumerate() {
            for (channel, actual) in pixel[..3].iter().enumerate() {
                let hex = index * 8 + channel * 2;
                let expected = u8::from_str_radix(&words[3][hex..hex + 2], 16)?;
                assert!(
                    actual.abs_diff(expected) <= 2,
                    "origin {origin:?}, pixel {index}, channel {channel}: {actual} != {expected}"
                );
            }
        }
        frames += 1;
    }
    assert_eq!(frames, 2);
    Ok(())
}

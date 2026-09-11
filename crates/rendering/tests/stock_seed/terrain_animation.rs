//! Original animated terrain pixels through authored WDT/ADT/BLP inputs.
#![allow(unsafe_code)]

use super::*;
use solarity_rendering::TerrainTextureAnimationState;
use std::io::Cursor;
use wow_adt::{
    McalChunk, MclyChunk, MclyFlags, MclyLayer, ParsedAdt, builder::BuiltAdt, chunks::MtxfChunk,
    parse_adt,
};

#[test]
fn terrain_animation_matches_native_shader_frames() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Terrain animation", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This live window supplied the enabled extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let paths = [
        "tileset/fixture/base.blp",
        "tileset/fixture/one.blp",
        "tileset/fixture/two.blp",
        "tileset/fixture/three.blp",
    ];
    renderer.configure_file_texture_sampling(
        WorldModelTextureFiltering::Bilinear,
        WorldModelBaseMip::Zero,
    )?;
    let mut blps = Vec::new();
    for layer in 0..4_u8 {
        let mut blp = solid_raw3_blp(4, 4, 0);
        let offset = u32::from_le_bytes(blp[20..24].try_into()?) as usize;
        for y in 0..4_u8 {
            for x in 0..4_u8 {
                let pixel = offset + (usize::from(y) * 4 + usize::from(x)) * 4;
                blp[pixel..pixel + 4].copy_from_slice(&[
                    16 + ((x + y * 2 + layer) % 4) * 60,
                    24 + ((y + layer) % 4) * 56,
                    32 + ((x + layer) % 4) * 48,
                    255,
                ]);
            }
        }
        blps.push(blp);
    }
    let maps = map_table();
    let mut frames = 0;
    let mut animation = TerrainTextureAnimationState::default();
    let mut offsets = 0;
    for line in include_str!("../fixtures/terrain_animation_native.txt").lines() {
        if let Some(delta) = line.strip_prefix("delta ") {
            animation.advance(delta.parse()?);
            continue;
        }
        if let Some(offset) = line.strip_prefix("offset ") {
            let values = offset.split_whitespace().collect::<Vec<_>>();
            let flags = values[0].parse::<u32>()?;
            let expected_x = values[1].parse::<f32>()?;
            let expected_y = values[2].parse::<f32>()?;
            let actual = animation.texture_offset(flags);
            assert_eq!(actual.x.to_bits(), expected_y.to_bits(), "{line}");
            assert_eq!(actual.y.to_bits(), expected_x.to_bits(), "{line}");
            assert_eq!(animation.texture_offset(flags & !0x40), glam::Vec2::ZERO);
            offsets += 1;
            continue;
        }
        let Some(line) = line.strip_prefix("pixels ") else {
            continue;
        };
        let words = line.split_whitespace().collect::<Vec<_>>();
        let layers = 4;
        let flags = words[..4]
            .iter()
            .map(|word| word.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()?;
        let mut builder = AdtBuilder::new().with_version(AdtVersion::WotLK);
        for path in &paths[..layers] {
            builder = builder.add_texture(*path);
        }
        let base = builder.build()?.to_bytes()?;
        let ParsedAdt::Root(mut root) = parse_adt(&mut Cursor::new(&base))? else {
            return Err("root ADT".into());
        };
        root.texture_flags = Some(MtxfChunk {
            flags: vec![0; layers],
        });
        let chunk = root.mcnk_chunks.first_mut().ok_or("first chunk")?;
        chunk.header.n_layers = layers as u32;
        let plane_bytes = 2048;
        chunk.layers = Some(MclyChunk {
            layers: (0..layers)
                .map(|i| MclyLayer {
                    texture_id: i as u32,
                    flags: MclyFlags {
                        value: flags[i] | if i > 0 { 0x100 } else { 0 },
                    },
                    offset_in_mcal: i.saturating_sub(1) as u32 * plane_bytes,
                    effect_id: 0,
                })
                .collect(),
        });
        // Equal nibbles in small MCAL expand to the native blend weights.
        chunk.alpha = Some(McalChunk::new(
            [34_u8, 51, 68]
                .into_iter()
                .flat_map(|value| std::iter::repeat_n(value, plane_bytes as usize))
                .collect(),
        ));
        let adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
        let mut wdt = WdtFile::new(WowVersion::WotLK);
        wdt.mwmo = Some(MwmoChunk::new());
        wdt.main.get_mut(32, 32).ok_or("tile")?.set_has_adt(true);
        let mut wdt_bytes = Vec::new();
        WdtWriter::new(&mut wdt_bytes).write(&wdt)?;
        let mut files = vec![
            FixtureFile {
                path: "DBFilesClient/Map.dbc",
                bytes: &maps,
            },
            FixtureFile {
                path: "World/Maps/Northrend/Northrend.wdt",
                bytes: &wdt_bytes,
            },
            FixtureFile {
                path: "World/Maps/Northrend/Northrend_32_32.adt",
                bytes: &adt,
            },
        ];
        files.extend(
            paths
                .iter()
                .zip(&blps)
                .map(|(path, bytes)| FixtureFile { path, bytes }),
        );
        let fixture = Fixture::new(&files)?;
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let catalog = MapCatalog::load(&mut store)?;
        let map = TerrainMap::load(&mut store, catalog.map(571).ok_or("map")?)?;
        let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
        let plan = TerrainTileMeshPlan::prepare(&tile)?;
        let mut textures = Vec::new();
        for path in &paths[..layers] {
            let source = BlpTextureSource::load(&mut store, &AssetPath::new(path)?)?;
            textures.push(renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?);
        }
        let mesh = renderer.upload_terrain_mesh(&plan)?;
        let material = renderer.upload_terrain_material(&plan)?;
        let set = TerrainTextureSet::new(material, &textures)?;
        let handle = renderer.prepare_terrain_texture_sets(std::slice::from_ref(&set))?[0];
        let pipeline = renderer.prepare_terrain_pipeline(TerrainLayerCount::try_from(layers)?)?;
        let draw = renderer.prepare_terrain_draw(mesh, pipeline, handle, &set, &plan, 0)?;
        let view = Mat4::look_at_rh(
            Vec3::new(-16., -16., 10.),
            Vec3::new(-16., -16., 0.),
            Vec3::Y,
        );
        let scene = TerrainSceneUniform::new(
            Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.),
            view,
            Vec3::splat(0.5),
            Vec3::ZERO,
            Vec3::Z,
        )
        .with_texture_animation(&animation);
        renderer.request_frame_capture()?;
        renderer.present_terrain(scene, &[draw])?;
        let frame = renderer
            .take_captured_frame()?
            .ok_or("terrain material frame")?;
        for (yi, y) in [24, 32, 40].into_iter().enumerate() {
            for (xi, x) in [24, 32, 40].into_iter().enumerate() {
                for channel in 0..3 {
                    let offset = (yi * 3 + xi) * 8 + channel * 2;
                    let expected = u8::from_str_radix(&words[4][offset..offset + 2], 16)?;
                    let actual = frame.rgba8()[(y * 64 + x) * 4 + channel];
                    assert!(
                        actual.abs_diff(expected) <= 2,
                        "{line}, pixel {x}/{y}, channel {channel}: {actual} != {expected}"
                    );
                }
            }
        }
        frames += 1;
    }
    assert_eq!(frames, 12);
    assert_eq!(offsets, 384);
    Ok(())
}

//! Original weighted/unlit terrain pixels through authored WDT/ADT/BLP inputs.
#![allow(unsafe_code)]

use super::*;
use std::io::Cursor;
use wow_adt::{
    McalChunk, MclyChunk, MclyFlags, MclyLayer, ParsedAdt,
    builder::BuiltAdt,
    chunks::{MccvChunk, McshChunk, MtxfChunk, VertexColor},
    parse_adt,
};
use wow_wdt::chunks::MphdFlags;

#[test]
fn terrain_material_variants_match_native_shader_frames() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Terrain material variants", 64, 64)
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
    let colors = [0x22336699, 0x55993355, 0xaa559933, 0xff774499];
    let blps = colors.map(|color| solid_raw3_blp(4, 4, color));
    let maps = map_table();
    let mut frames = 0;
    for line in include_str!("../fixtures/terrain_material_shader_native.txt")
        .lines()
        .filter_map(|line| line.strip_prefix("material "))
    {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let weighted = words[0] == "1";
        let layers = words[1].parse::<usize>()?;
        let unlit = words[2].parse::<u32>()?;
        let palette = words[3] == "1";
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
        let plane_bytes = if weighted { 4096 } else { 2048 };
        chunk.layers = Some(MclyChunk {
            layers: (0..layers)
                .map(|i| MclyLayer {
                    texture_id: i as u32,
                    flags: MclyFlags {
                        value: (if i > 0 { 0x100 } else { 0 })
                            | (if unlit & (1 << i) != 0 { 0x80 } else { 0 }),
                    },
                    offset_in_mcal: i.saturating_sub(1) as u32 * plane_bytes,
                    effect_id: 0,
                })
                .collect(),
        });
        let alpha = if palette {
            [170_u8, 153, 136]
        } else {
            [34_u8, 51, 68]
        };
        if layers > 1 {
            // Repeated equal nibbles expand to the same UNORM byte in small mode.
            chunk.alpha = Some(McalChunk::new(
                alpha[..layers - 1]
                    .iter()
                    .flat_map(|value| std::iter::repeat_n(*value, plane_bytes as usize))
                    .collect(),
            ));
        }
        if palette {
            chunk.vertex_colors = Some(MccvChunk {
                colors: vec![VertexColor::from_rgba(255, 128, 64, 255); 145],
            });
            chunk.shadow = Some(McshChunk {
                shadow_map: vec![255; 512],
            });
            chunk.header.flags.value |= 0x41;
        }
        let adt = BuiltAdt::from_root_adt(*root, None).to_bytes()?;
        let mut wdt = WdtFile::new(WowVersion::WotLK);
        wdt.mwmo = Some(MwmoChunk::new());
        wdt.main.get_mut(32, 32).ok_or("tile")?.set_has_adt(true);
        if weighted {
            wdt.mphd.flags |= MphdFlags::ADT_HAS_BIG_ALPHA;
        }
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
            Vec3::new(0.2, 0.3, 0.4),
            Vec3::new(0.3, 0.2, 0.1),
            Vec3::Z,
        )
        .with_specular(Vec3::new(0.65, 0.35, 0.15), true);
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
    assert_eq!(frames, 120);
    Ok(())
}

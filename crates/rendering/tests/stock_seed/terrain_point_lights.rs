//! Native terrain point-light selection and shader frames at two world origins.
#![allow(unsafe_code)]

use super::*;
use solarity_rendering::{M2PointLight, ScenePointLights};
use std::io::Cursor;
use wow_adt::{ParsedAdt, builder::BuiltAdt, parse_adt};

#[test]
fn terrain_point_lights_match_native_queries_and_shader_frames() -> Result<(), Box<dyn Error>> {
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Terrain point lights", 64, 64)
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
    let blp = solid_raw3_blp(4, 4, 0x55336699);
    let maps = map_table();
    let wdt = terrain_wdt()?;
    let mut frames = 0;
    for line in include_str!("../fixtures/terrain_point_lights_native.txt")
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
        let chunk = root.mcnk_chunks.first_mut().ok_or("first chunk")?;
        chunk.header.position = origin;
        if words[4] == "1" {
            chunk.vertex_colors = Some(wow_adt::chunks::MccvChunk {
                colors: vec![wow_adt::chunks::VertexColor::from_rgba(255, 128, 64, 255); 145],
            });
            chunk.header.flags.value |= 0x40;
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
        let origin = Vec3::from_array(origin);
        let eye = origin + Vec3::new(-16., -16., 10.);
        let view = Mat4::look_at_rh(eye, origin + Vec3::new(-16., -16., 0.), Vec3::Y);
        let (center, radius) = plan.chunks()[0].point_light_bounds();
        for (actual, word) in center
            .to_array()
            .into_iter()
            .chain([radius])
            .zip(&words[6..10])
        {
            assert_eq!(
                actual.to_bits(),
                word.parse::<f32>()?.to_bits(),
                "native chunk sphere: {line}"
            );
        }
        let mut lights = ScenePointLights::default();
        let positions = [
            Vec3::new(-38., -16., 4.),
            Vec3::new(-16., -16., 1.),
            Vec3::new(-13., -16., 3.),
            Vec3::new(-16., -12., 2.),
            Vec3::new(4., -16., 2.),
            Vec3::new(-50., -16., 4.),
        ];
        let count = words[3].parse::<usize>()?;
        let scale = if words[4] == "1" { 32. } else { 1. };
        for (index, position) in positions.into_iter().take(count).enumerate() {
            let index = index as f64;
            lights.publish(M2PointLight::new(
                origin + position,
                Vec3::ZERO,
                Vec3::new(
                    (scale * (0.15 + 0.05 * index)) as f32,
                    (scale * (0.4 - 0.03 * index)) as f32,
                    (scale * (0.1 + 0.04 * index)) as f32,
                ),
            ))?;
        }
        let points = lights.terrain_lighting(center, radius, eye)?;
        let draw = draw.with_point_lights(points);
        for (index, point) in points.iter().enumerate() {
            for (field, values) in [point.position(), point.diffuse(), point.attenuation()]
                .into_iter()
                .enumerate()
            {
                for (axis, value) in values.to_array().into_iter().enumerate() {
                    let expected = words[10 + index * 12 + field * 4 + axis].parse::<f32>()?;
                    assert_eq!(
                        value.to_bits(),
                        expected.to_bits(),
                        "native point {index}/{field}/{axis}: {line}"
                    );
                }
            }
        }
        let scene = TerrainSceneUniform::new(
            Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.),
            view,
            Vec3::new(0.2, 0.3, 0.4),
            Vec3::new(0.3, 0.2, 0.1),
            Vec3::Z,
        )
        .with_specular(Vec3::new(0.65, 0.35, 0.15), words[5] == "1");
        renderer.request_frame_capture()?;
        if origin == Vec3::ZERO {
            renderer.present_terrain(scene, &[draw])?;
        } else {
            let world = WorldFrameScene::new(
                scene,
                WorldModelSceneUniform::new(
                    Mat4::IDENTITY,
                    view,
                    eye,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    Vec3::Z,
                    Vec4::ZERO,
                ),
                M2SceneUniform::new(
                    Mat4::IDENTITY,
                    view,
                    eye,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    Vec3::Z,
                    Vec4::ZERO,
                    Vec3::ZERO,
                    [M2LocalLightState::disabled(); 4],
                ),
            );
            renderer.present_world_frame(world, &[], &[draw], &[], &[], &[], &[], &[], &[], &[])?;
        }
        let frame = renderer.take_captured_frame()?.ok_or("terrain UV frame")?;
        for (yi, y) in [24, 32, 40].into_iter().enumerate() {
            for (xi, x) in [24, 32, 40].into_iter().enumerate() {
                for channel in 0..3 {
                    let hex = (yi * 3 + xi) * 8 + channel * 2;
                    let expected = u8::from_str_radix(&words[46][hex..hex + 2], 16)?;
                    let actual = frame.rgba8()[(y * 64 + x) * 4 + channel];
                    assert!(
                        actual.abs_diff(expected) <= 2,
                        "pixel {x}/{y}, channel {channel}: {actual} != {expected}: {line}"
                    );
                }
            }
        }
        frames += 1;
    }
    assert_eq!(frames, 32);
    Ok(())
}

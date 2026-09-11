//! File sampler mip selection through actual terrain and M2 GPU descriptors.
#![allow(unsafe_code)]

use super::*;
use solarity_asset::{MapCatalog, TerrainMap, TerrainTileIndex};
use solarity_rendering::{
    TerrainLayerCount, TerrainTextureSet, TerrainTileMeshPlan, WorldModelBaseMip,
    WorldModelTextureFiltering,
};
use wow_adt::{AdtVersion, builder::AdtBuilder};

#[test]
fn file_texture_sampling_reaches_terrain_and_m2_without_changing_explicit_samplers()
-> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("FileSampling", 1)?;
    let vertices = m2_array_offset(&bytes, 0x3c)?;
    for (index, position) in [[-1_f32, -1., 0.5], [3., -1., 0.5], [-1., 3., 0.5]]
        .into_iter()
        .enumerate()
    {
        for (axis, value) in position.into_iter().enumerate() {
            let offset = vertices + index * 48 + axis * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let skin = render_skin_bytes()?;
    let colors = solid_raw3_blp(8, 8, &[0xff00_ff00, 0xffff_0000, 0xffff_0000, 0xffff_0000]);
    let map_bytes = crate::terrain::map_table();
    let wdt = crate::terrain::terrain_wdt()?;
    let adt = AdtBuilder::new()
        .with_version(AdtVersion::WotLK)
        .add_texture("Creature/Solarity/FileSampling.blp")
        .build()?
        .to_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Creature/Solarity/FileSampling.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Creature/Solarity/FileSampling00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Creature/Solarity/FileSampling.blp",
            bytes: &colors,
        },
        FixtureFile {
            path: "DBFilesClient/Map.dbc",
            bytes: &map_bytes,
        },
        FixtureFile {
            path: "World/Maps/Northrend/Northrend.wdt",
            bytes: &wdt,
        },
        FixtureFile {
            path: "World/Maps/Northrend/Northrend_32_32.adt",
            bytes: &adt,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Creature/Solarity/FileSampling.m2")?,
    )?;
    let source = BlpTextureSource::load(
        &mut store,
        &AssetPath::new("Creature/Solarity/FileSampling.blp")?,
    )?;
    let model_plan = M2MeshPlan::prepare(&model, 0)?;
    let maps = MapCatalog::load(&mut store)?;
    let map = TerrainMap::load(&mut store, maps.map(571).ok_or("map")?)?;
    let tile = map.load_tile(&mut store, TerrainTileIndex::new(32, 32).ok_or("tile")?)?;
    let terrain = TerrainTileMeshPlan::prepare(&tile)?;
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    for base in [WorldModelBaseMip::Zero, WorldModelBaseMip::One] {
        let window = video
            .window("File texture sampling", 64, 64)
            .vulkan()
            .hidden()
            .build()?;
        let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
        // SAFETY: The live SDL window supplied this instance's extensions.
        let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
        let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
        renderer
            .configure_file_texture_sampling(WorldModelTextureFiltering::Anisotropic16x, base)?;
        let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
        let explicit = renderer.prepare_m2_sampler(&model.textures()[0])?;
        let file = renderer.prepare_m2_file_sampler(&model.textures()[0])?;
        assert_ne!(file, explicit);
        assert_eq!(
            renderer.prepare_m2_file_sampler(&model.textures()[0])?,
            file
        );
        let info = renderer.m2_sampler_info(file).ok_or("file sampler")?;
        assert_eq!(
            info.file_filtering(),
            Some(WorldModelTextureFiltering::Anisotropic16x)
        );
        assert_eq!(info.base_mip(), base);
        assert!((1.0..=16.0).contains(&info.effective_anisotropy()));
        assert!(
            renderer
                .configure_file_texture_sampling(WorldModelTextureFiltering::Bilinear, base)
                .is_err()
        );
        renderer
            .configure_file_texture_sampling(WorldModelTextureFiltering::Anisotropic16x, base)?;
        let terrain_mesh = renderer.upload_terrain_mesh(&terrain)?;
        let material = renderer.upload_terrain_material(&terrain)?;
        let set = TerrainTextureSet::new(material, &[texture])?;
        let sets = renderer.prepare_terrain_texture_sets(std::slice::from_ref(&set))?;
        let pipeline = renderer.prepare_terrain_pipeline(TerrainLayerCount::One)?;
        let draw =
            renderer.prepare_terrain_draw(terrain_mesh, pipeline, sets[0], &set, &terrain, 0)?;
        let view = Mat4::look_at_rh(
            Vec3::new(-16., -16., 10.),
            Vec3::new(-16., -16., 0.),
            Vec3::Y,
        );
        renderer.request_frame_capture()?;
        renderer.present_terrain(
            TerrainSceneUniform::new(
                Mat4::orthographic_rh(-10., 10., -10., 10., 0.1, 100.),
                view,
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
            ),
            &[draw],
        )?;
        let frame = renderer.take_captured_frame()?.ok_or("terrain capture")?;
        let expected = if base == WorldModelBaseMip::Zero {
            [0, 255, 0]
        } else {
            [255, 0, 0]
        };
        assert_color(frame.rgba8(), expected)?;
        if base == WorldModelBaseMip::One {
            let mesh = renderer.upload_m2_mesh(&model_plan)?;
            let model_draw = &model_plan.draws()[0];
            let shader = M2ShaderPlan::resolve(&model, model_draw)?;
            let pipeline = renderer.prepare_m2_pipeline(
                shader,
                M2ShaderPermutation::resolve(
                    model_draw,
                    M2LocalLightCount::Zero,
                    M2ShadowPermutation::Disabled,
                    M2ShadowFiltering::Direct,
                ),
            )?;
            for (sampler, expected) in [
                (explicit, [0, 255, 0]),
                (file, [255, 0, 0]),
                (explicit, [0, 255, 0]),
            ] {
                let stage = M2SampledTexture::new(texture, sampler);
                let set = renderer.prepare_m2_texture_sets(&[M2TextureSet::Two([stage; 2])])?[0];
                let draw = renderer.prepare_m2_draw(
                    mesh,
                    pipeline,
                    set,
                    &model_plan,
                    0,
                    false,
                    M2MaterialUniform::new(
                        Mat4::IDENTITY,
                        [Mat4::IDENTITY; 2],
                        Mat4::IDENTITY,
                        Vec4::ONE,
                        Vec4::ZERO,
                        Vec4::ZERO,
                    ),
                    0,
                    0,
                )?;
                let bones = vec![Mat4::IDENTITY; draw.required_bone_transforms()];
                let scene = WorldFrameScene::new(
                    TerrainSceneUniform::new(
                        Mat4::IDENTITY,
                        Mat4::IDENTITY,
                        Vec3::ZERO,
                        Vec3::ZERO,
                        Vec3::Z,
                    ),
                    WorldModelSceneUniform::new(
                        Mat4::IDENTITY,
                        Mat4::IDENTITY,
                        Vec3::ZERO,
                        Vec3::ZERO,
                        Vec3::ZERO,
                        Vec3::Z,
                        Vec4::ZERO,
                    ),
                    M2SceneUniform::new(
                        Mat4::IDENTITY,
                        Mat4::IDENTITY,
                        Vec3::Z,
                        Vec3::ONE,
                        Vec3::ZERO,
                        Vec3::Z,
                        Vec4::new(10., 100., 0., 1.),
                        Vec3::ZERO,
                        [M2LocalLightState::disabled(); 4],
                    ),
                );
                renderer.request_frame_capture()?;
                renderer.present_world_frame(
                    scene,
                    &bones,
                    &[],
                    &[],
                    &[draw],
                    &[],
                    &[],
                    &[],
                    &[],
                    &[],
                )?;
                let frame = renderer.take_captured_frame()?.ok_or("M2 capture")?;
                assert_color(frame.rgba8(), expected)?;
            }
        }
        renderer.shutdown()?;
    }
    Ok(())
}

fn assert_color(bytes: &[u8], expected: [u8; 3]) -> Result<(), Box<dyn Error>> {
    let pixel = bytes
        .get((32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3)
        .ok_or("pixel")?;
    assert!(
        pixel.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "actual={pixel:?} expected={expected:?}"
    );
    Ok(())
}

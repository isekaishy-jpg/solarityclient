//! Native per-pass MapObj fog register and original shader pixel regressions.

use super::surface_fog::chunk_mut;
use super::*;
use solarity_asset::{DecodedWorldModel, WorldModelBatchClass};
use solarity_rendering::{
    WorldModelBaseMip, WorldModelFogMode, WorldModelMaterialState, WorldModelMaterialUniform,
    WorldModelMeshPlan, WorldModelSampledTexture, WorldModelSurfacePassPlan,
    WorldModelTextureFiltering, WorldModelTextureSet,
};

fn native_rows() -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    include_str!("../../fixtures/world_model_surface_fog_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            Ok(line
                .split_whitespace()
                .map(str::parse)
                .collect::<Result<Vec<_>, _>>()?)
        })
        .collect()
}

#[test]
fn world_model_fog_passes_match_original_callbacks() -> Result<(), Box<dyn Error>> {
    let rows = native_rows()?;
    assert_eq!(rows.len(), 1980);
    for unfogged in 0..2 {
        for blend in 0u32..11 {
            let mut root = crate::world_model::root_fixture();
            let material = chunk_mut(&mut root, b"TMOM")?;
            material[..4].copy_from_slice(&(5u32 | (unfogged * 2)).to_le_bytes());
            material[8..12].copy_from_slice(&blend.to_le_bytes());
            let group = crate::world_model::group_fixture();
            let fixture = Fixture::new(&[
                FixtureFile {
                    path: "Fog.wmo",
                    bytes: &root,
                },
                FixtureFile {
                    path: "Fog_000.wmo",
                    bytes: &group,
                },
            ])?;
            let mut assets = AssetStore::mount(ArchiveCatalog::discover(
                ClientDataRoot::new(fixture.data_root())?,
                Locale::EnUs,
            )?)?;
            let model = DecodedWorldModel::load(&mut assets, &AssetPath::new("Fog.wmo")?)?;
            for row in rows
                .iter()
                .filter(|row| row[4] == unfogged as f32 && row[5] == blend as f32)
            {
                let class = [
                    WorldModelBatchClass::Transition,
                    WorldModelBatchClass::Interior,
                    WorldModelBatchClass::Exterior,
                ][row[3] as usize];
                let passes = WorldModelSurfacePassPlan::prepare(
                    row[0] as u16 * 2,
                    (row[1] as u32 * 4) | row[2] as u32,
                    class,
                    &model.materials()[0],
                );
                let pass = passes.passes()[row[7] as usize];
                assert_eq!(
                    pass.material().blend().mode().index(),
                    row[8] as u32,
                    "{row:?}"
                );
                assert_eq!(
                    pass.fog_mode() != WorldModelFogMode::Disabled,
                    row[9] != 0.,
                    "{row:?}"
                );
                let selected = if row[6] != 0. {
                    Vec3::new(0.8, 0.4, 0.2)
                } else {
                    Vec3::new(0.2, 0.4, 0.6)
                };
                let color = if pass.fog_mode() == WorldModelFogMode::OutdoorColor {
                    Vec3::new(0.2, 0.4, 0.6)
                } else {
                    selected
                };
                let uniform = WorldModelMaterialUniform::new(
                    Mat4::IDENTITY,
                    [0; 4],
                    &model.materials()[0],
                    pass,
                    1.,
                    color,
                );
                assert_eq!(uniform.behavior()[2], row[9] as u32, "{row:?}");
                if row[9] != 0. {
                    for (actual, expected) in color.to_array().iter().zip(&row[14..17]) {
                        assert!((actual - expected).abs() < 0.000001, "{row:?}");
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
#[allow(unsafe_code)] // The hidden SDL surface transfers solely to Vulkan.
fn world_model_fog_pixels_match_original_physical_passes() -> Result<(), Box<dyn Error>> {
    let rows = native_rows()?;
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Native WMO pass fog", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The window outlives the renderer's solely owned surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let white_bytes = solid_raw3_blp(1, 1, &[0xffff_ffff]);
    let texture_fixture = Fixture::new(&[FixtureFile {
        path: "Surface.blp",
        bytes: &white_bytes,
    }])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(texture_fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let texture = BlpTextureSource::load(&mut assets, &AssetPath::new("Surface.blp")?)?;
    let white = renderer.upload_blp_texture(&texture, BlpColorSpace::Linear)?;
    let camera = WorldCamera::orthographic(
        Vec3::ZERO,
        Vec3::NEG_Z,
        Vec3::Y,
        [-1., 1.],
        [-1., 1.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let scene = WorldFrameScene::new(
        TerrainSceneUniform::new(
            camera.projection(),
            camera.view(),
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
        ),
        WorldModelSceneUniform::new(
            camera.projection(),
            camera.view(),
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::new(2., 22., 0., 2.),
        ),
        M2SceneUniform::new(
            camera.projection(),
            camera.view(),
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        ),
    )
    .with_background_color(Vec4::new(26. / 255., 51. / 255., 76. / 255., 102. / 255.));
    let mut captures = 0;
    for native in rows.chunk_by(|a, b| a[..7] == b[..7]) {
        let row = &native[0];
        // All blends and fog flags, transition/exterior, both callback families,
        // ordinary/selected banks. The register test covers remaining aliases.
        if row[1] != 1. || row[2] == 64. || row[3] == 1. || row[6] != 1. {
            continue;
        }
        let mut root = crate::world_model::root_fixture();
        let texture_chunk = root
            .windows(4)
            .position(|value| value == b"XTOM")
            .ok_or("MOTX")?;
        root[texture_chunk + 4..texture_chunk + 8].copy_from_slice(&12u32.to_le_bytes());
        root.splice(texture_chunk + 8..texture_chunk + 9, *b"Surface.blp\0");
        chunk_mut(&mut root, b"DHOM")?[60..64]
            .copy_from_slice(&(8u32 | (row[0] as u32 * 2)).to_le_bytes());
        let material = chunk_mut(&mut root, b"TMOM")?;
        material[..4].copy_from_slice(&(5u32 | (row[4] as u32 * 2)).to_le_bytes());
        material[8..12].copy_from_slice(&(row[5] as u32).to_le_bytes());
        material[16..20].fill(0);
        let mut group = crate::world_model::group_fixture();
        group[28..32].copy_from_slice(&(4u32 | row[2] as u32).to_le_bytes());
        group[60..66].fill(0);
        let count_offset = 60 + row[3] as usize * 2;
        group[count_offset..count_offset + 2].copy_from_slice(&1u16.to_le_bytes());
        let vertices = chunk_mut(&mut group[88..], b"TVOM")?;
        for (index, value) in [-1f32, -1., 0., 3., -1., 0., -1., 3., 0.]
            .iter()
            .enumerate()
        {
            vertices[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        let alpha = if row[3] != 0. && row[5] < 2. {
            255
        } else {
            128
        };
        chunk_mut(&mut group[88..], b"VCOM")?.copy_from_slice(&[96, 64, 32, alpha].repeat(3));
        let root_path = format!("Fog{captures}.wmo");
        let group_path = format!("Fog{captures}_000.wmo");
        let fixture = Fixture::new(&[
            FixtureFile {
                path: &root_path,
                bytes: &root,
            },
            FixtureFile {
                path: &group_path,
                bytes: &group,
            },
        ])?;
        let mut assets = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = DecodedWorldModel::load(&mut assets, &AssetPath::new(root_path)?)?;
        let plan = WorldModelMeshPlan::prepare(&model)?;
        let mesh = renderer.upload_world_model_mesh(&plan)?;
        let material = &plan.materials()[0];
        let sampler = renderer.prepare_world_model_sampler(
            WorldModelMaterialState::from_material(material),
            WorldModelTextureFiltering::Bilinear,
            WorldModelBaseMip::Zero,
        )?;
        let textures = renderer.prepare_world_model_texture_sets(&[WorldModelTextureSet::One(
            WorldModelSampledTexture::new(white, sampler),
        )])?[0];
        let passes = WorldModelSurfacePassPlan::prepare(
            plan.root_flags(),
            plan.groups()[0].flags(),
            plan.draws()[0].class(),
            material,
        );
        assert_eq!(passes.passes().len(), native.len());
        for (index, pass) in passes.passes().iter().enumerate() {
            let pipeline = renderer.prepare_world_model_pipeline(passes.is_unified(), *pass)?;
            let draw = renderer
                .prepare_world_model_draw(
                    mesh,
                    pipeline,
                    textures,
                    &plan,
                    0,
                    index,
                    Mat4::from_translation(Vec3::new(0., 0., -12.)),
                    1.,
                    Vec3::new(0.8, 0.4, 0.2),
                )?
                .with_outdoor_fog_color(Vec3::new(0.2, 0.4, 0.6));
            renderer.request_frame_capture()?;
            renderer.present_world_frame(scene, &[], &[], &[draw], &[], &[], &[], &[], &[], &[])?;
            let capture = renderer.take_captured_frame()?.ok_or("WMO fog capture")?;
            let pixel = rgba8_pixel(capture.rgba8(), 64, 32, 32);
            for (actual, expected) in pixel.iter().zip(&native[index][17..21]) {
                assert!(
                    (f32::from(*actual) - expected).abs() <= 1.,
                    "{:?}: {pixel:?}",
                    native[index]
                );
            }
            captures += 1;
        }
    }
    assert_eq!(captures, 264);
    Ok(())
}

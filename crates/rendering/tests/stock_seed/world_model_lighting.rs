//! Rendered regression for 7C8560's missing-MOCV colors in MapObj and MapObjU.

#![allow(unsafe_code)]

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedWorldModel, Locale,
};
use solarity_rendering::{
    M2LocalLightState, M2SceneUniform, TerrainSceneUniform, VulkanBootstrap, WorldCamera,
    WorldFrameScene, WorldModelBaseMip, WorldModelMaterialState, WorldModelMeshPlan,
    WorldModelSampledTexture, WorldModelSceneUniform, WorldModelSurfacePassPlan,
    WorldModelTextureFiltering, WorldModelTextureSet,
};

use crate::support::{Fixture, FixtureFile};

/// Native black unified and half-gray ordinary defaults both retain ambient response.
#[test]
fn world_model_missing_vertex_colors_preserve_stock_rendered_lighting() -> Result<(), Box<dyn Error>>
{
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity WMO absent colors", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled the live window's required extensions.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole ownership of this SDL surface transfers to the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let green = renderer.upload_stock_world_model_green()?;
    let camera = WorldCamera::stock(
        Vec3::new(0.5, 0.5, 2.0),
        Vec3::new(0.5, 0.5, 0.0),
        Vec3::Y,
        100.0,
    )
    .frame(1.0)?;
    for root_flags in [0_u32, 2, 5, 8, 10, 15] {
        let mut root = crate::world_model::root_fixture();
        let header = chunk_mut(&mut root, b"DHOM")?;
        header[60..64].copy_from_slice(&root_flags.to_le_bytes());
        let material = chunk_mut(&mut root, b"TMOM")?;
        material[0..4].copy_from_slice(&6_u32.to_le_bytes()); // Unfogged, two-sided.
        material[8..12].fill(0); // Opaque diffuse shader.
        material[16..20].fill(0); // No additive material color.
        let original_group = crate::world_model::group_fixture();
        let mut group = original_group[..88].to_vec();
        let mut cursor = 88;
        while cursor < original_group.len() {
            let size =
                u32::from_le_bytes(original_group[cursor + 4..cursor + 8].try_into()?) as usize;
            if &original_group[cursor..cursor + 4] != b"VCOM" {
                group.extend_from_slice(&original_group[cursor..cursor + 8 + size]);
            }
            cursor += 8 + size;
        }
        let group_size = (group.len() - 20) as u32;
        group[16..20].copy_from_slice(&group_size.to_le_bytes());
        group[28..32].copy_from_slice(&8_u32.to_le_bytes()); // Exterior group.
        group[60..64].fill(0); // No transition or interior batches.
        group[64..66].copy_from_slice(&1_u16.to_le_bytes());
        let root_path = format!("Render{root_flags}.wmo");
        let group_path = format!("Render{root_flags}_000.wmo");
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
        let mut store = AssetStore::mount(ArchiveCatalog::discover(
            ClientDataRoot::new(fixture.data_root())?,
            Locale::EnUs,
        )?)?;
        let model = DecodedWorldModel::load(&mut store, &AssetPath::new(root_path)?)?;
        let plan = WorldModelMeshPlan::prepare(&model)?;
        let mesh = renderer.upload_world_model_mesh(&plan)?;
        let material = &plan.materials()[0];
        let sampler = renderer.prepare_world_model_sampler(
            WorldModelMaterialState::from_material(material),
            WorldModelTextureFiltering::Bilinear,
            WorldModelBaseMip::Zero,
        )?;
        let textures = renderer.prepare_world_model_texture_sets(&[WorldModelTextureSet::One(
            WorldModelSampledTexture::new(green, sampler),
        )])?[0];
        let passes = WorldModelSurfacePassPlan::prepare(
            plan.root_flags(),
            plan.groups()[0].flags(),
            plan.draws()[0].class(),
            material,
        );
        assert_eq!(passes.passes().len(), 1);
        let pipeline =
            renderer.prepare_world_model_pipeline(passes.is_unified(), passes.passes()[0])?;
        let draw = renderer.prepare_world_model_draw(
            mesh,
            pipeline,
            textures,
            &plan,
            0,
            0,
            Mat4::IDENTITY,
            1.0,
            Vec3::ZERO,
        )?;
        for ambient in [0.0, 0.25, 0.75] {
            let light = Vec3::splat(ambient);
            let fog = Vec4::new(10.0, 100.0, 0.0, 1.0);
            let scene = WorldFrameScene::new(
                TerrainSceneUniform::new(
                    camera.projection(),
                    camera.view(),
                    light,
                    Vec3::ZERO,
                    Vec3::Z,
                ),
                WorldModelSceneUniform::new(
                    camera.projection(),
                    camera.view(),
                    camera.camera().position(),
                    light,
                    Vec3::ZERO,
                    Vec3::Z,
                    fog,
                ),
                M2SceneUniform::new(
                    camera.projection(),
                    camera.view(),
                    camera.camera().position(),
                    light,
                    Vec3::ZERO,
                    Vec3::Z,
                    fog,
                    Vec3::ZERO,
                    [M2LocalLightState::disabled(); 4],
                ),
            );
            renderer.request_frame_capture()?;
            renderer.present_world_frame(scene, &[], &[], &[draw], &[], &[], &[], &[], &[], &[])?;
            let frame = renderer
                .take_captured_frame()?
                .ok_or("missing WMO lighting capture")?;
            let pixel = &frame.rgba8()[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 3];
            // 7C8560 supplies 127/255 for MapObj or zero for MapObjU; the
            // original diffuse shaders multiply by 2 after their lighting lane.
            let expected = ambient * if root_flags & 2 == 0 { 254.0 } else { 255.0 };
            assert!(
                pixel[0] == 0 && pixel[2] == 0 && (f32::from(pixel[1]) - expected).abs() <= 1.0,
                "flags {root_flags}, ambient {ambient}: {pixel:?}, expected {expected}"
            );
        }
    }
    Ok(())
}

/// Finds a controlled top-level WMO fixture chunk for targeted field edits.
fn chunk_mut<'a>(bytes: &'a mut [u8], magic: &[u8; 4]) -> Result<&'a mut [u8], Box<dyn Error>> {
    let mut cursor = 0;
    while cursor + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into()?) as usize;
        if &bytes[cursor..cursor + 4] == magic {
            return Ok(&mut bytes[cursor + 8..cursor + 8 + size]);
        }
        cursor += 8 + size;
    }
    Err("missing WMO fixture chunk".into())
}

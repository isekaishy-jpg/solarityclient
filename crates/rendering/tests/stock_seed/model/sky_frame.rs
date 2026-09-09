//! GPU coverage of native sky M2 queue order and shared frame-slot storage.

#![allow(unsafe_code)]
use super::*;
use solarity_rendering::{
    TerrainSceneUniform, WorldCamera, WorldFrameScene, WorldModelSceneUniform, WorldSkyDome,
    WorldSkyFrame, WorldSkyModelFrame,
};

#[test]
fn sky_models_share_bone_storage_and_keep_native_compositor_order() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("SkyModelProbe", 1)?;
    let vertices = m2_array_offset(&bytes, 0x3c)?;
    for (i, position) in [[-1_f32, -1., 0.5], [3., -1., 0.5], [-1., 3., 0.5]]
        .into_iter()
        .enumerate()
    {
        for (axis, value) in position.into_iter().enumerate() {
            let at = vertices + i * 48 + axis * 4;
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    // M2 material table: alpha blend, unlit, unfogged, two-sided, no depth writes.
    let materials = m2_array_offset(&bytes, 0x70)?;
    bytes[materials..materials + 2].copy_from_slice(&0x17_u16.to_le_bytes());
    bytes[materials + 2..materials + 4].copy_from_slice(&2_u16.to_le_bytes());
    bytes[materials + 4..materials + 6].copy_from_slice(&0x7_u16.to_le_bytes());
    bytes[materials + 6..materials + 8].copy_from_slice(&0_u16.to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Sky.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Sky00.skin",
            bytes: &skin,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new("Sky.m2")?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let draw_index = plan
        .draws()
        .iter()
        .position(|draw| draw.material().blend_mode() == solarity_asset::M2BlendMode::Alpha)
        .ok_or("no alpha batch")?;
    let draw = &plan.draws()[draw_index];
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity sky model queues", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: SDL transfers surface ownership and the window outlives the renderer.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mesh = renderer.upload_m2_mesh(&plan)?;
    let shader = M2ShaderPlan::resolve(&model, draw)?;
    let permutation = M2ShaderPermutation::resolve(
        draw,
        M2LocalLightCount::Zero,
        M2ShadowPermutation::Disabled,
        M2ShadowFiltering::Direct,
    );
    let pipeline = renderer.prepare_m2_pipeline(shader, permutation)?;
    let opaque_index = plan
        .draws()
        .iter()
        .position(|draw| draw.material().blend_mode() == solarity_asset::M2BlendMode::Opaque)
        .ok_or("no opaque batch")?;
    let opaque_draw = &plan.draws()[opaque_index];
    let opaque_shader = M2ShaderPlan::resolve(&model, opaque_draw)?;
    let opaque_permutation = M2ShaderPermutation::resolve(
        opaque_draw,
        M2LocalLightCount::Zero,
        M2ShadowPermutation::Disabled,
        M2ShadowFiltering::Direct,
    );
    let opaque_pipeline = renderer.prepare_m2_pipeline(opaque_shader, opaque_permutation)?;
    let white = renderer.upload_stock_m2_white()?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let textures = renderer.prepare_m2_texture_sets(&[M2TextureSet::Two(
        [M2SampledTexture::new(white, sampler); 2],
    )])?[0];
    let mut gradient = WorldSkyDome::new();
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::Z, Vec3::Y, 1000.).frame(1.)?;
    gradient.update_colors(
        [Vec3::new(0., 0., 32. / 255.); 5],
        Vec3::new(0., 0., 32. / 255.),
        0.,
        0.,
        camera,
    );
    let depth = Mat4::from_cols(
        Vec4::X,
        Vec4::Y,
        Vec4::new(0., 0., 0.0009765625, 0.),
        Vec4::new(0., 0., 0.999_023_44, 1.),
    );
    let uniform = |projection| {
        M2SceneUniform::new(
            projection,
            Mat4::IDENTITY,
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::ZERO,
            Vec3::ZERO,
            [M2LocalLightState::disabled(); 4],
        )
    };
    for (case, prefix) in [0_usize, 3, 12, 0, 9, 3, 24, 0, 3, 12, 3, 3]
        .into_iter()
        .enumerate()
    {
        let mode = case % 6;
        let material = |color| {
            M2MaterialUniform::new(
                Mat4::IDENTITY,
                [Mat4::IDENTITY; 2],
                Mat4::IDENTITY,
                color,
                Vec4::ZERO,
                Vec4::ZERO,
            )
        };
        let red = renderer.prepare_m2_draw(
            mesh,
            pipeline,
            textures,
            &plan,
            draw_index,
            false,
            material(Vec4::new(1., 0., 0., 0.5)),
            prefix as u32,
            0,
        )?;
        let green = renderer.prepare_m2_draw(
            mesh,
            pipeline,
            textures,
            &plan,
            draw_index,
            false,
            material(Vec4::new(0., 1., 0., 0.25)),
            prefix as u32,
            0,
        )?;
        let blue = renderer
            .prepare_m2_draw(
                mesh,
                pipeline,
                textures,
                &plan,
                draw_index,
                false,
                material(Vec4::new(0., 0., 1., 1.)),
                0,
                0,
            )?
            .with_scene_order(0);
        let red = if mode == 4 {
            renderer.prepare_m2_draw(
                mesh,
                opaque_pipeline,
                textures,
                &plan,
                opaque_index,
                false,
                material(Vec4::new(1., 0., 0., 1.)),
                prefix as u32,
                0,
            )?
        } else {
            red
        };
        let stars = if mode == 1 { vec![] } else { vec![red] };
        let boxes = if mode == 0 || mode == 5 {
            vec![]
        } else {
            vec![green]
        };
        // A distinct bone prefix must never be sampled by a sky packet.
        let world_bones = vec![
            if mode == 4 {
                Mat4::from_translation(Vec3::new(0., 0., 0.25))
            } else {
                Mat4::from_translation(Vec3::splat(1000.))
            };
            prefix
        ];
        let sky_bones = vec![Mat4::IDENTITY; red.required_bone_transforms() - prefix];
        let world_draws = if mode == 4 { vec![blue] } else { vec![] };
        let world_projection = if mode == 4 {
            Mat4::IDENTITY
        } else {
            Mat4::from_translation(Vec3::splat(1000.))
        };
        let mut scene = WorldFrameScene::new(
            TerrainSceneUniform::new(
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
            ),
            WorldModelSceneUniform::new(
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                Vec3::ZERO,
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
                Vec4::ZERO,
            ),
            uniform(world_projection),
        )
        .with_sky_models(WorldSkyModelFrame::new(
            uniform(depth),
            &sky_bones,
            &stars,
            &boxes,
        ));
        if mode == 3 || mode == 5 {
            scene = scene.with_sky(WorldSkyFrame::new(&gradient, camera));
        }
        renderer.request_frame_capture()?;
        let report = renderer.present_world_frame(
            scene,
            &world_bones,
            &[],
            &[],
            &world_draws,
            &[],
            &[],
            &[],
            &[],
            &[],
        )?;
        assert_eq!(report.sky_model_draw_count(), stars.len() + boxes.len());
        assert_eq!(
            report.bone_transform_count(),
            world_bones.len() + sky_bones.len()
        );
        let frame = renderer
            .take_captured_frame()?
            .ok_or("missing sky model capture")?;
        let expected = match mode {
            0 => [128, 0, 0],
            1 => [0, 64, 0],
            2 => [96, 64, 0],
            3 => [96, 64, 24],
            4 => [0, 0, 255],
            _ => [128, 0, 32],
        };
        for y in (8..56).step_by(8) {
            for x in (8..56).step_by(8) {
                let actual = rgba8_pixel(frame.rgba8(), 64, x, y);
                for channel in 0..3 {
                    assert!(
                        actual[channel].abs_diff(expected[channel]) <= 2,
                        "case {case}, pixel {x}/{y}: {actual:?} expected {expected:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

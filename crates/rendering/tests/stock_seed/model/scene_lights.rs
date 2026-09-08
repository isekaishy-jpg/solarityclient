//! GPU coverage of independent instance scenes across all three M2 queues.

#![allow(unsafe_code)]
use super::*;

#[test]
fn instance_lighting_grows_independently_of_mesh_draws_and_selects_each_gpu_queue()
-> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("SceneLights", 1)?;
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
    let materials = m2_array_offset(&bytes, 0x70)?;
    for index in 0..2 {
        // Lit, unfogged, two-sided, opaque: the pixel directly exposes lighting.
        bytes[materials + index * 4..materials + index * 4 + 2]
            .copy_from_slice(&6_u16.to_le_bytes());
        bytes[materials + index * 4 + 2..materials + index * 4 + 4]
            .copy_from_slice(&0_u16.to_le_bytes());
    }
    let particle = m2_array_offset(&bytes, 0x128)?;
    let flags = 0x0002_0010_u32;
    bytes[particle + 4..particle + 8].copy_from_slice(&flags.to_le_bytes());
    bytes[particle + 40] = 0;
    for relative in [0x178, 0x17c, 0x180, 0x184] {
        bytes[particle + relative..particle + relative + 4].copy_from_slice(&0_f32.to_le_bytes());
    }
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Lights.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Lights00.skin",
            bytes: &skin,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new("Lights.m2")?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let draw_index = plan
        .draws()
        .iter()
        .position(|draw| draw.batch().material_index == 0)
        .ok_or("mesh batch")?;
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity instance scene lights", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap owns the enabled SDL extensions; window outlives the renderer.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mesh = renderer.upload_m2_mesh(&plan)?;
    let draw = &plan.draws()[draw_index];
    let shader = M2ShaderPlan::resolve(&model, draw)?;
    let pipeline = renderer.prepare_m2_pipeline(
        shader,
        M2ShaderPermutation::resolve(
            draw,
            M2LocalLightCount::Four,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        ),
    )?;
    let white = renderer.upload_stock_m2_white()?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let texture = M2SampledTexture::new(white, sampler);
    let textures = renderer
        .prepare_m2_texture_sets(&[M2TextureSet::Two([texture; 2]), M2TextureSet::One(texture)])?;
    let mesh_draw = renderer.prepare_m2_draw(
        mesh,
        pipeline,
        textures[0],
        &plan,
        draw_index,
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
    let bones = vec![Mat4::IDENTITY; mesh_draw.required_bone_transforms()];
    let camera = WorldCamera::orthographic(
        Vec3::new(0., 0., 8.),
        Vec3::ZERO,
        Vec3::Y,
        [-1., 1.],
        [-1., 1.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let emitter = &model.animations().particles()[0];
    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 0., 0.),
    )?;
    let particles = M2ParticleMeshPlan::prepare(
        emitter,
        pose,
        &[M2ParticleState::new(0., Vec3::ZERO, Vec3::ZERO, 0x2483)?],
        camera,
        1.,
    )?;
    let particle_pipeline = renderer.prepare_m2_particle_pipeline(0, flags)?;
    let particle_draw = renderer.prepare_m2_particle_draw(
        particle_pipeline,
        textures[1],
        0,
        flags,
        M2EffectOrder::new(0, 0),
        0,
        0,
        &particles,
    )?;
    let ribbon = &model.animations().ribbons()[0];
    let ribbon_pose =
        M2RibbonPose::sample(model.animations(), ribbon, M2AnimationClock::new(0, 0., 0.))?;
    let mut trail = M2RibbonTrail::new(ribbon)?;
    trail.advance(
        0.05,
        M2RibbonControlPoint::new(Vec3::new(-1., 0., 0.), Vec3::Y, Vec3::X),
        ribbon_pose,
    )?;
    trail.advance(
        0.05,
        M2RibbonControlPoint::new(Vec3::new(1., 0., 0.), Vec3::Y, Vec3::X),
        ribbon_pose,
    )?;
    let ribbon_mesh = M2RibbonMeshPlan::prepare(ribbon, &trail)?;
    let ribbon_material = model.materials()[0];
    let ribbon_pipeline = renderer.prepare_m2_ribbon_pipeline(ribbon_material)?;
    let ribbon_draw = renderer.prepare_m2_ribbon_draw(
        ribbon_pipeline,
        textures[1],
        ribbon_material,
        M2EffectOrder::new(0, 0),
        0,
        &ribbon_mesh,
    )?;
    let base = |projection, color: Vec3| {
        let mut lights = [M2LocalLightState::disabled(); 4];
        lights[0] = M2LocalLightState::directional(Vec3::Z, color, Vec3::ZERO);
        M2SceneUniform::new(
            projection,
            camera.view(),
            camera.camera().position(),
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::Z,
            Vec4::new(0., 100., 0., 0.),
            Vec3::ZERO,
            lights,
        )
    };
    let hidden = base(Mat4::from_translation(Vec3::splat(1000.)), Vec3::ZERO);
    // Each frame has at most one draw but up to 96 independent scenes. Particle
    // and ribbon-only frames must grow scene storage and validate their index.
    for (frame_index, count) in [1, 3, 20, 2, 64, 1, 96, 4, 1, 48, 2, 3]
        .into_iter()
        .enumerate()
    {
        for queue in 0..3 {
            let color = if frame_index % 2 == 0 {
                Vec3::X
            } else {
                Vec3::Y
            };
            let mut scenes = vec![hidden; count];
            scenes[count - 1] = base(
                if queue == 0 {
                    Mat4::IDENTITY
                } else {
                    camera.view_projection()
                },
                color,
            );
            let scene = WorldFrameScene::new(
                TerrainSceneUniform::new(Mat4::IDENTITY, Vec3::ZERO, Vec3::ZERO, Vec3::Z),
                WorldModelSceneUniform::new(
                    Mat4::IDENTITY,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    Vec3::ZERO,
                    Vec3::Z,
                    Vec4::ZERO,
                ),
                hidden,
            )
            .with_m2_instance_scenes(&scenes);
            let index = Some((count - 1) as u32);
            let meshes = [mesh_draw.with_scene_index(index)];
            let particle_draws = [particle_draw.with_scene_index(index)];
            let ribbon_draws = [ribbon_draw.with_scene_index(index)];
            renderer.request_frame_capture()?;
            renderer.present_world_frame(
                scene,
                &bones,
                &[],
                &[],
                if queue == 0 { &meshes } else { &[] },
                particles.vertices(),
                particles.indices(),
                if queue == 1 { &particle_draws } else { &[] },
                ribbon_mesh.vertices(),
                if queue == 2 { &ribbon_draws } else { &[] },
            )?;
            let captured = renderer
                .take_captured_frame()?
                .ok_or("instance lighting capture")?;
            let pixel = rgba8_pixel(captured.rgba8(), 64, 32, 32);
            let expected = match queue {
                0 => (color * 255.).to_array(),
                1 => {
                    let bgra = particles.vertices()[0].color_bgra();
                    (Vec3::new(f32::from(bgra[2]), f32::from(bgra[1]), f32::from(bgra[0])) * color)
                        .to_array()
                }
                _ => {
                    let bgra = ribbon_mesh.vertices()[0].color_bgra();
                    [f32::from(bgra[2]), f32::from(bgra[1]), f32::from(bgra[0])]
                }
            };
            for channel in 0..3 {
                assert!(
                    (f32::from(pixel[channel]) - expected[channel]).abs() <= 1.,
                    "frame {frame_index}, queue {queue}, scene {index:?}: {pixel:?}, expected {expected:?}"
                );
            }
            let meshes = [mesh_draw.with_scene_index(Some(count as u32))];
            let particle_draws = [particle_draw.with_scene_index(Some(count as u32))];
            let ribbon_draws = [ribbon_draw.with_scene_index(Some(count as u32))];
            assert!(matches!(
                renderer.present_world_frame(
                    scene,
                    &bones,
                    &[],
                    &[],
                    if queue == 0 { &meshes } else { &[] },
                    particles.vertices(),
                    particles.indices(),
                    if queue == 1 { &particle_draws } else { &[] },
                    ribbon_mesh.vertices(),
                    if queue == 2 { &ribbon_draws } else { &[] },
                ),
                Err(VulkanError::WorldFrameCapacity)
            ));
        }
    }
    Ok(())
}

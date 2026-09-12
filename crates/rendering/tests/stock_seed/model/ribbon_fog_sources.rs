//! Common fog publication by preceding model, particle and WMO packets.

use super::*;
use solarity_asset::DecodedWorldModel;
use solarity_rendering::{
    WorldModelBaseMip, WorldModelMaterialState, WorldModelMeshPlan, WorldModelSampledTexture,
    WorldModelSurfacePassPlan, WorldModelTextureFiltering, WorldModelTextureSet,
};

#[test]
#[allow(unsafe_code)] // The hidden SDL surface transfers to the renderer.
fn ribbon_inherits_fog_from_mesh_particle_and_world_model_submissions() -> Result<(), Box<dyn Error>>
{
    let mut bytes = render_m2_bytes("Ribbon.blp", 1)?;
    let materials = bytes.len();
    for flags in [5u16, 7] {
        for blend in 0u16..7 {
            bytes.extend(flags.to_le_bytes());
            bytes.extend(blend.to_le_bytes());
        }
    }
    set_render_header_array(&mut bytes, 0x70, 14, materials)?;
    let ribbon = m2_array_offset(&bytes, 0x120)?;
    append_render_track(
        &mut bytes,
        ribbon + 36,
        &[0],
        &render_f32_values(&[0.25, 0.5, 0.75]),
        12,
    )?;
    append_render_track(
        &mut bytes,
        ribbon + 56,
        &[0],
        &render_i16_values(&[12_288]),
        2,
    )?;
    for offset in [76, 96] {
        append_render_track(
            &mut bytes,
            ribbon + offset,
            &[0],
            &render_f32_values(&[1.0]),
            4,
        )?;
    }
    bytes[ribbon + 124..ribbon + 128].copy_from_slice(&0f32.to_le_bytes());
    let skin = render_skin_bytes()?;
    let texture = solid_raw3_blp(1, 1, &[0xc0cc_9966]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Ribbon.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Ribbon00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Ribbon.blp",
            bytes: &texture,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut store, &AssetPath::new("Ribbon.m2")?)?;
    let texture = BlpTextureSource::load(&mut store, &AssetPath::new("Ribbon.blp")?)?;
    let emitter = &model.animations().ribbons()[0];
    let pose = M2RibbonPose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 0., 0.),
    )?;
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Ribbon material", 128, 128)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The window outlives the renderer, which solely owns the surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (128, 128), 0) }?;
    let texture = renderer.upload_blp_texture(&texture, BlpColorSpace::Linear)?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let texture_set = renderer
        .prepare_m2_texture_sets(&[M2TextureSet::One(M2SampledTexture::new(texture, sampler))])?[0];
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
    let mut trail = M2RibbonTrail::new(emitter)?;
    for x in [-0.8, 0.8] {
        trail.advance(
            0.05,
            M2RibbonControlPoint::new(Vec3::new(x, 0., -12.), Vec3::Y * 0.8, Vec3::X),
            pose,
        )?;
    }
    let mesh = M2RibbonMeshPlan::prepare(emitter, &trail)?;
    let mut ribbons = Vec::new();
    for (material_index, first) in [(7, true), (0, false), (5, true)] {
        let material = model.materials()[material_index];
        let pipeline = renderer.prepare_m2_ribbon_pipeline(material)?;
        ribbons.push(
            renderer
                .prepare_m2_ribbon_draw(
                    pipeline,
                    texture_set,
                    material,
                    M2EffectOrder::new(0, 0),
                    0,
                    &mesh,
                )?
                .with_first_material_pass(first)
                .with_scene_order(1),
        );
    }
    let color = Vec3::new(0.213, 0.421, 0.637);
    let wrong_color = Vec3::new(0.8, 0.1, 0.3);
    let fog = Vec4::new(2., 22., 0., 2.);
    let m2_scene = |parameters, color| {
        M2SceneUniform::new(
            camera.projection(),
            camera.view(),
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Y,
            parameters,
            color,
            [M2LocalLightState::disabled(); 4],
        )
    };
    let instance_scenes = [m2_scene(fog, color)];
    let scene = WorldFrameScene::new(
        TerrainSceneUniform::new(
            camera.projection(),
            camera.view(),
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Y,
        ),
        WorldModelSceneUniform::new(
            camera.projection(),
            camera.view(),
            Vec3::ZERO,
            Vec3::ONE,
            Vec3::ZERO,
            Vec3::Y,
            fog,
        ),
        m2_scene(Vec4::new(5., 45., 0., 3.), wrong_color),
    )
    .with_m2_instance_scenes(&instance_scenes);
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let model_mesh = renderer.upload_m2_mesh(&plan)?;
    let pipeline = renderer.prepare_m2_pipeline(
        M2ShaderPlan::resolve(&model, &plan.draws()[0])?,
        M2ShaderPermutation::resolve(
            &plan.draws()[0],
            M2LocalLightCount::Zero,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        ),
    )?;
    let model_textures = renderer.prepare_m2_texture_sets(&[M2TextureSet::Two(
        [M2SampledTexture::new(texture, sampler); 2],
    )])?[0];
    let offscreen = Mat4::from_translation(Vec3::new(500., 0., -12.));
    let model_draw = renderer
        .prepare_m2_draw(
            model_mesh,
            pipeline,
            model_textures,
            &plan,
            0,
            false,
            M2MaterialUniform::new(
                offscreen,
                [Mat4::IDENTITY; 2],
                Mat4::IDENTITY,
                Vec4::ONE,
                color.extend(0.),
                Vec4::ZERO,
            ),
            0,
            0,
        )?
        .with_scene_index(Some(0))
        .with_scene_order(0);
    let bones = vec![Mat4::IDENTITY; model_draw.required_bone_transforms()];
    let particle_emitter = &model.animations().particles()[0];
    let particle_pose = M2ParticlePose::sample(
        model.animations(),
        particle_emitter,
        M2AnimationClock::new(0, 0., 0.),
    )?;
    let particle_mesh = M2ParticleMeshPlan::prepare(
        particle_emitter,
        particle_pose,
        &[M2ParticleState::new(
            0.,
            Vec3::new(500., 0., -12.),
            Vec3::ZERO,
            0x2483,
        )?],
        camera,
        1.,
    )?;
    let particle_pipeline =
        renderer.prepare_m2_particle_pipeline(M2MaterialState::from_particle(0, 1))?;
    let particle_draw = renderer
        .prepare_m2_particle_draw(
            particle_pipeline,
            texture_set,
            0,
            1,
            1.,
            M2EffectOrder::new(0, 0),
            0,
            0,
            &particle_mesh,
        )?
        .with_scene_index(Some(0))
        .with_scene_order(0);
    let green = renderer.upload_stock_world_model_green()?;
    let native: Vec<Vec<f32>> = include_str!("../../fixtures/ribbon_fog_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    // Ordinary with/without MOCV, then unified interior/exterior, including
    // nontransition MOMT-unfogged cases whose native callback still publishes.
    for (source, root_flags, group_flags, material_flags) in [
        (0, 0u32, 4u32, 4u32),
        (1, 0, 4, 4),
        (2, 0, 4, 4),
        (2, 0, 8, 4),
        (2, 2, 4, 4),
        (2, 2, 8, 4),
        (2, 2, 4, 6),
        (2, 2, 8, 6),
        (2, 0, 4, 6),
    ] {
        let mut world_draws = Vec::new();
        if source == 2 {
            let mut root = crate::world_model::root_fixture();
            chunk_mut(&mut root, b"DHOM")?[60..64].copy_from_slice(&root_flags.to_le_bytes());
            let material = chunk_mut(&mut root, b"TMOM")?;
            material[0..4].copy_from_slice(&material_flags.to_le_bytes());
            material[8..12].fill(0);
            let original = crate::world_model::group_fixture();
            let mut group = original[..88].to_vec();
            let mut cursor = 88;
            while cursor < original.len() {
                let size =
                    u32::from_le_bytes(original[cursor + 4..cursor + 8].try_into()?) as usize;
                if group_flags & 4 != 0 || &original[cursor..cursor + 4] != b"VCOM" {
                    group.extend_from_slice(&original[cursor..cursor + 8 + size]);
                }
                cursor += 8 + size;
            }
            let size = (group.len() - 20) as u32;
            group[16..20].copy_from_slice(&size.to_le_bytes());
            group[28..32].copy_from_slice(&group_flags.to_le_bytes());
            group[60..64].fill(0);
            group[64..66].copy_from_slice(&1u16.to_le_bytes());
            let path = format!("Fog{root_flags}_{group_flags}_{material_flags}");
            let root_path = format!("{path}.wmo");
            let group_path = format!("{path}_000.wmo");
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
            let textures =
                renderer.prepare_world_model_texture_sets(&[WorldModelTextureSet::One(
                    WorldModelSampledTexture::new(green, sampler),
                )])?[0];
            let passes = WorldModelSurfacePassPlan::prepare(
                plan.root_flags(),
                plan.groups()[0].flags(),
                plan.draws()[0].class(),
                material,
            );
            for (pass_index, pass) in passes.passes().iter().enumerate() {
                let pipeline = renderer.prepare_world_model_pipeline(passes.is_unified(), *pass)?;
                world_draws.push(
                    renderer
                        .prepare_world_model_draw(
                            mesh,
                            pipeline,
                            textures,
                            &plan,
                            0,
                            pass_index,
                            offscreen,
                            1.,
                            if group_flags & 8 != 0 {
                                wrong_color
                            } else {
                                color
                            },
                        )?
                        .with_outdoor_fog_color(color),
                );
            }
        }
        // Reset to native modulation-white so a missing publication cannot pass
        // merely because the preceding source left the same scene color behind.
        renderer.present_world_frame(
            scene,
            &bones,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            mesh.vertices(),
            &[ribbons[2].with_scene_index(Some(0))],
        )?;
        renderer.request_frame_capture()?;
        renderer.present_world_frame(
            scene,
            &bones,
            &[],
            &world_draws,
            if source == 0 {
                std::slice::from_ref(&model_draw)
            } else {
                &[]
            },
            particle_mesh.vertices(),
            particle_mesh.indices(),
            if source == 1 {
                std::slice::from_ref(&particle_draw)
            } else {
                &[]
            },
            mesh.vertices(),
            &ribbons[..2],
        )?;
        let capture = renderer
            .take_captured_frame()?
            .ok_or("source fog capture")?;
        let pixel = &capture.rgba8()[(64 * 128 + 64) * 4..][..4];
        let blend = if source == 2 && root_flags == 0 && material_flags & 2 != 0 {
            5.
        } else {
            0.
        };
        let expected = native
            .iter()
            .find(|row| row[0] == 5. && row[1] == blend && row[6] == 2. && row[10] == 12.)
            .ok_or("native fog row")?;
        for (actual, expected) in pixel.iter().zip(&expected[11..15]) {
            assert!(
                (f32::from(*actual) - expected).abs() <= 1.,
                "source={source}, root={root_flags}, group={group_flags}, material={material_flags}: {pixel:?}"
            );
        }
    }
    Ok(())
}

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

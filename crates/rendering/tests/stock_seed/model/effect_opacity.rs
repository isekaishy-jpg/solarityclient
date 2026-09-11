//! Original common/ribbon material state and actual faded particle pixels.

use super::*;
use solarity_rendering::M2BlendFactor;

#[test]
fn effect_materials_match_original_common_and_ribbon_submission() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Effect", 1)?;
    let materials = bytes.len();
    for flags in [0u16, 4, 8, 16, 28] {
        for blend in 0u16..7 {
            bytes.extend(flags.to_le_bytes());
            bytes.extend(blend.to_le_bytes());
        }
    }
    set_render_header_array(&mut bytes, 0x70, 35, materials)?;
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Effect.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Effect00.skin",
            bytes: &skin,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut store, &AssetPath::new("Effect.m2")?)?;
    let ribbon_compiler = solarity_rendering::M2RibbonSpirvCompiler::new()?;
    let mut count = 0;
    for line in include_str!("../../fixtures/effect_material_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split_whitespace().collect();
        let flags: u16 = fields[0].parse()?;
        let blend: u16 = fields[1].parse()?;
        let pass: u32 = fields[2].parse()?;
        let values: Vec<_> = fields[3..]
            .iter()
            .map(|value| u32::from_str_radix(value, 16))
            .collect::<Result<_, _>>()?;
        let alpha = f32::from_bits(values[0]);
        let authored = M2MaterialState::from_material(
            *model
                .materials()
                .iter()
                .find(|material| material.flags() == flags && material.blend_mode() as u16 == blend)
                .ok_or("material")?,
        );
        let common = if pass != 0 && !authored.blend_enabled() {
            authored.with_runtime_alpha_fade()
        } else {
            authored
        };
        let ribbon = ribbon_compiler.compile(authored)?;
        for (state, alpha_reference, expected) in [
            (common, common.alpha_reference(alpha), &values[1..6]),
            (
                ribbon.material(),
                f32::from_bits(ribbon.fragment_specialization()[0]),
                &values[6..11],
            ),
        ] {
            assert_eq!(
                [
                    gx_blend(state),
                    u32::from(state.depth_write_enabled()),
                    u32::from(state.depth_test_enabled()),
                    u32::from(state.cull_enabled())
                ],
                expected[..4],
                "{line}",
            );
            assert!(
                (alpha_reference - f32::from_bits(expected[4])).abs() < 1e-7,
                "{line}"
            );
        }
        count += 1;
    }
    assert_eq!(count, 315);
    Ok(())
}

fn gx_blend(state: M2MaterialState) -> u32 {
    if !state.blend_enabled() {
        return u32::from(state.blend_mode() == M2BlendMode::AlphaKey);
    }
    match (state.source_blend(), state.destination_blend()) {
        (M2BlendFactor::SourceAlpha, M2BlendFactor::OneMinusSourceAlpha) => 2,
        (M2BlendFactor::One, M2BlendFactor::One) => 10,
        (M2BlendFactor::SourceAlpha, M2BlendFactor::One) => 3,
        (M2BlendFactor::DestinationColor, M2BlendFactor::Zero) => 4,
        (M2BlendFactor::DestinationColor, M2BlendFactor::SourceColor) => 5,
        other => panic!("unexpected native blend {other:?}"),
    }
}

/// A faded front card still writes depth for opaque/alpha-key particles. Its
/// later opaque backing card must remain hidden; alpha-blended particles keep
/// their authored no-write policy. Alpha-key thresholds scale with owner alpha.
#[test]
#[allow(unsafe_code)] // The hidden SDL surface transfers to the renderer.
fn faded_particle_pixels_keep_native_alpha_test_blend_and_depth() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Effect.blp", 1)?;
    let offset = m2_array_offset(&bytes, 0x128)?;
    let flags = 0x0002_0019u32; // Unlit, unfogged, ordinary head card.
    bytes[offset + 4..offset + 8].copy_from_slice(&flags.to_le_bytes());
    bytes[offset + 45] = 0;
    for relative in [0x178, 0x17c, 0x180, 0x184] {
        bytes[offset + relative..offset + relative + 4].copy_from_slice(&0f32.to_le_bytes());
    }
    let skin = render_skin_bytes()?;
    let low_alpha = solid_raw3_blp(1, 1, &[0x80ff_ffff]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Effect.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Effect00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Low.blp",
            bytes: &low_alpha,
        },
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut store, &AssetPath::new("Effect.m2")?)?;
    let low_alpha = BlpTextureSource::load(&mut store, &AssetPath::new("Low.blp")?)?;
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Particle opacity", 128, 128)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The window outlives the renderer, which solely owns the surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (128, 128), 0) }?;
    let white = renderer.upload_stock_m2_white()?;
    let low = renderer.upload_blp_texture(&low_alpha, BlpColorSpace::Linear)?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let textures = renderer.prepare_m2_texture_sets(
        &[white, low].map(|texture| M2TextureSet::One(M2SampledTexture::new(texture, sampler))),
    )?;
    let emitter = &model.animations().particles()[0];
    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 0., 0.),
    )?;
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 100.).frame(1.)?;
    let front = M2ParticleMeshPlan::prepare(
        emitter,
        pose,
        &[M2ParticleState::new(0., Vec3::X * 10., Vec3::ZERO, 0x2483)?],
        camera,
        0.5,
    )?;
    let back = M2ParticleMeshPlan::prepare(
        emitter,
        pose,
        &[M2ParticleState::new(0., Vec3::X * 12., Vec3::ZERO, 0x2483)?],
        camera,
        1.,
    )?;
    assert_eq!(front.vertices()[0].color_bgra(), [0, 0, 255, 128]);
    let vertices: Vec<_> = front
        .vertices()
        .iter()
        .chain(back.vertices())
        .copied()
        .collect();
    let indices: Vec<_> = front
        .indices()
        .iter()
        .chain(back.indices())
        .copied()
        .collect();
    let back_pipeline =
        renderer.prepare_m2_particle_pipeline(M2MaterialState::from_particle(0, flags))?;
    let backing = renderer
        .prepare_m2_particle_draw(
            back_pipeline,
            textures[0],
            0,
            flags,
            1.,
            M2EffectOrder::new(0, 1),
            front.vertices().len() as u32,
            front.indices().len() as u32,
            &back,
        )?
        .with_scene_order(1);
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
            Vec4::ZERO,
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
    );
    for (blend, texture, expected) in [(0, 0, 128u8), (1, 0, 128), (1, 1, 255), (2, 0, 255)] {
        let material = M2MaterialState::from_particle(blend, flags);
        let material = if material.blend_enabled() {
            material
        } else {
            material.with_runtime_alpha_fade()
        };
        let pipeline = renderer.prepare_m2_particle_pipeline(material)?;
        let draw = renderer
            .prepare_m2_particle_draw(
                pipeline,
                textures[texture],
                blend,
                flags,
                0.5,
                M2EffectOrder::new(0, 0),
                0,
                0,
                &front,
            )?
            .with_scene_order(0);
        renderer.request_frame_capture()?;
        renderer.present_world_frame(
            scene,
            &[],
            &[],
            &[],
            &[],
            &vertices,
            &indices,
            &[draw, backing],
            &[],
            &[],
        )?;
        let capture = renderer.take_captured_frame()?.ok_or("capture")?;
        let pixel = &capture.rgba8()[(64 * 128 + 64) * 4..][..3];
        assert!(
            pixel[0].abs_diff(expected) <= 1 && pixel[1] == 0 && pixel[2] == 0,
            "blend={blend}, texture={texture}: {pixel:?}"
        );
    }
    renderer.shutdown()?;
    Ok(())
}

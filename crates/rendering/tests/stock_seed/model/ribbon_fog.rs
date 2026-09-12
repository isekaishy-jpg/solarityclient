//! Serial native ribbon fog registers and original BLS pixel captures.

use super::*;

#[test]
#[allow(unsafe_code)] // The hidden SDL surface transfers to the renderer.
fn ribbon_fog_matches_native_serial_state_and_pixels() -> Result<(), Box<dyn Error>> {
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
    for eye in [Vec3::ZERO, Vec3::new(7., -4., 2.)] {
        let direction = if eye == Vec3::ZERO {
            Vec3::NEG_Z
        } else {
            Vec3::X
        };
        let camera = WorldCamera::orthographic(
            eye,
            eye + direction,
            Vec3::Y,
            [-1., 1.],
            [-1., 1.],
            0.1,
            100.,
        )
        .frame(1.)?;
        let world = camera.view().inverse();
        let mut count = 0;
        for (case, line) in include_str!("../../fixtures/ribbon_fog_native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .enumerate()
        {
            if case == 0 && eye != Vec3::ZERO {
                // Only the first camera starts with the renderer's zero bank.
                continue;
            }
            let row = line
                .split_whitespace()
                .map(str::parse::<f32>)
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(row.len(), 22);
            let mut trail = M2RibbonTrail::new(emitter)?;
            for x in [-0.8, 0.8] {
                trail.advance(
                    0.05,
                    M2RibbonControlPoint::new(
                        world.transform_point3(Vec3::new(x, 0., -row[10])),
                        world.transform_vector3(Vec3::Y * 0.8),
                        world.transform_vector3(Vec3::X),
                    ),
                    pose,
                )?;
            }
            let mesh = M2RibbonMeshPlan::prepare(emitter, &trail)?;
            let mut draws = Vec::new();
            // The opaque final pass exposes the first pass's retained fog color
            // independently of its framebuffer blend equation.
            for (flags, blend, first) in [(row[0], row[1], true), (row[3], 0., false)] {
                let material = *model
                    .materials()
                    .iter()
                    .find(|m| {
                        f32::from(m.flags()) == flags && m.blend_mode() as u16 == blend as u16
                    })
                    .ok_or("fog material")?;
                let pipeline = renderer.prepare_m2_ribbon_pipeline(material)?;
                draws.push(
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
                        .with_scene_order(0),
                );
            }
            let fog = Vec4::new(row[4], row[5], 0., row[6]);
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
                    eye,
                    Vec3::ONE,
                    Vec3::ZERO,
                    Vec3::Y,
                    fog,
                ),
                M2SceneUniform::new(
                    camera.projection(),
                    camera.view(),
                    eye,
                    Vec3::ONE,
                    Vec3::ZERO,
                    Vec3::Y,
                    fog,
                    Vec3::new(row[7], row[8], row[9]),
                    [M2LocalLightState::disabled(); 4],
                )
                .with_fog_enabled(row[2] != 0.),
            );
            renderer.request_frame_capture()?;
            renderer.present_world_frame(
                scene,
                &[Mat4::IDENTITY],
                &[],
                &[],
                &[],
                &[],
                &[],
                &[],
                mesh.vertices(),
                &draws,
            )?;
            let capture = renderer
                .take_captured_frame()?
                .ok_or("ribbon fog capture")?;
            let pixel = &capture.rgba8()[(64 * 128 + 64) * 4..][..4];
            for (actual, expected) in pixel.iter().zip(&row[11..15]) {
                assert!(
                    (f32::from(*actual) - expected).abs() <= 1.,
                    "eye={eye:?}: {line}: {pixel:?}"
                );
            }
            count += 1;
        }
        assert_eq!(count, if eye == Vec3::ZERO { 72 } else { 71 });
    }
    Ok(())
}

//! Original Particle_Unlit shader color and authored ribbon culling.

use super::*;

#[test]
#[allow(unsafe_code)] // The hidden SDL surface transfers to the renderer.
fn ribbon_pixels_match_original_color_variants_and_material_culling() -> Result<(), Box<dyn Error>>
{
    let mut bytes = render_m2_bytes("Ribbon.blp", 1)?;
    let materials = bytes.len();
    for flags in [0u16, 1, 4, 5] {
        bytes.extend(flags.to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
    }
    set_render_header_array(&mut bytes, 0x70, 4, materials)?;
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
    let mut trail = M2RibbonTrail::new(emitter)?;
    for x in [-0.8, 0.8] {
        trail.advance(
            0.05,
            M2RibbonControlPoint::new(Vec3::new(x, 0., 0.5), Vec3::Y * 0.8, Vec3::X),
            pose,
        )?;
    }
    let mesh = M2RibbonMeshPlan::prepare(emitter, &trail)?;
    assert!(mesh.vertices().len() >= 4);
    assert!(
        mesh.vertices()
            .iter()
            .all(|v| v.color_bgra() == [191, 128, 64, 96])
    );
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
    let fog = Vec4::new(0., 100., 0., 1.);
    let mut cases = Vec::new();
    for line in include_str!("../../fixtures/ribbon_shader_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        cases.push((line, row));
    }
    for line in include_str!("../../fixtures/ribbon_shadow_native.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let native = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(&native[4..7], &[3, 0, 0], "{line}");
        assert!(native[7] < 10 && matches!(native[8], 0 | 4), "{line}");
        // Local-light aliases still select the even/odd Color_T1 rule;
        // previous shadow state is overwritten by the native ribbon element.
        let mut row = cases[..4]
            .iter()
            .find(|(_, row)| row[0] == native[0])
            .ok_or("native ribbon material")?
            .1
            .clone();
        row[2] = native[7] % 2;
        row[4..8].copy_from_slice(&native[9..13]);
        cases.push((line, row));
    }
    assert_eq!(cases.len(), 28);
    for (line, row) in cases {
        let material = *model
            .materials()
            .iter()
            .find(|m| u32::from(m.flags()) == row[0])
            .ok_or("material")?;
        let state = M2MaterialState::from_material(material);
        let program = M2RibbonSpirvCompiler::new()?.compile(state)?;
        assert_eq!(program.vertex_specialization()[0], row[2]);
        assert_eq!(u32::from(state.cull_enabled()), (row[1] >> 4) & 1);
        let pipeline = renderer.prepare_precompiled_m2_ribbon_pipeline(&program)?;
        let draw = renderer.prepare_m2_ribbon_draw(
            pipeline,
            texture_set,
            material,
            M2EffectOrder::new(0, 0),
            0,
            &mesh,
        )?;
        for light in [Vec3::ZERO, Vec3::new(3., 2., 4.)] {
            let scene = WorldFrameScene::new(
                TerrainSceneUniform::new(Mat4::IDENTITY, Mat4::IDENTITY, light, light, Vec3::Z),
                WorldModelSceneUniform::new(
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    Vec3::ZERO,
                    light,
                    light,
                    Vec3::Z,
                    fog,
                ),
                M2SceneUniform::new(
                    Mat4::IDENTITY,
                    Mat4::IDENTITY,
                    Vec3::ZERO,
                    light,
                    light,
                    Vec3::Z,
                    fog,
                    Vec3::ZERO,
                    [M2LocalLightState::disabled(); 4],
                ),
            );
            let mut visible = 0;
            for reverse in [false, true] {
                let mut vertices = mesh.vertices().to_vec();
                if reverse {
                    for edge in vertices.as_chunks_mut::<2>().0 {
                        edge.swap(0, 1);
                    }
                }
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
                    &vertices,
                    &[draw],
                )?;
                let capture = renderer.take_captured_frame()?.ok_or("ribbon capture")?;
                let pixel = &capture.rgba8()[(64 * 128 + 64) * 4..][..4];
                if pixel == [0, 0, 0, 255] {
                    assert!(state.cull_enabled(), "two-sided ribbon vanished: {line}");
                } else {
                    for (actual, expected) in pixel.iter().zip(&row[4..8]) {
                        assert!(
                            (i64::from(*actual) - i64::from(*expected)).abs() <= 1,
                            "{line}, reverse={reverse}, light={light:?}: {pixel:?}"
                        );
                    }
                    visible += 1;
                }
            }
            assert_eq!(visible, if state.cull_enabled() { 1 } else { 2 }, "{line}");
        }
    }
    Ok(())
}

//! GPU regression for local surface separation at distant world placements.

#![allow(unsafe_code)]
use super::*;

/// A local quarter-millimeter gap must survive projection at map-edge coordinates.
#[test]
fn projection_preserves_nearby_model_surfaces_at_large_world_coordinates()
-> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Projection", 1)?;
    let vertices = m2_array_offset(&bytes, 0x3c)?;
    for (index, position) in [[-2_f32, -2., 0.5], [6., -2., 0.5], [-2., 6., 0.5]]
        .into_iter()
        .enumerate()
    {
        for (axis, value) in position.into_iter().enumerate() {
            let offset = vertices + index * 48 + axis * 4;
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let materials = m2_array_offset(&bytes, 0x70)?;
    // Unlit, unfogged, two-sided opaque surfaces expose only depth ordering.
    bytes[materials..materials + 2].copy_from_slice(&7_u16.to_le_bytes());
    bytes[materials + 2..materials + 4].copy_from_slice(&0_u16.to_le_bytes());
    let skin = render_skin_bytes()?;
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Projection.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Projection00.skin",
            bytes: &skin,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new("Projection.m2")?)?;
    let plan = M2MeshPlan::prepare(&model, 0)?;
    let draw_index = plan
        .draws()
        .iter()
        .position(|draw| draw.batch().material_index == 0)
        .ok_or("projection mesh batch")?;
    let draw = &plan.draws()[draw_index];
    let _lock = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity distant surface projection", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The bootstrap enabled this window's extensions; the window outlives its surface.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole ownership of the live SDL surface transfers into the renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mesh = renderer.upload_m2_mesh(&plan)?;
    let pipeline = renderer.prepare_m2_pipeline(
        M2ShaderPlan::resolve(&model, draw)?,
        M2ShaderPermutation::resolve(
            draw,
            M2LocalLightCount::Zero,
            M2ShadowPermutation::Disabled,
            M2ShadowFiltering::Direct,
        ),
    )?;
    let white = renderer.upload_stock_m2_white()?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let texture = M2SampledTexture::new(white, sampler);
    let textures = renderer.prepare_m2_texture_sets(&[M2TextureSet::Two([texture; 2])])?[0];
    for origin in [
        Vec3::ZERO,
        Vec3::new(1340., -4380., 28.),
        Vec3::splat(16000.),
    ] {
        for angle in [0_f32, 0.17, 0.43, 0.91, 1.31, 2.16, 3.3, 4.7] {
            let placement = Mat4::from_translation(origin) * Mat4::from_rotation_z(angle);
            let camera =
                WorldCamera::stock(origin + Vec3::Z * 8., origin, Vec3::Y, 1000.).frame(1.)?;
            let material = |color| {
                M2MaterialUniform::new(
                    placement,
                    [Mat4::IDENTITY; 2],
                    camera.view() * placement,
                    color,
                    Vec4::ZERO,
                    Vec4::ZERO,
                )
            };
            let front = renderer.prepare_m2_draw(
                mesh,
                pipeline,
                textures,
                &plan,
                draw_index,
                false,
                material(Vec4::new(0., 1., 0., 1.)),
                0,
                0,
            )?;
            let bone_count = front.required_bone_transforms();
            let back = renderer.prepare_m2_draw(
                mesh,
                pipeline,
                textures,
                &plan,
                draw_index,
                false,
                material(Vec4::new(1., 0., 0., 1.)),
                bone_count as u32,
                0,
            )?;
            let mut bones = vec![Mat4::IDENTITY; bone_count];
            bones.extend(std::iter::repeat_n(
                Mat4::from_translation(Vec3::new(0., 0., -0.00025)),
                bone_count,
            ));
            let scene = M2SceneUniform::new(
                camera.projection(),
                camera.view(),
                camera.camera().position(),
                Vec3::ONE,
                Vec3::ZERO,
                Vec3::Z,
                Vec4::ZERO,
                Vec3::ZERO,
                [M2LocalLightState::disabled(); 4],
            );
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
                    camera.camera().position(),
                    Vec3::ONE,
                    Vec3::ZERO,
                    Vec3::Z,
                    Vec4::ZERO,
                ),
                scene,
            );
            renderer.request_frame_capture()?;
            // Draw the back surface last: collapsed depth would overwrite green with red.
            renderer.present_world_frame(
                scene,
                &bones,
                &[],
                &[],
                &[front, back],
                &[],
                &[],
                &[],
                &[],
                &[],
            )?;
            let capture = renderer
                .take_captured_frame()?
                .ok_or("projection capture")?;
            for (x, y) in [(32, 32), (30, 32), (32, 30)] {
                assert_eq!(
                    rgba8_pixel(capture.rgba8(), 64, x, y),
                    [0, 255, 0, 255],
                    "origin {origin:?}, angle {angle}, pixel {x}/{y}"
                );
            }
        }
    }
    Ok(())
}

//! Framebuffer regression for stock Color_T1/CDiffuse_T1 depth fog.

use std::error::Error;

use glam::{Mat4, Vec3, Vec4};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, BlpTextureSource, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{
    BlpColorSpace, M2AnimationClock, M2EffectOrder, M2LocalLightState, M2ParticleMeshPlan,
    M2ParticlePose, M2ParticleState, M2SampledTexture, M2SceneUniform, M2TextureSet,
    TerrainSceneUniform, VulkanBootstrap, WorldCamera, WorldFrameScene, WorldModelSceneUniform,
};

use super::{m2_array_offset, render_m2_bytes, render_skin_bytes, solid_raw3_blp};
use crate::support::{Fixture, FixtureFile};

/// Original Color_T1 permutation 0 and CDiffuse_T1 permutations 1/3/9 use
/// eye Z and c30's exponent. Equal-depth cards must retain the same fog color
/// off-axis, including with a parallel camera where clip W is constant.
#[test]
#[allow(unsafe_code)] // SDL transfers the Vulkan surface through its raw handle API.
fn particle_framebuffer_uses_stock_depth_and_fog_exponent() -> Result<(), Box<dyn Error>> {
    let mut bytes = render_m2_bytes("Particle.blp", 1)?;
    let offset = m2_array_offset(&bytes, 0x128)?;
    let flags = 0x0002_0010_u32;
    bytes[offset + 4..offset + 8].copy_from_slice(&flags.to_le_bytes());
    bytes[offset + 40] = 0;
    for relative in [0x178, 0x17c, 0x180, 0x184] {
        bytes[offset + relative..offset + relative + 4].copy_from_slice(&0.0_f32.to_le_bytes());
    }
    let skin = render_skin_bytes()?;
    let texture = solid_raw3_blp(1, 1, &[u32::MAX]);
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Fog.m2",
            bytes: &bytes,
        },
        FixtureFile {
            path: "Fog00.skin",
            bytes: &skin,
        },
        FixtureFile {
            path: "Particle.blp",
            bytes: &texture,
        },
    ])?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let model = DecodedM2Model::load(&mut assets, &AssetPath::new("Fog.m2")?)?;
    let source = BlpTextureSource::load(&mut assets, &AssetPath::new("Particle.blp")?)?;
    let emitter = &model.animations().particles()[0];
    let pose = M2ParticlePose::sample(
        model.animations(),
        emitter,
        M2AnimationClock::new(0, 0.0, 0.0),
    )?;
    let _sdl_test = crate::support::sdl_test_lock();
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity particle fog test", 256, 256)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: This live bootstrap enabled the extensions required by the window.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole ownership transfers to the creating instance's renderer.
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (256, 256), 0) }?;
    let texture = renderer.upload_blp_texture(&source, BlpColorSpace::Linear)?;
    let sampler = renderer.prepare_m2_sampler(&model.textures()[0])?;
    let texture_set = renderer
        .prepare_m2_texture_sets(&[M2TextureSet::One(M2SampledTexture::new(texture, sampler))])?[0];
    let eye = Vec3::new(7.0, -4.0, 2.0);
    let cameras = [
        WorldCamera::stock(eye, eye + Vec3::X, Vec3::Z, 100.0),
        WorldCamera::orthographic(
            eye,
            eye + Vec3::X,
            Vec3::Z,
            [-6.0, 6.0],
            [-6.0, 6.0],
            0.1,
            100.0,
        ),
    ];
    for camera in cameras {
        let camera = camera.frame(1.0)?;
        for shaded in [false, true] {
            let material_flags = flags | if shaded { 0 } else { 1 };
            let pipeline = renderer.prepare_m2_particle_pipeline(0, material_flags)?;
            for (exponent, visibility) in [(1.0, 0.5), (2.0, 0.25)] {
                for lateral in [0.0, 3.0] {
                    let center = eye + Vec3::new(10.0, lateral, 0.0);
                    let particle = M2ParticleState::new(0.0, center, Vec3::ZERO, 0x2483)?;
                    let mesh =
                        M2ParticleMeshPlan::prepare(emitter, pose, &[particle], camera, 1.0)?;
                    let draw = renderer.prepare_m2_particle_draw(
                        pipeline,
                        texture_set,
                        0,
                        material_flags,
                        M2EffectOrder::new(0, 0),
                        0,
                        0,
                        &mesh,
                    )?;
                    let fog = Vec4::new(0.0, 20.0, 0.0, exponent);
                    let scene = WorldFrameScene::new(
                        TerrainSceneUniform::new(
                            camera.view_projection(),
                            Vec3::ONE,
                            Vec3::ZERO,
                            Vec3::Z,
                        ),
                        WorldModelSceneUniform::new(
                            camera.view_projection(),
                            eye,
                            Vec3::ONE,
                            Vec3::ZERO,
                            Vec3::Z,
                            fog,
                        ),
                        M2SceneUniform::new(
                            camera.view_projection(),
                            camera.view(),
                            eye,
                            Vec3::ONE,
                            Vec3::ZERO,
                            Vec3::Z,
                            fog,
                            Vec3::ZERO,
                            [M2LocalLightState::disabled(); 4],
                        ),
                    );
                    renderer.request_frame_capture()?;
                    renderer.present_world_frame(
                        scene,
                        &[Mat4::IDENTITY],
                        &[],
                        &[],
                        &[],
                        mesh.vertices(),
                        mesh.indices(),
                        &[draw],
                        &[],
                        &[],
                    )?;
                    let capture = renderer
                        .take_captured_frame()?
                        .ok_or("missing fog capture")?;
                    let clip = camera.view_projection() * center.extend(1.0);
                    let x = ((clip.x / clip.w * 0.5 + 0.5) * 256.0) as usize;
                    let y = ((0.5 - clip.y / clip.w * 0.5) * 256.0) as usize;
                    let pixel = &capture.rgba8()[(y * 256 + x) * 4..][..3];
                    let bgra = mesh.vertices()[0].color_bgra();
                    for (actual, color) in pixel.iter().zip([bgra[2], bgra[1], bgra[0]]) {
                        let expected = f32::from(color) * visibility;
                        assert!(
                            (f32::from(*actual) - expected).abs() <= 1.0,
                            "projection={:?} shaded={shaded} exponent={exponent} lateral={lateral}: pixel={pixel:?}, expected channel={expected}",
                            camera.camera().projection()
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

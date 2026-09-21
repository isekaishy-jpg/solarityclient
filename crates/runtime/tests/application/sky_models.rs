//! Real archive stars model, animation, camera-relative placement, and GPU coverage.

use super::*;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_rendering::{
    TerrainSceneUniform, VulkanBootstrap, WorldCamera, WorldFrameScene, WorldModelSceneUniform,
    WorldSkyModelFrame,
};

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
#[allow(unsafe_code)]
fn installed_stars_follow_opacity_without_camera_translation()
-> Result<(), Box<dyn std::error::Error>> {
    let recording_cpu = crate::frame_cpu_support::executor()?;
    let _lock = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL lock poisoned")?;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let animations = AnimationDataCatalog::load(&mut store)?;
    let resident = ResidentM2Source::load(
        &AssetPath::new("Environments/Stars/stars.mdl")?,
        &mut Default::default(),
        &mut Default::default(),
        &mut store,
    )?;
    assert_eq!(resident.model().animations().particles().len(), 0);
    assert_eq!(resident.model().animations().ribbons().len(), 0);
    let mut stars = SkyM2Model::new(resident, 0);
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity installed stars", 512, 512)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: SDL transfers sole surface ownership; its window outlives the renderer.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (512, 512), 0) }?;
    let mut random = CrtRand::new();
    let mut previous = Vec::new();
    let mut first = Vec::new();
    let mut fade_changes = 0;
    for (case, time) in [
        0_u32, 0, 12000, 12000, 40000, 40000, 83000, 83000, 100000, 100000,
    ]
    .into_iter()
    .enumerate()
    {
        let eye = if case % 2 == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(-12345., 6789., 543.)
        };
        let camera =
            WorldCamera::stock(eye, eye + Vec3::new(1., 0., 1.), Vec3::Z, 1000.).frame(1.)?;
        let opacity = match time {
            12000 => 127. / 255.,
            83000 => 64. / 255.,
            _ => 1.,
        };
        stars.prepare(
            &mut crate::frame_cpu_support::gpu_preparation(&mut renderer),
            camera,
            time,
            opacity,
            0,
            &animations,
            &mut random,
        )?;
        assert_eq!(stars.draws().len(), 7);
        let uniform = sky_scene(camera);
        let scene = WorldFrameScene::new(
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
            uniform,
        )
        .with_sky_models(WorldSkyModelFrame::new(
            uniform,
            stars.bones(),
            stars.draws(),
            &[],
        ));
        renderer.request_frame_capture()?;
        let report = renderer.present_world_frame(
            &mut &recording_cpu,
            scene,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
        )?;
        assert_eq!(report.sky_model_draw_count(), 7);
        let capture = renderer
            .take_captured_frame()?
            .ok_or("missing installed sky capture")?;
        let pixels = capture.rgba8();
        let visible = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|rgba| rgba[..3].iter().any(|channel| *channel > 8))
            .count();
        assert!(
            visible > 50,
            "case {case}: only {visible} visible star pixels"
        );
        if case % 2 != 0 {
            assert_eq!(pixels, previous, "sky changed under camera translation");
        }
        if case == 0 {
            first = pixels.to_vec();
        } else if opacity == 1. {
            // The installed model has an 83-second sequence with static geometry/materials.
            assert_eq!(
                pixels, first,
                "static authored stars drifted with scene time"
            );
        } else {
            let energy = |data: &[u8]| {
                data.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|rgba| rgba[..3].iter().map(|v| u64::from(*v)).sum::<u64>())
                    .sum::<u64>()
            };
            assert!(
                energy(pixels) < energy(&first) * 3 / 4,
                "dawn fade did not attenuate stars"
            );
            fade_changes += 1;
        }
        if case == 0
            && let Some(path) = std::env::var_os("SOLARITY_STARS_CAPTURE_RGBA")
        {
            std::fs::write(path, pixels)?;
        }
        previous = pixels.to_vec();
    }
    assert!(fade_changes == 4, "missing star opacity samples");
    let camera = WorldCamera::stock(Vec3::ZERO, Vec3::X, Vec3::Z, 1000.).frame(1.)?;
    stars.prepare(
        &mut crate::frame_cpu_support::gpu_preparation(&mut renderer),
        camera,
        120000,
        0.,
        0,
        &animations,
        &mut random,
    )?;
    assert!(stars.draws().is_empty());
    assert!(stars.bones().is_empty());
    Ok(())
}

/// Uses the animated light fixture's real immutable GPU generation for exact
/// independent-slot parity, including differing opacity and translated cameras.
pub(in crate::application::terrain_frame::m2) fn verify_worker_batch(
    cpu: &solarity_cpu::CpuExecutor,
    source: &M2GpuSource,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::application::frame_pipeline::FrameWait;
    let mut batch = SkyBatch::default();
    let mut expected = Vec::new();
    for (index, job) in batch.jobs.iter_mut().enumerate() {
        let eye = Vec3::new(index as f32 * 123., -50., 2.);
        let camera = WorldCamera::stock(eye, eye + Vec3::X, Vec3::Z, 1000.).frame(1.)?;
        let input = SkyInput {
            source: Arc::clone(source),
            camera,
            clock: solarity_rendering::M2AnimationClock::new(
                0,
                index as f32 * 157.,
                index as f32 * 157.,
            ),
            opacity: 1. - index as f32 * 0.17,
        };
        let mut serial = SkyPrepared::default();
        serial.prepare(&input)?;
        expected.push(serial);
        job.input = Some(input);
    }
    let held = crate::frame_cpu_support::continuation_support::HeldFrameWorkers::new(cpu)?;
    batch.batch.start(cpu, &mut batch.jobs)?;
    let pending = !batch.batch.is_finished();
    held.release()?;
    assert!(
        pending,
        "sky submission must return while workers are occupied"
    );
    batch.finish(&mut FrameWait::Offline)?;
    for (job, expected) in batch.jobs.iter().zip(&expected) {
        assert!(
            job.input.is_none(),
            "source pins return on worker completion"
        );
        assert_eq!(job.prepared.bones(), expected.bones());
        assert_eq!(job.prepared.draws, expected.draws);
        assert_eq!(job.prepared.local_lights, expected.local_lights);
    }
    assert_ne!(
        batch.jobs[0].prepared.bones(),
        batch.jobs[4].prepared.bones()
    );
    assert_ne!(batch.jobs[0].prepared.draws, batch.jobs[4].prepared.draws);
    // Reuse previously active slots without publishing old draws, light banks or pins.
    for job in &mut batch.jobs {
        job.reset();
    }
    batch.run(cpu, &mut FrameWait::Offline)?;
    assert!(
        batch
            .jobs
            .iter()
            .all(|job| job.prepared.bones().is_empty() && job.prepared.draws.is_empty())
    );
    Ok(())
}

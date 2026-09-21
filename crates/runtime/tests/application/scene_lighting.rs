//! World publication must precede every receiver query, including culled sources.

use super::*;
use crate::test_support::{ClientFixture, SDL_TEST_LOCK, game_object_models};
use solarity_asset::ResourceLease;

use crate::frame_cpu_support::continuation_support as late_continuation_support;

#[path = "interior_scene_lighting.rs"]
mod interior;
#[path = "scene_lighting_overlap.rs"]
mod overlap;

/// Reusing sparse storage must match a fresh bank across residency and graph changes.
#[test]
fn sparse_receiver_reset_cannot_inherit_removed_or_remapped_parents() -> Result<(), Box<dyn Error>>
{
    let mut retained = scene_lighting::SceneLighting::default();
    let base = M2SceneUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::ZERO,
        Vec3::ZERO,
        [M2LocalLightState::disabled(); 4],
    );
    let exterior = M2DirectionalLight::new(-Vec3::Z, Vec3::ZERO, Vec3::ONE);
    let callback = M2DirectionalLight::new(Vec3::X, Vec3::Y, Vec3::Z);
    retained.receiver_with_light(50_000, Some(50_001), Vec3::X, Some(callback), Some(Vec3::Y))?;
    retained.receiver_with_light(50_001, None, Vec3::Y, Some(callback), None)?;
    retained.finish(base, exterior)?;

    for parent in [50_000, 1, 50_001, 75_000] {
        retained.clear();
        let mut fresh = scene_lighting::SceneLighting::default();
        for bank in [&mut retained, &mut fresh] {
            // The parent can be absent, move to an earlier index after compaction,
            // or appear beyond a previous high-water mark. Every new frame must
            // match a fresh bank even though backing storage remains allocated.
            bank.receiver(0, Some(parent), Vec3::ZERO)?;
            if parent == 1 || parent == 75_000 {
                bank.receiver_with_light(parent, None, Vec3::Z, Some(callback), None)?;
            }
            bank.finish(base, exterior)?;
        }
        assert_eq!(retained.scenes, fresh.scenes);
    }
    // Backing storage must not change the old dense bank's bounded resolution
    // of an attachment cycle reached from a separate receiver.
    retained.clear();
    retained.receiver(90_001, None, Vec3::ZERO)?;
    retained.clear();
    let mut fresh = scene_lighting::SceneLighting::default();
    for bank in [&mut retained, &mut fresh] {
        bank.receiver(0, Some(1), Vec3::ZERO)?;
        bank.receiver_with_light(1, Some(2), Vec3::X, Some(callback), None)?;
        bank.receiver_with_light(2, Some(1), Vec3::Y, Some(exterior), None)?;
        bank.finish(base, exterior)?;
    }
    assert_eq!(retained.scenes, fresh.scenes);
    Ok(())
}

#[test]
fn directional_reenable_order_matches_native_and_attached_receivers_inherit()
-> Result<(), Box<dyn Error>> {
    let mut bank = scene_lighting::SceneLighting::default();
    let owner = Rc::new(());
    let base = M2SceneUniform::new(
        Mat4::IDENTITY,
        Mat4::IDENTITY,
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::ZERO,
        Vec3::ZERO,
        [M2LocalLightState::disabled(); 4],
    );
    let exterior = M2DirectionalLight::new(-Vec3::Z, Vec3::ZERO, Vec3::ZERO);
    for row in include_str!("../../../rendering/tests/fixtures/scene_liquid_light_native.order.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let row = row
            .split_ascii_whitespace()
            .map(str::parse::<usize>)
            .collect::<Result<Vec<_>, _>>()?;
        bank.clear();
        bank.sample_directional.clear();
        for index in 0..4 {
            if row[0] & (1 << index) != 0 {
                bank.sample_directional.push((
                    index,
                    M2DirectionalLight::new(
                        -Vec3::Z,
                        Vec3::ZERO,
                        Vec3::new((index + 1) as f32, 0., 0.),
                    ),
                ));
            }
        }
        bank.sample_points = vec![
            solarity_rendering::M2PointLight::new(Vec3::X, Vec3::ZERO, Vec3::ONE),
            solarity_rendering::M2PointLight::new(Vec3::X * 41., Vec3::ZERO, Vec3::Y),
        ];
        bank.publish(&owner, 0.5)?;
        bank.receiver(0, None, Vec3::ZERO)?;
        bank.receiver(1, Some(0), Vec3::X * 40.)?;
        bank.receiver(2, None, Vec3::X * 40.)?;
        bank.receiver(3, Some(5), Vec3::Y * 80.)?;
        bank.receiver(5, Some(7), Vec3::Y * 100.)?;
        let callback = M2DirectionalLight::new(Vec3::X, Vec3::Y, Vec3::Z);
        bank.receiver_with_light(7, None, Vec3::ZERO, Some(callback), None)?;
        // Registration indices need not be monotonic either.
        bank.receiver(4, Some(3), Vec3::Z * 60.)?;
        bank.finish(base, exterior)?;
        assert_eq!(
            bank.directionals()
                .iter()
                .map(|light| (light.diffuse().x * 2.) as usize - 1)
                .collect::<Vec<_>>(),
            row[1..]
        );
        assert_eq!(
            bank.scenes[0], bank.scenes[1],
            "an attached receiver uses its parent's complete query"
        );
        assert_ne!(
            bank.scenes[0], bank.scenes[2],
            "a separate model queries its own position"
        );
        for child in [3, 4, 6] {
            assert_eq!(
                bank.scenes[child], bank.scenes[5],
                "nested later parents supply both point query and entity callback"
            );
        }
        assert_ne!(
            bank.scenes[5], bank.scenes[0],
            "root callback survives forward inheritance"
        );
        assert_eq!(bank.points().points()[0].diffuse(), Vec3::splat(0.5));
    }
    Ok(())
}
use glam::{Vec3, Vec4};
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_rendering::{
    M2DirectionalLight, M2LocalLightState, M2SceneUniform, VulkanBootstrap, WorldCamera,
    WorldScreenWindow,
};
use std::error::Error;

#[test]
#[allow(unsafe_code)] // The hidden SDL surface transfers to Vulkan ownership.
fn offscreen_animated_sources_light_distinct_receivers_and_retire_when_hidden()
-> Result<(), Box<dyn Error>> {
    let receiver = game_object_models::model_with_animations(&[0])?;
    let mut light = receiver.clone();
    let bones = u32::from_le_bytes(light[0x30..0x34].try_into()?) as usize;
    track(
        &mut light,
        bones + 16,
        &[0, 1000],
        &floats(&[0., 0., 0., 2., 0., 0.]),
        12,
        1,
    );
    let record = light.len();
    light.resize(record + 156, 0);
    light[record..record + 2].copy_from_slice(&1_u16.to_le_bytes());
    light[record + 12..record + 16].copy_from_slice(&(-4_f32).to_le_bytes());
    track(
        &mut light,
        record + 0x10,
        &[0],
        &floats(&[0., 0., 0.]),
        12,
        0,
    );
    track(&mut light, record + 0x24, &[0], &floats(&[1.]), 4, 0);
    track(
        &mut light,
        record + 0x38,
        &[0],
        &floats(&[1., 0.5, 0.25]),
        12,
        0,
    );
    track(&mut light, record + 0x4c, &[0], &floats(&[1.]), 4, 0);
    for at in [0x60, 0x74] {
        light[record + at + 2..record + at + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    }
    track(&mut light, record + 0x88, &[0, 499, 500], &[1, 1, 0], 1, 0);
    array(&mut light, 0x108, 1, record);
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[
        ("Receiver.m2", &receiver),
        ("Receiver00.skin", &skin),
        ("Light.m2", &light),
        ("Light00.skin", &skin),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let receiver = ResourceLease::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Receiver.m2")?,
    )?);
    let light = ResourceLease::new(DecodedM2Model::load(
        &mut store,
        &AssetPath::new("Light.m2")?,
    )?);
    let _lock = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock poisoned")?;
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity world scene light publication", 64, 64)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The window remains live and the renderer receives sole surface ownership.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    let mut renderer = unsafe { bootstrap.attach_surface(surface, (64, 64), 0) }?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut crate::frame_cpu_support::gpu_preparation(&mut renderer),
        &ResidentM2Scene::default(),
        Arc::clone(&animations),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    for model in [&receiver, &light] {
        frame.sources.push(Some(prepare_gpu_source(
            &mut crate::frame_cpu_support::gpu_preparation(&mut renderer),
            model,
            &[M2ResolvedTexture::StockWhite],
            None,
            M2LocalLightCount::Four,
            M2ModelOrientation::Authored,
        )?));
    }
    for (index, (source, position)) in [
        (0, Vec3::ZERO),
        (0, Vec3::Y * 20.),
        (1, Vec3::Z * 5.),
        (1, Vec3::new(0., 20., 5.)),
    ]
    .into_iter()
    .enumerate()
    {
        let model = if source == 0 { &receiver } else { &light };
        let playback = M2Playback::default_sequence(model, &animations, 0, &mut random)?;
        frame.placements.push(m2_gpu_placement(
            source,
            Mat4::from_translation(position),
            M2GpuPlacementOwner::Static(ResidentM2Owner::TerrainDoodad {
                unique_id: index as u32,
            }),
            model,
            Some(M2PlaybackStorage::Local(playback)),
            None,
            0,
        )?);
    }
    let camera = WorldCamera::orthographic(
        Vec3::new(8., 10., 0.),
        Vec3::Y * 10.,
        Vec3::Z,
        [-12., 12.],
        [-2., 2.],
        0.1,
        100.,
    )
    .frame(1.)?;
    let base = M2SceneUniform::new(
        camera.projection(),
        camera.view(),
        camera.camera().position(),
        Vec3::ZERO,
        Vec3::ZERO,
        Vec3::Z,
        Vec4::new(0., 100., 0., 1.),
        Vec3::ZERO,
        [M2LocalLightState::disabled(); 4],
    );
    let exterior = M2DirectionalLight::new(-Vec3::Z, Vec3::splat(0.1), Vec3::splat(0.2));
    let cpu = crate::frame_cpu_support::executor()?;
    verify_late_pose_dependency(&cpu, frame.sources[1].as_ref().ok_or("light source")?)?;
    verify_scene_pose_batch(&cpu, frame.sources[1].as_ref().ok_or("light source")?)?;
    for now in [1., 251., 751.] {
        let consumed = frame.late_pose.consumed();
        let visible = frame.prepare_visible_draws_with_unit_effects(
            &renderer,
            &cpu,
            &mut crate::application::frame_pipeline::FrameWait::Offline,
            WorldFrustum::new(camera, WorldScreenWindow::FULL)?,
            camera,
            M2TransparentPass::One,
            Vec3::ZERO,
            now,
            M2CameraEffectScale::EXTERNAL_CAMERA,
            &mut random,
            None,
            None,
            Some((base, exterior)),
            None,
            None,
            None,
        )?;
        assert_eq!(
            visible.draws.len(),
            2,
            "offscreen sources publish without drawing"
        );
        assert_eq!(
            visible.instance_scenes.len(),
            2,
            "offscreen light emitters do not allocate color receiver scenes"
        );
        for (receiver_index, y) in [(0, 0_f32), (1, 20.)] {
            let scene = visible.instance_scenes[visible.draws[receiver_index]
                .scene_index()
                .ok_or("instance scene")? as usize];
            let bytes = scene.to_bytes();
            // Fixed prefix 160 bytes, then 64 bytes per local slot. Slot one
            // is the nearest point; slot zero is the finalized directional sun.
            let point = Vec3::new(word(&bytes, 224), word(&bytes, 228), word(&bytes, 232));
            if now < 500. {
                assert!(
                    point.abs_diff_eq(Vec3::new((now - 1.) * 0.002, y, 1.), 0.00001),
                    "time {now}, receiver {receiver_index}: {point}"
                );
            } else {
                assert_eq!(
                    &bytes[240..272],
                    &[0; 32],
                    "visibility retires both point-color terms"
                );
            }
        }
        assert_eq!(
            frame.late_pose.consumed() - consumed,
            2,
            "both late light bones are consumed from workers"
        );
        assert_eq!(
            frame.scene_lighting.points().points().len(),
            if now < 500. { 2 } else { 0 }
        );
    }
    Ok(())
}

fn word(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}
fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
fn array(bytes: &mut [u8], at: usize, count: usize, offset: usize) {
    bytes[at..at + 4].copy_from_slice(&(count as u32).to_le_bytes());
    bytes[at + 4..at + 8].copy_from_slice(&(offset as u32).to_le_bytes());
}
fn track(
    bytes: &mut Vec<u8>,
    at: usize,
    times: &[u32],
    values: &[u8],
    stride: usize,
    interpolation: u16,
) {
    bytes[at..at + 2].copy_from_slice(&interpolation.to_le_bytes());
    bytes[at + 2..at + 4].copy_from_slice(&u16::MAX.to_le_bytes());
    let timestamps = bytes.len();
    bytes.extend(times.iter().flat_map(|value| value.to_le_bytes()));
    let data = bytes.len();
    bytes.extend(values);
    let channels = bytes.len();
    bytes.resize(channels + 16, 0);
    array(bytes, channels, times.len(), timestamps);
    array(bytes, channels + 8, values.len() / stride, data);
    array(bytes, at + 4, 1, channels);
    array(bytes, at + 12, 1, channels + 8);
}

impl scene_lighting::SceneLighting {
    /// Reference-only evaluation for the frozen serial/parity fixtures.
    pub fn finish(
        &mut self,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<(), RuntimeTerrainFrameError> {
        self.begin_finish(None, None, base, exterior)?;
        self.finish_pending(&mut crate::application::frame_pipeline::FrameWait::Offline)
    }
}

/// A discovered dependency returns main immediately even when every worker is occupied.
fn verify_scene_pose_batch(
    cpu: &solarity_cpu::CpuExecutor,
    source: &M2GpuSource,
) -> Result<(), Box<dyn Error>> {
    use crate::application::frame_pipeline::FrameWait;
    use solarity_rendering::M2BoneTransforms;
    sky::tests::verify_worker_batch(cpu, source)?;
    let mut poses = preparation::poses::ScenePoses::default();
    let mut wait = FrameWait::Offline;
    let view = Mat4::from_rotation_z(0.3);
    let overrides = M2BonePoseOverrides {
        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
        ..Default::default()
    };
    let clock = |i: usize| M2AnimationClock::new(0, 31. * i as f32, 31. * i as f32);
    let held = late_continuation_support::HeldFrameWorkers::new(cpu)?;
    for index in 0..12 {
        poses.seed(cpu, index, source, clock(index), view, overrides, &[0, 0])?;
    }
    poses.start(cpu)?;
    let all_pending = (0..12)
        .map(|index| poses.is_ready(index))
        .collect::<Result<Vec<_>, _>>()?;
    held.release()?;
    assert!(
        all_pending.into_iter().all(|ready| !ready),
        "batch dispatch must return before occupied workers"
    );
    let mut serial = solarity_rendering::M2BoneSamples::default();
    for index in (0..12).rev() {
        serial.recompose(
            source.model.animations(),
            clock(index),
            view,
            overrides,
            &[0],
        )?;
        let sampled = poses.sample(
            Some(cpu),
            &mut wait,
            index,
            source,
            clock(index),
            view,
            overrides,
            &[0],
        )?;
        assert_eq!(sampled.bone_transform(0), serial.bone_transform(0));
    }
    assert_eq!(
        poses.hits, 12,
        "every frozen event sample consumed its independent worker result"
    );
    assert_eq!(poses.misses, 0);
    let unchanged = poses
        .sample(
            Some(cpu),
            &mut wait,
            0,
            source,
            clock(0),
            view,
            overrides,
            &[0],
        )?
        .bone_transform(0);
    assert_eq!(
        poses.hits, 13,
        "another callback reuses the same immutable result"
    );
    for (tick, changed_view) in [(251., view), (751., Mat4::from_rotation_y(0.6))] {
        let changed = M2AnimationClock::new(0, tick, tick);
        serial.recompose(
            source.model.animations(),
            changed,
            changed_view,
            overrides,
            &[0],
        )?;
        let actual = poses
            .sample(
                Some(cpu),
                &mut wait,
                0,
                source,
                changed,
                changed_view,
                overrides,
                &[0],
            )?
            .bone_transform(0);
        assert_eq!(actual, serial.bone_transform(0));
        assert_ne!(actual, unchanged);
    }
    assert_eq!(
        poses.misses, 2,
        "changed callback inputs cannot reuse a stale pose"
    );
    poses.finish(&mut wait)?;

    let hits_before = poses.hits;
    for index in 0..6 {
        poses.seed_palette(cpu, index, source, clock(index), view, overrides)?;
    }
    poses.start(cpu)?;
    let mut palette = M2BonePose::default();
    for index in 0..6 {
        serial.recompose(
            source.model.animations(),
            clock(index),
            view,
            overrides,
            &[0],
        )?;
        poses.palette(
            cpu,
            &mut wait,
            index,
            source,
            clock(index),
            view,
            overrides,
            &mut palette,
        )?;
        assert_eq!(palette.bone_transform(0), serial.bone_transform(0));
    }
    assert_eq!(
        poses.hits - hits_before,
        6,
        "independent parents consume batched full palettes"
    );
    poses.finish(&mut wait)?;

    // Failed unused demand cannot poison a narrower callback.
    poses.seed(cpu, 0, source, clock(0), view, overrides, &[0, usize::MAX])?;
    poses.start(cpu)?;
    let valid = poses.sample(
        Some(cpu),
        &mut wait,
        0,
        source,
        clock(0),
        view,
        overrides,
        &[0],
    )?;
    assert_eq!(valid.bone_transform(0), unchanged);
    poses.finish(&mut wait)?;

    // Abandonment must return worker-owned state before this owner is reused.
    poses.seed(cpu, 3, source, clock(3), view, overrides, &[0])?;
    poses.start(cpu)?;
    poses.finish(&mut wait)?;
    poses.seed(cpu, 3, source, clock(4), view, overrides, &[0])?;
    poses.start(cpu)?;
    serial.recompose(source.model.animations(), clock(4), view, overrides, &[0])?;
    assert_eq!(
        poses
            .sample(
                Some(cpu),
                &mut wait,
                3,
                source,
                clock(4),
                view,
                overrides,
                &[0]
            )?
            .bone_transform(0),
        serial.bone_transform(0)
    );
    poses.finish(&mut wait)?;
    Ok(())
}

fn verify_late_pose_dependency(
    cpu: &solarity_cpu::CpuExecutor,
    source: &M2GpuSource,
) -> Result<(), Box<dyn Error>> {
    use crate::application::frame_pipeline::FrameWait;
    use solarity_rendering::M2BoneTransforms;
    let mut late = preparation::poses::LatePose::default();
    let clock = M2AnimationClock::new(0, 251., 251.);
    let view = Mat4::from_rotation_z(0.3);
    let overrides = M2BonePoseOverrides {
        model_oriented_billboard_bones: &source.model_oriented_billboard_bones,
        ..Default::default()
    };
    for palette in [false, true] {
        let held = late_continuation_support::HeldFrameWorkers::new(cpu)?;
        late.start(cpu, 2, source, clock, view, overrides, &[0], palette)?;
        let suspended = !late.is_ready();
        held.release()?;
        assert!(suspended, "pending numeric work did not run or block main");
        late.finish(&mut FrameWait::Offline)?;
        let mut serial = solarity_rendering::M2BoneSamples::default();
        serial.recompose(source.model.animations(), clock, view, overrides, &[0])?;
        if palette {
            let mut output = M2BonePose::default();
            assert!(late.take(2, &source.model, clock, view, overrides, &mut output)?);
            assert_eq!(output.bone_transform(0), serial.bone_transform(0));
        } else {
            let mut output = solarity_rendering::M2BoneSamples::default();
            assert!(late.take_samples(
                2,
                &source.model,
                clock,
                view,
                overrides,
                &[0],
                &mut output
            )?);
            assert_eq!(output.bone_transform(0), serial.bone_transform(0));
        }
        late.release_model();
    }
    Ok(())
}

//! Actual Vulkan resource admission preserves independent object playback.

#[path = "game_object_world_model_scene.rs"]
mod world_model;

#[path = "unit_animation_scene.rs"]
mod unit_animation;

use super::{CrtRand, M2Frame, M2GpuPlacementOwner, M2Playback, ResidentM2Scene};
use crate::application::game_object_coordinator::RuntimeGameObjectPresentation;
use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::SdlPlatform;
use crate::test_support::{ClientFixture, game_object_models as models};
use glam::Vec3;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, AssetStoreHandle, ClientDataRoot,
    DecodedM2Model, GameObjectDisplayCatalog, Locale, M2ModelAnimationMode,
};
use solarity_ecs::{
    ActiveWorld, GameObjectMovement, GameObjectPresentation, GameObjectTransport, ObjectKind,
    ObjectPresentation, WorldBootstrap, WorldMapId, WorldTransform,
};
use solarity_rendering::{M2ParticleTwinkleTable, VulkanBootstrap, VulkanRenderer};
use std::error::Error;
use std::sync::Arc;

// SDL owns one process-wide main thread and event pump until its context drops.
static SDL_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn game_object_fallback_timers_preserve_state_and_random_order() -> Result<(), Box<dyn Error>> {
    use M2ModelAnimationMode::{Forward, HoldEnd, HoldStart};
    let skin = models::skin()?;
    let mut dbc = b"WDBC".to_vec();
    for value in [2_u32, 8, 32, 1] {
        dbc.extend_from_slice(&value.to_le_bytes());
    }
    for (id, fallback) in [(147_u32, 146_u32), (149, 148)] {
        for value in [id, 0, 0, 0, 0x20, fallback, id, 0] {
            dbc.extend_from_slice(&value.to_le_bytes());
        }
    }
    dbc.push(0);
    for (animations, state, selected, mode) in [
        (&[0_u16][..], 1, 0, Forward),
        (&[0][..], 0, 0, Forward),
        (&[148][..], 1, 148, HoldStart),
        (&[146][..], 0, 146, HoldStart),
        (&[146][..], 1, 146, HoldEnd),
        (&[148][..], 0, 148, HoldEnd),
        (&[7][..], 2, 7, Forward),
    ] {
        let mut model_bytes = models::model_with_animations(animations)?;
        let sequences = u32::from_le_bytes(model_bytes[0x20..0x24].try_into()?) as usize;
        for index in 0..animations.len() {
            // Non-looping clips distinguish the terminal hold from the first pose.
            let flags = sequences + index * 64 + 12;
            model_bytes[flags..flags + 4].copy_from_slice(&0x21_u32.to_le_bytes());
        }
        let fixture = ClientFixture::with_common_files(&[
            ("World\\GameObject.m2", &model_bytes),
            ("World\\GameObject00.skin", &skin),
            ("DBFilesClient\\AnimationData.dbc", &dbc),
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let mut store = AssetStore::mount(catalog)?;
        let model = DecodedM2Model::load(&mut store, &AssetPath::new("World\\GameObject.m2")?)?;
        let catalog = AnimationDataCatalog::load(&mut store)?;
        let mut playback = M2Playback::unstarted(0);
        let mut random = CrtRand::new();
        let mut expected_random = random;
        let _variation = expected_random.next_u15();
        let _cycle = expected_random.next_u15();
        playback.select_game_object_state(&model, &catalog, state, 2_000, &mut random)?;
        assert_eq!(playback.animation_id, selected);
        assert_eq!(playback.script_mode, mode);
        assert_eq!(random, expected_random);
        let timer = playback.script_timer.ok_or("missing fallback timer")?;
        assert_eq!(timer.start_time_ms(), 2_001);
        playback.select_game_object_state(&model, &catalog, state, 2_500, &mut random)?;
        assert_eq!(
            playback.script_timer,
            Some(timer),
            "unchanged state must retain timer"
        );
        assert_eq!(
            random, expected_random,
            "unchanged state must not roll again"
        );
        if animations == [0] && state == 0 {
            playback.select_game_object_state(&model, &catalog, 2, 2_500, &mut random)?;
            assert_eq!(
                playback.script_timer,
                Some(timer),
                "native missing-clip branch keeps its current pre-fallback request"
            );
            assert_eq!(random, expected_random);
        }
        let clock = playback.clock(&model, 2_501.0, 2_501.0, &mut random)?.clock;
        assert_eq!(
            clock.animation_time_ms(),
            match mode {
                HoldStart => 0.0,
                HoldEnd => 1_000.0,
                _ => 500.0,
            }
        );
    }
    Ok(())
}

#[test]
fn game_object_gpu_resources_and_playback_follow_independent_lifetimes()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    verify_independent_lifetimes(models::model()?)?;
    // Ordinary props frequently have only Stand; they must still own geometry,
    // a valid native sequence timer, and independent effect/playback histories.
    verify_independent_lifetimes(models::model_with_animations(&[0])?)
}

fn verify_independent_lifetimes(model: Vec<u8>) -> Result<(), Box<dyn Error>> {
    let skin = models::skin()?;
    let displays = models::displays();
    let fixture = ClientFixture::with_common_files(&[
        ("World\\GameObject.m2", &model),
        ("World\\GameObject00.skin", &skin),
        ("DBFilesClient\\GameObjectDisplayInfo.dbc", &displays),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut objects =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.0,
    ));
    add_object(&mut world, 30, 42)?;
    add_object(&mut world, 10, 43)?;
    objects.synchronize(Some(&world))?;
    assert_eq!(objects.resident_object_count(), 2);
    assert_eq!(
        objects.resident_resource_count(),
        1,
        "MDX/M2 aliases must share complete preparation"
    );

    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut random = CrtRand::new();
    let mut frame = M2Frame::prepare(
        &mut renderer,
        &ResidentM2Scene::default(),
        &mut random,
        Arc::new(M2ParticleTwinkleTable::new(1)),
    )?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 2);
    assert_eq!(frame.sources.len(), 1);
    let mesh = frame.sources[0]
        .as_ref()
        .ok_or("missing GPU source")?
        .mesh
        .ok_or("fixture must upload an actual triangle mesh")?;
    let first_identity = world.object_identity(30).ok_or("missing first lifetime")?;
    let mut playback = frame.placements[0]
        .playback
        .as_mut()
        .map(super::M2PlaybackStorage::borrow_mut)
        .ok_or("missing retained playback")?;
    assert!(playback.script_timer.is_some());
    playback.cycle_started_ms = 40.0;
    playback.previous_event_elapsed_ms = 25.0;
    playback.event_timeline_started = true;
    drop(playback);
    let mut expected_random = random;
    add_object(&mut world, 20, 42)?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    let _new_variation_roll = expected_random.next_u15();
    let _new_cycle_roll = expected_random.next_u15();
    assert_eq!(
        random, expected_random,
        "only the newly admitted object rolls a weighted variation and cycle count"
    );
    assert_eq!(frame.sources.len(), 1);
    assert_eq!(frame.placements.len(), 3);
    assert_eq!(
        frame.sources[0].as_ref().ok_or("lost source")?.mesh,
        Some(mesh)
    );
    let playback = frame.placements[0]
        .playback
        .as_ref()
        .map(super::M2PlaybackStorage::borrow)
        .ok_or("lost playback")?;
    assert_eq!(playback.cycle_started_ms, 40.0);
    assert_eq!(playback.previous_event_elapsed_ms, 25.0);
    assert!(playback.event_timeline_started);
    drop(playback);

    world.update_game_object_movement(
        30,
        GameObjectMovement::new(
            0,
            Some(GameObjectTransport {
                guid: 99,
                position: Vec3::ONE,
                orientation: 0.0,
            }),
        ),
    )?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    frame.update_game_object_states(objects.frame_input(Some(&world)), 400.0, &mut random)?;
    assert!(!frame.placements[0].placement_valid);
    assert_eq!(frame.placements.len(), 3);
    let parent = world.create_object(
        99,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::splat(5.0), 0.0)),
        [],
    )?;
    world
        .storage_mut()
        .add_component(parent, (ObjectPresentation::new(1, 2.0),));
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    frame.update_game_object_states(objects.frame_input(Some(&world)), 500.0, &mut random)?;
    assert!(frame.placements[0].placement_valid);
    assert_eq!(
        frame.placements[0].transform.w_axis.truncate(),
        Vec3::splat(7.0)
    );
    assert_eq!(
        frame.placements[0]
            .playback
            .as_ref()
            .map(super::M2PlaybackStorage::borrow)
            .ok_or("parent arrival lost playback")?
            .cycle_started_ms,
        40.0
    );
    assert_eq!(
        frame.sources[0]
            .as_ref()
            .ok_or("parent arrival lost source")?
            .mesh,
        Some(mesh)
    );

    world.remove_object(30)?;
    add_object(&mut world, 30, 42)?;
    objects.synchronize(Some(&world))?;
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert_eq!(frame.placements.len(), 3);
    assert_eq!(frame.sources.len(), 1);
    assert!(frame.placements.iter().all(|placement| !matches!(placement.owner, M2GpuPlacementOwner::GameObject { identity, .. } if identity == first_identity)));
    let recreated = frame
        .placements
        .last()
        .ok_or("missing recreated placement")?;
    assert!(
        !recreated
            .playback
            .as_ref()
            .map(super::M2PlaybackStorage::borrow)
            .ok_or("missing recreated playback")?
            .event_timeline_started
    );
    objects.disconnect();
    objects.synchronize_animations(Some(&world), &mut random)?;
    frame.synchronize_game_objects(
        &mut renderer,
        objects.frame_input(Some(&world)),
        &mut random,
    )?;
    assert!(frame.sources.is_empty());
    assert!(frame.placements.is_empty());
    renderer.shutdown()?;
    Ok(())
}

fn add_object(world: &mut ActiveWorld, guid: u64, display: u32) -> Result<(), Box<dyn Error>> {
    let entity = world.create_object(
        guid,
        ObjectKind::GameObject,
        Some(WorldTransform::new(Vec3::X * guid as f32, 0.0)),
        [(14, 0xFFFF_0000)],
    )?;
    world.storage_mut().add_component(
        entity,
        (
            ObjectPresentation::new(1, 1.0),
            GameObjectPresentation::new(display, 1).with_dynamic_word(0xFFFF_0000),
        ),
    );
    Ok(())
}

#[allow(unsafe_code)]
fn renderer(platform: &SdlPlatform) -> Result<VulkanRenderer, Box<dyn Error>> {
    let bootstrap = VulkanBootstrap::start(&platform.vulkan_instance_extensions()?)?;
    // SAFETY: The hidden SDL window outlives its surface and renderer. The
    // bootstrap enabled its required extensions and takes surface ownership.
    let surface = unsafe { platform.create_vulkan_surface(bootstrap.instance_handle()) }?;
    Ok(unsafe { bootstrap.attach_surface(surface, platform.pixel_extent(), 0) }?)
}

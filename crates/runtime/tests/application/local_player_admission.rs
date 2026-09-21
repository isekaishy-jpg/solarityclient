//! Local worker admission preserves source identity, live motion and publication.

use super::{add_unit, renderer, unit_presentation};
use crate::application::player_coordinator::{RuntimePlayerPoll, RuntimePlayerPresentation};
use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::SdlPlatform;
use crate::test_support::SDL_TEST_LOCK;
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{
    ActiveWorld, ObjectKind, PlayerViewState, WorldBootstrap, WorldMapId, WorldTransform,
};
use solarity_rendering::VulkanRenderer;
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{Arc, mpsc};
use std::time::Duration;

struct Notifier(mpsc::Sender<()>);
impl solarity_cpu::CoordinatorNotifier for Notifier {
    fn notify(&self) {
        let _ = self.0.send(());
    }
}

fn executor(capacity: usize) -> Result<(CpuExecutor, mpsc::Receiver<()>), Box<dyn Error>> {
    let (send, receive) = mpsc::channel();
    Ok((
        CpuExecutor::with_notifier(
            CpuPoolConfig::new(
                {
                    let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
                    solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1).unwrap_or_else(
                        |_| unreachable!("one flexible worker fits a nonzero total"),
                    )
                },
                NonZeroUsize::new(capacity).ok_or("capacity")?,
                solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
            ),
            Arc::new(Notifier(send)),
        )?,
        receive,
    ))
}

fn world() -> Result<ActiveWorld, Box<dyn Error>> {
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    add_unit(&mut world, 7, ObjectKind::Player, 0)?;
    Ok(world)
}

fn mount(world: &mut ActiveWorld, display: u32) -> Result<(), Box<dyn Error>> {
    world.update_fields(7, [(69, display)])?;
    solarity_systems::project_object_fields(world, 7, [(69, display)])?;
    Ok(())
}

/// Wait only for admitted CPU work. GPU warmup advances in finite main turns.
fn publish(
    presentation: &mut RuntimePlayerPresentation,
    world: &ActiveWorld,
    cpu: &CpuExecutor,
    renderer: &mut VulkanRenderer,
    ready: &mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..128 {
        if presentation.synchronize_local_async(Some(world), cpu, renderer)?
            == RuntimePlayerPoll::ModelLoaded
        {
            return Ok(());
        }
        if presentation.local_preparation_pending() {
            ready.recv_timeout(Duration::from_secs(10))?;
        }
    }
    Err("local appearance did not publish".into())
}

fn same_output(
    actual: &RuntimePlayerPresentation,
    expected: &RuntimePlayerPresentation,
) -> Result<(), Box<dyn Error>> {
    let a = actual.resident_frame_input().ok_or("async resident")?;
    let b = expected.resident_frame_input().ok_or("serial resident")?;
    assert_eq!(a.model().path(), b.model().path());
    assert_eq!(a.atlas(), b.atlas());
    assert_eq!(a.geosets(), b.geosets());
    assert_eq!(a.world_transform(), b.world_transform());
    assert_eq!(a.object_scale(), b.object_scale());
    assert_eq!(a.animation(), b.animation());
    assert_eq!(a.attachments().len(), b.attachments().len());
    assert_eq!(
        a.mount().map(|m| m.model().path()),
        b.mount().map(|m| m.model().path())
    );
    assert_eq!(actual.collision_extent(), expected.collision_extent());
    assert_eq!(actual.camera_pose(), expected.camera_pose());
    assert_eq!(
        actual.camera_subject_height(),
        expected.camera_subject_height()
    );
    Ok(())
}

#[test]
fn local_shared_primary_wait_uses_no_worker_and_publishes_latest_motion()
-> Result<(), Box<dyn Error>> {
    exercise_shared_primary(false)
}

#[test]
fn local_shared_primary_failure_returns_the_cache_for_the_next_request()
-> Result<(), Box<dyn Error>> {
    exercise_shared_primary(true)
}

fn exercise_shared_primary(abandon: bool) -> Result<(), Box<dyn Error>> {
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = solarity_asset::AssetResourceKey::new(
        catalog.namespace(),
        solarity_asset::AssetPath::new("Character/Human/Male/HumanMale.m2")?,
    );
    let solarity_asset::M2Load::Producer(producer) = catalog.model_cache_service().request(&key)?
    else {
        return Err("producer".into());
    };
    let observer = producer.subscribe();
    let skin_key = solarity_asset::AssetResourceKey::new(
        catalog.namespace(),
        solarity_asset::AssetPath::new("Character/Human/Male/Skin.blp")?,
    );
    let solarity_asset::BlpLoad::Producer(skin_producer) = catalog
        .texture_cache_service()
        .request_for(&skin_key, solarity_cpu::CpuService::Speculative)
    else {
        return Err("skin producer".into());
    };
    let mut presentation = unit_presentation(&fixture)?.with_glue_worker_catalog(catalog.clone());
    let mut world = world()?;
    mount(&mut world, 102)?;
    world.update_fields(7, [(4, 0.5_f32.to_bits())])?;
    solarity_systems::project_object_fields(&mut world, 7, [(4, 0.5_f32.to_bits())])?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let (mut cpu, ready) = executor(2)?;
    let occupied = [cpu.try_reserve()?, cpu.try_reserve()?];
    assert_eq!(
        presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
        RuntimePlayerPoll::Pending
    );
    assert!(!presentation.local_preparation_pending());
    assert!(presentation.resident_model().is_none());
    assert!(
        presentation.collision_extent().is_some(),
        "table dimensions do not wait for visual resources"
    );
    drop(occupied);
    for _ in 0..3 {
        assert_eq!(
            presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
            RuntimePlayerPoll::Pending
        );
        assert!(presentation.local_preparation_pending());
    }
    assert_eq!(
        cpu.try_reserve()?.submit(|| 41).join()?,
        41,
        "waiting source occupies no worker"
    );
    assert!(observer.poll().is_none());
    let producer = if abandon {
        drop(producer);
        while presentation.local_preparation_pending() {
            ready.recv_timeout(Duration::from_secs(10))?;
        }
        assert!(matches!(
            presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer),
            Err(
                crate::application::player_coordinator::RuntimePlayerError::ModelRequest(
                    solarity_asset::M2LoadError::Abandoned
                )
            )
        ));
        let solarity_asset::M2Load::Producer(next) = catalog.model_cache_service().request(&key)?
        else {
            return Err("replacement producer".into());
        };
        assert_eq!(
            presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
            RuntimePlayerPoll::Pending
        );
        next
    } else {
        producer
    };
    let moved = WorldTransform::new(Vec3::new(3., 4., 5.), 0.75);
    world.update_transform(7, moved)?;
    world.set_local_player_view(PlayerViewState::new(8., 0.2, 0.3, 2))?;
    let model = producer.load(&mut solarity_asset::AssetStore::mount(catalog.clone())?)?;
    assert!(
        presentation.resident_model().is_none(),
        "shared body texture still gates publication"
    );
    let mut reader = solarity_asset::AssetStore::mount(catalog)?;
    let policy = solarity_asset::AssetReadBudget::for_service(
        cpu.storage().clone(),
        solarity_cpu::CpuService::Required,
    );
    cpu.try_submit(move || skin_producer.load(&mut reader, &policy))?
        .join()??;
    publish(&mut presentation, &world, &cpu, &mut renderer, &ready)?;
    assert!(solarity_asset::ResourceLease::ptr_eq(
        &model,
        presentation.resident_model().ok_or("resident")?
    ));
    let mut serial = unit_presentation(&fixture)?;
    assert_eq!(
        serial.synchronize(Some(&world))?,
        RuntimePlayerPoll::ModelLoaded
    );
    same_output(&presentation, &serial)?;
    assert_eq!(
        presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
        RuntimePlayerPoll::Current
    );
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn local_replacement_keeps_live_motion_and_reverted_request_cannot_publish()
-> Result<(), Box<dyn Error>> {
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut presentation = unit_presentation(&fixture)?.with_glue_worker_catalog(catalog);
    let mut serial = unit_presentation(&fixture)?;
    let mut world = world()?;
    presentation.synchronize(Some(&world))?;
    serial.synchronize(Some(&world))?;
    presentation.apply_mount_camera_sample(None, 850.)?;
    serial.apply_mount_camera_sample(None, 850.)?;
    let generation = presentation
        .resident_frame_input()
        .ok_or("resident")?
        .generation()
        .clone();
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let (mut cpu, ready) = executor(2)?;
    let (release, wait) = mpsc::channel();
    let (started, running) = mpsc::channel();
    let blocker = cpu.try_reserve()?.submit(move || {
        let _ = started.send(());
        wait.recv()
    });
    running.recv_timeout(Duration::from_secs(10))?;
    mount(&mut world, 102)?;
    assert_eq!(
        presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
        RuntimePlayerPoll::Current
    );
    assert!(presentation.local_preparation_pending());
    let moved = WorldTransform::new(Vec3::new(7., 8., 9.), 1.);
    world.update_transform(7, moved)?;
    world.set_local_player_view(PlayerViewState::new(10., 0.3, -0.5, 2))?;
    assert_eq!(
        presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
        RuntimePlayerPoll::Current
    );
    assert_eq!(
        presentation
            .resident_frame_input()
            .ok_or("moving resident")?
            .world_transform(),
        moved
    );
    mount(&mut world, 0)?;
    assert_eq!(
        presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
        RuntimePlayerPoll::Current
    );
    release.send(())?;
    blocker.join()??;
    while presentation.local_preparation_pending() {
        ready.recv_timeout(Duration::from_secs(10))?;
    }
    presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?;
    assert!(
        presentation
            .resident_frame_input()
            .ok_or("retained resident")?
            .generation()
            .matches(&generation)
    );
    serial.synchronize(Some(&world))?;
    same_output(&presentation, &serial)?;
    mount(&mut world, 102)?;
    publish(&mut presentation, &world, &cpu, &mut renderer, &ready)?;
    serial.synchronize(Some(&world))?;
    same_output(&presentation, &serial)?;
    assert!(
        !presentation
            .resident_frame_input()
            .ok_or("replaced resident")?
            .generation()
            .matches(&generation)
    );
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn local_world_withdrawal_and_reused_guid_reject_the_previous_request() -> Result<(), Box<dyn Error>>
{
    let _guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL lock")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = solarity_asset::AssetResourceKey::new(
        catalog.namespace(),
        solarity_asset::AssetPath::new("Character/Human/Male/HumanMale.m2")?,
    );
    let solarity_asset::M2Load::Producer(producer) = catalog.model_cache_service().request(&key)?
    else {
        return Err("producer".into());
    };
    let observer = producer.subscribe();
    let mut presentation = unit_presentation(&fixture)?.with_glue_worker_catalog(catalog.clone());
    let mut world = world()?;
    let original = world.object_identity(7).ok_or("identity")?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let (mut cpu, ready) = executor(2)?;
    presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?;
    assert_eq!(
        presentation.synchronize_local_async(None, &cpu, &mut renderer)?,
        RuntimePlayerPoll::Idle
    );
    assert!(observer.poll().is_none());
    assert!(presentation.collision_extent().is_none());
    while presentation.local_preparation_pending() {
        ready.recv_timeout(Duration::from_secs(10))?;
    }
    // A local player leaves by replacing ActiveWorld, never by an out-of-range removal.
    world = self::world()?;
    assert_ne!(world.object_identity(7), Some(original));
    world.update_transform(7, WorldTransform::new(Vec3::splat(15.), 2.))?;
    assert_eq!(
        presentation.synchronize_local_async(Some(&world), &cpu, &mut renderer)?,
        RuntimePlayerPoll::Pending
    );
    producer.load(&mut solarity_asset::AssetStore::mount(catalog)?)?;
    publish(&mut presentation, &world, &cpu, &mut renderer, &ready)?;
    let mut serial = unit_presentation(&fixture)?;
    serial.synchronize(Some(&world))?;
    same_output(&presentation, &serial)?;
    cpu.shutdown()?;
    Ok(())
}

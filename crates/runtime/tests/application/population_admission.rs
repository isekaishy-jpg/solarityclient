//! Unpublished appearance work cannot retire a still-live population generation.

use super::{add_unit, renderer, unit_presentation};
use crate::application::player_coordinator::{RuntimeCreaturePoll, RuntimeRemotePlayerPoll};
use crate::configuration::{WindowConfiguration, WindowMode};
use crate::platform::SdlPlatform;
use crate::test_support::SDL_TEST_LOCK;
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::{Arc, mpsc};
use std::time::Duration;

/// Test synchronization uses the same durable completion notification as runtime.
struct PopulationNotifier(mpsc::Sender<()>);

impl solarity_cpu::CoordinatorNotifier for PopulationNotifier {
    fn notify(&self) {
        let _ = self.0.send(());
    }
}

#[test]
fn population_joins_shared_primary_without_workers_and_rejoins_after_withdrawal()
-> Result<(), Box<dyn Error>> {
    exercise_shared_primary(SourceLifecycle::Publish)
}

#[test]
fn population_recovers_shared_primary_failure_before_publication() -> Result<(), Box<dyn Error>> {
    exercise_shared_primary(SourceLifecycle::AbandonThenPublish)
}

/// Both source outcomes use the same real NPC/player loading and publication path.
enum SourceLifecycle {
    Publish,
    AbandonThenPublish,
}

/// Keeps source readiness under test control while the real worker pool advances.
fn exercise_shared_primary(lifecycle: SourceLifecycle) -> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = solarity_asset::AssetResourceKey::new(
        catalog.namespace(),
        solarity_asset::AssetPath::new("Character/Human/Male/HumanMale.m2")?,
    );
    let service = catalog.model_cache_service();
    let solarity_asset::M2Load::Producer(producer) = service.request(&key)? else {
        return Err("new source needs a producer".into());
    };
    let observer = producer.subscribe();
    let mut presentation = unit_presentation(&fixture)?.with_glue_worker_catalog(catalog.clone());
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    add_unit(&mut world, 40, ObjectKind::Player, 0)?;
    let original = world.object_identity(30).ok_or("original identity")?;
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let (notify, ready) = mpsc::channel();
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::new(3).ok_or("three admissions")?,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ),
        Arc::new(PopulationNotifier(notify)),
    )?;
    let saturated = (0..3)
        .map(|_| cpu.try_reserve())
        .collect::<Result<Vec<_>, _>>()?;
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?;
    assert!(
        !presentation.population_preparation_pending(),
        "denied admission retains both banks without a partial phase"
    );
    drop(saturated);
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?;
    assert!(presentation.population_preparation_pending());
    assert!(observer.poll().is_none());
    assert!(presentation.resident_creature_frame_inputs().is_empty());
    assert!(
        presentation
            .resident_remote_player_frame_inputs()
            .is_empty()
    );
    assert_eq!(
        cpu.try_reserve()?.submit(|| 42).join()?,
        42,
        "shared readiness occupies no worker"
    );

    world.remove_object(30)?;
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    assert!(
        observer.poll().is_none(),
        "one withdrawal cannot abandon the shared producer"
    );
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    assert_ne!(world.object_identity(30), Some(original));
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    let producer = match lifecycle {
        SourceLifecycle::Publish => producer,
        SourceLifecycle::AbandonThenPublish => {
            drop(producer);
            while presentation.population_preparation_pending() {
                ready.recv_timeout(Duration::from_secs(10))?;
            }
            assert!(matches!(
                presentation.synchronize_creatures_async(
                    Some(&world),
                    |_| None,
                    &cpu,
                    &mut renderer
                ),
                Err(
                    crate::application::player_coordinator::RuntimePlayerError::ModelRequest(
                        solarity_asset::M2LoadError::Abandoned
                    )
                )
            ));
            assert!(matches!(
                presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer),
                Err(
                    crate::application::player_coordinator::RuntimePlayerError::ModelRequest(
                        solarity_asset::M2LoadError::Abandoned
                    )
                )
            ));
            assert!(!presentation.population_preparation_pending());
            let solarity_asset::M2Load::Producer(next) = service.request(&key)? else {
                return Err("failed source must release producer authority".into());
            };
            presentation.synchronize_creatures_async(
                Some(&world),
                |_| None,
                &cpu,
                &mut renderer,
            )?;
            presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?;
            next
        }
    };
    let model = producer.load(&mut solarity_asset::AssetStore::mount(catalog)?)?;
    let mut npc_ready = false;
    let mut player_ready = false;
    for _ in 0..128 {
        if !npc_ready {
            npc_ready = presentation.synchronize_creatures_async(
                Some(&world),
                |_| None,
                &cpu,
                &mut renderer,
            )? == RuntimeCreaturePoll::ModelsChanged;
        }
        if !player_ready {
            player_ready =
                presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?
                    == RuntimeRemotePlayerPoll::ModelsChanged;
        }
        if npc_ready && player_ready {
            break;
        }
        if presentation.population_preparation_pending() {
            ready.recv_timeout(Duration::from_secs(10))?;
        }
    }
    assert!(npc_ready && player_ready);
    assert!(solarity_asset::ResourceLease::ptr_eq(
        &model,
        presentation.resident_creature_frame_inputs()[0].model()
    ));
    assert!(solarity_asset::ResourceLease::ptr_eq(
        &model,
        presentation.resident_remote_player_frame_inputs()[0].model()
    ));
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn saturated_population_admission_preserves_active_generations_until_replacement()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut presentation = unit_presentation(&fixture)?.with_glue_worker_catalog(catalog);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for (guid, kind) in [
        (30, ObjectKind::Unit),
        (31, ObjectKind::Unit),
        (40, ObjectKind::Player),
        (41, ObjectKind::Player),
    ] {
        add_unit(&mut world, guid, kind, 0)?;
    }
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    presentation.synchronize_remote_players(Some(&world))?;
    let creatures = presentation
        .resident_creature_frame_inputs()
        .iter()
        .map(|resident| resident.generation().clone())
        .collect::<Vec<_>>();
    let players = presentation
        .resident_remote_player_frame_inputs()
        .iter()
        .map(|resident| resident.generation().clone())
        .collect::<Vec<_>>();
    assert_eq!(creatures.len(), 2);
    assert_eq!(players.len(), 2);
    for guid in [30, 31, 40, 41] {
        world.update_fields(guid, [(69, 102)])?;
        solarity_systems::project_object_fields(&mut world, guid, [(69, 102)])?;
    }
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let occupied = cpu.try_reserve()?;
    for _ in 0..3 {
        assert_eq!(
            presentation.synchronize_creatures_async(
                Some(&world),
                |_| None,
                &cpu,
                &mut renderer
            )?,
            RuntimeCreaturePoll::Current
        );
        assert_eq!(
            presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?,
            RuntimeRemotePlayerPoll::Current
        );
        let current = presentation.resident_creature_frame_inputs();
        assert_eq!(current.len(), creatures.len());
        assert!(
            current
                .iter()
                .zip(&creatures)
                .all(|(resident, generation)| resident.generation().matches(generation))
        );
        let current = presentation.resident_remote_player_frame_inputs();
        assert_eq!(current.len(), players.len());
        assert!(
            current
                .iter()
                .zip(&players)
                .all(|(resident, generation)| resident.generation().matches(generation))
        );
    }
    world.remove_object(31)?;
    world.remove_object(41)?;
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?;
    assert_eq!(presentation.resident_creature_frame_inputs().len(), 1);
    assert_eq!(presentation.resident_remote_player_frame_inputs().len(), 1);
    drop(occupied);
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn prepared_population_workers_publish_complete_mounts_with_current_motion()
-> Result<(), Box<dyn Error>> {
    let _sdl_guard = SDL_TEST_LOCK.lock().map_err(|_| "SDL test lock poisoned")?;
    let fixture = crate::test_support::unit_models::fixture_with_mount_effects()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut presentation = unit_presentation(&fixture)?.with_glue_worker_catalog(catalog);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    add_unit(&mut world, 30, ObjectKind::Unit, 0)?;
    add_unit(&mut world, 40, ObjectKind::Player, 0)?;
    presentation.synchronize_creatures(Some(&world), |_| None)?;
    presentation.synchronize_remote_players(Some(&world))?;
    let creature = presentation.resident_creature_frame_inputs()[0]
        .generation()
        .clone();
    let player = presentation.resident_remote_player_frame_inputs()[0]
        .generation()
        .clone();
    for guid in [30, 40] {
        world.update_fields(guid, [(69, 102)])?;
        solarity_systems::project_object_fields(&mut world, guid, [(69, 102)])?;
    }
    let platform = SdlPlatform::start(WindowConfiguration::new(128, 128, WindowMode::Windowed))?;
    let mut renderer = renderer(&platform)?;
    let slots = NonZeroUsize::new(2).ok_or("two worker slots")?;
    let (notify, ready) = mpsc::channel();
    let mut cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            {
                let total: std::num::NonZeroUsize = slots;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            slots,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ),
        Arc::new(PopulationNotifier(notify)),
    )?;
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?;
    // Movement can change while CPU source dependencies are pending. Completion
    // wakes main; it does not authorize publishing the earlier transform.
    let transform = solarity_ecs::WorldTransform::new(Vec3::new(3., 4., 5.), 0.5);
    for guid in [30, 40] {
        world.update_transform(guid, transform)?;
    }
    let mut creature_published = false;
    let mut player_published = false;
    for _ in 0..64 {
        if !creature_published {
            creature_published = presentation.synchronize_creatures_async(
                Some(&world),
                |_| None,
                &cpu,
                &mut renderer,
            )? == RuntimeCreaturePoll::ModelsChanged;
        }
        if !player_published {
            player_published =
                presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?
                    == RuntimeRemotePlayerPoll::ModelsChanged;
        }
        if creature_published && player_published {
            break;
        }
        if presentation.population_preparation_pending() {
            ready.recv_timeout(Duration::from_secs(10))?;
        }
    }
    assert!(creature_published && player_published);
    let creatures = presentation.resident_creature_frame_inputs();
    assert_eq!(creatures.len(), 1);
    assert!(!creatures[0].generation().matches(&creature));
    assert!(creatures[0].mount().is_some());
    assert_eq!(creatures[0].world_transform(), transform);
    let players = presentation.resident_remote_player_frame_inputs();
    assert_eq!(players.len(), 1);
    assert!(!players[0].generation().matches(&player));
    assert!(players[0].mount().is_some());
    assert_eq!(players[0].world_transform(), transform);
    cpu.shutdown()?;
    Ok(())
}

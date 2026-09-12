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
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
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
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(slots, slots))?;
    presentation.synchronize_creatures_async(Some(&world), |_| None, &cpu, &mut renderer)?;
    presentation.synchronize_remote_players_async(Some(&world), &cpu, &mut renderer)?;
    // Executor shutdown joins the finite CPU jobs without a timing-dependent poll.
    // Their completed immutable results still require render-owner admission.
    cpu.shutdown()?;
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
    Ok(())
}

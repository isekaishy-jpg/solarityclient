//! Retired task failures cannot escape into a replacement world's admission.

use super::{
    PendingGeneration, ResourceRequest, RuntimeGameObjectError, RuntimeGameObjectPresentation,
    RuntimeGameObjectResourceKind,
};
use crate::test_support::ClientFixture;
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, AssetStoreHandle, ClientDataRoot,
    GameObjectDisplayCatalog, Locale,
};
use solarity_cpu::{CpuError, CpuExecutor, CpuPoolConfig};
use std::error::Error;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn fishing_hole_opacity_follows_model_admission_and_independent_lifetimes()
-> Result<(), Box<dyn Error>> {
    use crate::test_support::game_object_models as models;
    use glam::Vec3;
    use solarity_ecs::{ActiveWorld, ObjectKind, WorldBootstrap, WorldMapId, WorldTransform};
    let fixture = ClientFixture::with_common_files(&[
        (
            "World\\GameObject.m2",
            &models::model_with_animations(&[0])?,
        ),
        ("World\\GameObject00.skin", &models::skin()?),
        (
            "DBFilesClient\\GameObjectDisplayInfo.dbc",
            &models::displays(),
        ),
    ])?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut owner =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(0),
        7,
        "Local",
        Vec3::ZERO,
        0.,
    ));
    for (guid, dynamic) in [(20, 0), (30, 2)] {
        world.create_object(
            guid,
            ObjectKind::GameObject,
            Some(WorldTransform::new(Vec3::ZERO, 0.)),
            [],
        )?;
        solarity_systems::project_object_fields(
            &mut world,
            guid,
            [
                (4, 1),
                (5, 1_f32.to_bits()),
                (8, 42),
                (14, dynamic),
                (17, 31 << 8 | 1),
            ],
        )?;
    }
    let mut random = crate::random::CrtRand::new();
    owner.synchronize(Some(&world))?;
    owner.synchronize_animations(Some(&world), &mut random)?;
    let first = std::rc::Rc::clone(owner.instances[0].opacity_owner());
    let second = std::rc::Rc::clone(owner.instances[1].opacity_owner());
    assert_eq!(first.opacity(), 0.);
    assert_eq!(second.opacity(), 0.);
    owner
        .frame_input(Some(&world))
        .advance_scene(500., &mut random)?;
    assert!((first.opacity() - 64. / 255.).abs() < 1e-6);
    assert!((second.opacity() - 127. / 255.).abs() < 1e-6);
    // Field changes cannot replay the model's admission callback.
    solarity_systems::project_object_fields(&mut world, 20, [(14, 2)])?;
    owner.synchronize(Some(&world))?;
    owner.synchronize_animations(Some(&world), &mut random)?;
    owner
        .frame_input(Some(&world))
        .advance_scene(1000., &mut random)?;
    assert!((first.opacity() - 128. / 255.).abs() < 1e-6);
    assert_eq!(second.opacity(), 1.);
    // A changed display reselects the target on the retained object owner.
    solarity_systems::project_object_fields(&mut world, 20, [(8, 43)])?;
    owner.synchronize(Some(&world))?;
    owner.synchronize_animations(Some(&world), &mut random)?;
    owner
        .frame_input(Some(&world))
        .advance_scene(1500., &mut random)?;
    assert!((first.opacity() - 191. / 255.).abs() < 1e-6);
    Ok(())
}

#[test]
fn game_object_task_panics_only_fail_the_owning_world() -> Result<(), Box<dyn Error>> {
    let mut displays = b"WDBC".to_vec();
    for word in [0_u32, 19, 76, 1] {
        displays.extend_from_slice(&word.to_le_bytes());
    }
    displays.push(0);
    let fixture = ClientFixture::with_common_files(&[(
        "DBFilesClient\\GameObjectDisplayInfo.dbc",
        &displays,
    )])?;
    let archive =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(archive)?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut owner =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations);
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    for retired in [false, true] {
        let task = cpu.try_submit(|| panic!("injected GameObject worker failure"))?;
        owner.pending = Some(PendingGeneration {
            model_demand: None,
            request: ResourceRequest {
                kind: RuntimeGameObjectResourceKind::M2,
                path: AssetPath::new("World\\Failed.m2")?,
            },
            eligible: true,
            task: super::PendingTask::Direct(task),
        });
        if retired {
            owner.disconnect();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !owner
            .pending
            .as_ref()
            .is_some_and(|pending| pending.task.is_finished())
        {
            if Instant::now() >= deadline {
                return Err("injected worker did not finish".into());
            }
            std::thread::yield_now();
        }
        let result = owner.finish_pending();
        if retired {
            result?;
        } else {
            assert!(matches!(
                result,
                Err(RuntimeGameObjectError::Cpu(CpuError::TaskPanicked))
            ));
        }
        assert!(owner.pending.is_none());
    }
    cpu.shutdown()?;
    Ok(())
}

/// Shared source readiness precedes worker dispatch and never admits an object by itself.
#[test]
fn game_object_source_wait_survives_world_withdrawal_without_blocking_a_worker()
-> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[
        (
            "World/GameObject.m2",
            &crate::test_support::game_object_models::model_with_animations(&[0])?,
        ),
        (
            "World/GameObject00.skin",
            &crate::test_support::game_object_models::skin()?,
        ),
        (
            "DBFilesClient/GameObjectDisplayInfo.dbc",
            &crate::test_support::game_object_models::displays(),
        ),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let displays = GameObjectDisplayCatalog::load(&mut store)?;
    let animations = Arc::new(AnimationDataCatalog::load(&mut store)?);
    let mut owner =
        RuntimeGameObjectPresentation::new(AssetStoreHandle::new(store), displays, animations)
            .with_worker_catalog(catalog.clone());
    let key = solarity_asset::AssetResourceKey::new(
        catalog.namespace(),
        AssetPath::new("World/GameObject.m2")?,
    );
    let solarity_asset::M2Load::Producer(producer) = catalog.model_cache_service().request(&key)?
    else {
        return Err("missing producer".into());
    };
    let mut world = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(0),
        7,
        "Local",
        glam::Vec3::ZERO,
        0.,
    ));
    create_shared_model_object(&mut world)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or("positive capacity required")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let occupied_a = cpu.try_reserve()?;
    let occupied_b = cpu.try_reserve()?;
    owner.synchronize_async(Some(&world), &cpu)?;
    assert!(owner.pending.is_none());
    assert!(owner.model_wait.is_some());
    assert!(owner.instances[0].resource.is_none());
    owner.synchronize_async(None, &cpu)?;
    assert!(owner.model_wait.is_none());
    owner.synchronize_async(Some(&world), &cpu)?;
    assert!(owner.model_wait.is_some());
    drop((occupied_a, occupied_b));
    let mut reader = AssetStore::mount(catalog)?;
    // Bind the dependent phase while another admitted owner controls publication.
    owner.synchronize_async(Some(&world), &cpu)?;
    assert!(owner.pending.is_some());
    assert!(owner.model_wait.is_none());
    // Losing the last object consumer cancels its derived phase without touching
    // the independently owned source producer. A replacement lifetime can rejoin.
    world.remove_object(20)?;
    owner.synchronize_async(Some(&world), &cpu)?;
    assert!(owner.instances.is_empty());
    // Resumable cancellation returns the bank on a retirement turn. It must
    // finish while the independent source producer is still unpublished.
    let deadline = Instant::now() + Duration::from_secs(5);
    while owner.pending.is_some() {
        if Instant::now() >= deadline {
            return Err("withdrawn source consumer did not return its bank".into());
        }
        std::thread::sleep(Duration::from_millis(1));
        owner.synchronize_async(Some(&world), &cpu)?;
    }
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    create_shared_model_object(&mut world)?;
    owner.synchronize_async(Some(&world), &cpu)?;
    assert!(owner.pending.is_some());
    let produced = cpu
        .try_submit(move || producer.load(&mut reader))?
        .join()??;
    // No coordinator poll is needed between source publication and useful work.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner
        .pending
        .as_ref()
        .is_some_and(|pending| pending.task.is_finished())
    {
        if Instant::now() >= deadline {
            return Err("dependent preparation did not complete without a coordinator poll".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    owner.synchronize_async(Some(&world), &cpu)?;
    let resource = owner.instances[0]
        .resource
        .as_ref()
        .ok_or("model was not published")?;
    let super::GameObjectResource::M2(source) = resource.as_ref() else {
        return Err("wrong source kind".into());
    };
    assert!(solarity_asset::ResourceLease::ptr_eq(
        &produced,
        source.model()
    ));
    assert!(owner.model_wait.is_none());

    // An abandoned dependency after world withdrawal must return the exact
    // mounted bank and must not report an error into the replacement world.
    let bank = std::ptr::from_ref(owner.worker.as_deref().ok_or("missing mounted bank")?);
    let catalog = owner.worker_catalog.as_ref().ok_or("missing catalog")?;
    let path = AssetPath::new("World/Abandoned.m2")?;
    let key = solarity_asset::AssetResourceKey::new(catalog.namespace(), path.clone());
    let solarity_asset::M2Load::Producer(producer) = catalog.model_cache_service().request(&key)?
    else {
        return Err("missing producer".into());
    };
    let request = ResourceRequest {
        kind: RuntimeGameObjectResourceKind::M2,
        path,
    };
    owner.model_wait = Some((request.clone(), producer.subscribe()));
    owner.start_model_dependency(&cpu, request)?;
    owner.disconnect();
    drop(producer);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !owner
        .pending
        .as_ref()
        .is_some_and(|pending| pending.task.is_finished())
    {
        if Instant::now() >= deadline {
            return Err("failed dependency did not complete".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    owner.finish_pending()?;
    assert_eq!(
        bank,
        std::ptr::from_ref(
            owner
                .worker
                .as_deref()
                .ok_or("bank lost on dependency failure")?
        )
    );
    assert!(owner.instances.is_empty());
    cpu.shutdown()?;
    Ok(())
}

/// Consumer withdrawal cannot temporarily demote a producer still needed by another owner.
#[test]
fn retiring_game_object_leaves_shared_producer_priority_with_combined_demand()
-> Result<(), Box<dyn Error>> {
    use solarity_cpu::{CpuService, CpuServiceDemand};
    use std::sync::mpsc;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(2).ok_or("capacity")?,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let blocker = cpu.try_submit(move || {
        entered_tx.send(()).ok();
        release_rx.recv_timeout(Duration::from_secs(10)).ok();
    })?;
    entered_rx.recv_timeout(Duration::from_secs(5))?;
    let task = cpu.try_submit(|| super::GameObjectWorkerCompletion {
        worker: None,
        result: Err(solarity_asset::M2LoadError::Abandoned.into()),
    })?;
    let control = task.service_control();
    let demand = CpuServiceDemand::default();
    let own = demand.subscribe(CpuService::Required);
    let other = demand.subscribe(CpuService::Required);
    assert!(demand.bind(control.clone()));
    let mut pending = super::PendingTask::Direct(task);
    pending.retire();
    let before_own_withdrawal = control.service();
    own.set_service(CpuService::Retirement);
    let with_other_consumer = control.service();
    drop(other);
    let without_other_consumer = control.service();
    release_tx.send(())?;
    blocker.join()?;
    let _completion = pending.join()?;
    assert_eq!(before_own_withdrawal, CpuService::Required);
    assert_eq!(with_other_consumer, CpuService::Required);
    assert_eq!(without_other_consumer, CpuService::Retirement);
    cpu.shutdown()?;
    Ok(())
}

/// Recreates the exact authored display under a fresh visible object lifetime.
fn create_shared_model_object(world: &mut solarity_ecs::ActiveWorld) -> Result<(), Box<dyn Error>> {
    world.create_object(
        20,
        solarity_ecs::ObjectKind::GameObject,
        Some(solarity_ecs::WorldTransform::new(glam::Vec3::ZERO, 0.)),
        [],
    )?;
    solarity_systems::project_object_fields(
        world,
        20,
        [
            (4, 1),
            (5, 1_f32.to_bits()),
            (8, 42),
            (14, 0),
            (17, 31 << 8 | 1),
        ],
    )?;
    Ok(())
}

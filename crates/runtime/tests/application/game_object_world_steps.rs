//! Root and MODD waits keep one ordered result and return the same bank after withdrawal.

use super::super::{GameObjectResource, ResourceRequest, RuntimeGameObjectResourceKind};
use super::{GameObjectWorkerSource, world_model_steps};
use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::test_support::{ClientFixture, game_object_models, game_object_world_models};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale, M2Load,
    ResourceLease, WmoLoad,
};
use solarity_cpu::{
    CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan, CpuTaskStep,
};
use std::{error::Error, num::NonZeroUsize, sync::mpsc};

/// One WMO and its referenced M2 exercise whole-consumer publication through real decoders.
fn fixture() -> Result<(ClientFixture, ArchiveCatalog), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[
        ("World/Attached.wmo", &game_object_world_models::root()),
        ("World/Attached_000.wmo", &game_object_world_models::group()),
        (
            "World/GameObject.m2",
            &game_object_models::model_with_animations(&[0])?,
        ),
        ("World/GameObject00.skin", &game_object_models::skin()?),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    Ok((fixture, catalog))
}

/// Other work runs on the sole worker while a root or nested M2 dependency is unresolved.
#[test]
fn world_steps_join_root_and_doodad_without_partial_publication() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog) = fixture()?;
    let path = AssetPath::new("World/Attached.wmo")?;
    let WmoLoad::Producer(root) = catalog.world_model_cache_service().request_for(
        &AssetResourceKey::new(catalog.namespace(), path.clone()),
        CpuService::Required,
    )?
    else {
        return Err("root producer".into());
    };
    let M2Load::Producer(doodad) =
        catalog
            .model_cache_service()
            .request(&AssetResourceKey::new(
                catalog.namespace(),
                AssetPath::new("World/GameObject.m2")?,
            ))?
    else {
        return Err("doodad producer".into());
    };
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut steps = world_model_steps(
        GameObjectWorkerSource::Catalog(catalog.clone()),
        ResourceRequest {
            kind: RuntimeGameObjectResourceKind::WorldModel,
            path,
        },
        shared,
    );
    let (notice, observed) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |context| {
        let next = steps(context);
        if matches!(next, CpuTaskStep::Wait(_)) {
            let _ = notice.send(());
        }
        next
    });
    observed.recv_timeout(std::time::Duration::from_secs(5))?;
    assert_eq!(cpu.try_submit(|| 17)?.join()?, 17);
    assert!(!task.is_finished());
    let mut reader = AssetStore::mount(catalog)?;
    let (root, mut reader) = cpu
        .try_submit(move || (root.load(&mut reader), reader))?
        .join()?;
    let root = root?;
    observed.recv_timeout(std::time::Duration::from_secs(5))?;
    assert!(!task.is_finished());
    let model = cpu.try_submit(move || doodad.load(&mut reader))?.join()??;
    let done = task.join()?;
    assert!(done.worker.is_some());
    let GameObjectResource::WorldModel(source) = done.result? else {
        return Err("wrong result kind".into());
    };
    assert!(ResourceLease::ptr_eq(&root, source.model()));
    assert_eq!(
        source.doodads().iter().map(|d| d.index).collect::<Vec<_>>(),
        [1, 0]
    );
    for doodad in source.doodads() {
        assert!(ResourceLease::ptr_eq(&model, doodad.source.model()));
    }
    cpu.shutdown()?;
    Ok(())
}

/// Cancelling a suspended root returns the mounted reader instead of waiting for source completion.
#[test]
fn world_steps_withdraw_suspended_root_and_keep_bank() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog) = fixture()?;
    let path = AssetPath::new("World/Attached.wmo")?;
    let WmoLoad::Producer(producer) = catalog.world_model_cache_service().request_for(
        &AssetResourceKey::new(catalog.namespace(), path.clone()),
        CpuService::Required,
    )?
    else {
        return Err("root producer".into());
    };
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(2).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut steps = world_model_steps(
        GameObjectWorkerSource::Catalog(catalog),
        ResourceRequest {
            kind: RuntimeGameObjectResourceKind::WorldModel,
            path,
        },
        shared,
    );
    let (notice, observed) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |context| {
        let next = steps(context);
        if matches!(next, CpuTaskStep::Wait(_)) {
            let _ = notice.send(());
        }
        next
    });
    observed.recv_timeout(std::time::Duration::from_secs(5))?;
    task.cancel();
    let done = task.join()?;
    assert!(done.worker.is_some());
    assert!(matches!(
        done.result,
        Err(super::super::RuntimeGameObjectError::Cpu(
            solarity_cpu::CpuError::JobCancelled
        ))
    ));
    drop(producer);
    cpu.shutdown()?;
    Ok(())
}

/// Withdrawing the initiating scene cannot abandon a source needed by another consumer.
#[test]
fn withdrawn_world_producer_finishes_joined_root_without_loading_doodads()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog) = fixture()?;
    let path = AssetPath::new("World/Attached.wmo")?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(2).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (gate_owner, gate) = solarity_cpu::SharedProduct::<(), ()>::new(
        1,
        cpu.storage(),
        solarity_cpu::CpuStorageClass::Required,
    )?;
    let mut gate_edge = Some(solarity_cpu::CpuTaskDependency::new(&gate.readiness())?);
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut steps = world_model_steps(
        GameObjectWorkerSource::Catalog(catalog.clone()),
        ResourceRequest {
            kind: RuntimeGameObjectResourceKind::WorldModel,
            path: path.clone(),
        },
        shared,
    );
    let (notice, observed) = mpsc::channel();
    let mut turns = 0;
    let task = permit.submit_resumable_with_context(move |context| {
        let next = steps(context);
        turns += 1;
        // Mount, claim producer, then decode the root. Suspend before its group.
        if turns == 3 && matches!(next, CpuTaskStep::Continue) {
            let _ = notice.send(());
            return CpuTaskStep::Wait(gate_edge.take().unwrap_or_else(|| unreachable!("one gate")));
        }
        next
    });
    let reached = observed.recv_timeout(std::time::Duration::from_secs(5));
    if reached.is_err() {
        task.cancel();
    }
    reached?;
    let WmoLoad::Pending(joiner) = catalog.world_model_cache_service().request_for(
        &AssetResourceKey::new(catalog.namespace(), path),
        CpuService::Required,
    )?
    else {
        task.cancel();
        return Err("shared producer disappeared".into());
    };
    task.cancel();
    let done = task.join()?;
    assert!(done.worker.is_some());
    assert!(matches!(
        done.result,
        Err(super::super::RuntimeGameObjectError::Cpu(
            solarity_cpu::CpuError::JobCancelled
        ))
    ));
    let root = joiner.poll().ok_or("source not published")??;
    assert_eq!(root.groups().len(), 1);
    assert!(matches!(
        catalog
            .model_cache_service()
            .request(&AssetResourceKey::new(
                catalog.namespace(),
                AssetPath::new("World/GameObject.m2")?,
            ))?,
        M2Load::Producer(_)
    ));
    drop((gate_owner, gate, joiner, root));
    cpu.shutdown()?;
    Ok(())
}

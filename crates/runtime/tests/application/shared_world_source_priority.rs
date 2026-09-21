//! Shared WMO priority belongs to its producer lifetime, including retirement.

use super::{PendingWorldModel, SharedTerrainSources};
use crate::test_support::{ClientFixture, game_object_world_models};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetResourceKey, AssetStore, ClientDataRoot, Locale, WmoLoad,
};
use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize, ops::ControlFlow};

/// A cancelled terrain owner drains the joined source at required priority, then
/// returns to retirement even if that source's consumers retain their handles.
#[test]
fn joined_wmo_keeps_priority_through_retirement_and_releases_it_at_publication()
-> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[
        ("World/Attached.wmo", &game_object_world_models::root()),
        ("World/Attached_000.wmo", &game_object_world_models::group()),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(1 << 20, 1 << 20, 1 << 20),
    ))?;
    let permit = cpu.try_reserve_for(CpuService::Speculative)?;
    let owner = permit.service_control();
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: owner.clone(),
    };
    let path = AssetPath::new("World/Attached.wmo")?;
    let mut pending = None;
    assert!(matches!(
        shared.world_model(&path, &mut pending, &mut store)?,
        ControlFlow::Continue(None)
    ));
    assert_eq!(owner.service(), CpuService::Speculative);
    let WmoLoad::Pending(other) = catalog.world_model_cache_service().request_for(
        &AssetResourceKey::new(catalog.namespace(), path),
        CpuService::Required,
    )?
    else {
        return Err("source should be pending".into());
    };
    assert_eq!(owner.service(), CpuService::Required);
    owner.set_service(CpuService::Retirement);
    assert_eq!(owner.service(), CpuService::Required);
    let mut stages = 0;
    while !PendingWorldModel::retire_step(&mut pending, &mut store) {
        stages += 1;
        assert_eq!(owner.service(), CpuService::Required);
        assert!(other.poll().is_none());
    }
    assert!(
        stages >= 2,
        "root and group yield before final source publication"
    );
    assert!(other.poll().ok_or("complete source")?.is_ok());
    assert_eq!(owner.service(), CpuService::Retirement);
    other.set_service(CpuService::Speculative);
    other.set_service(CpuService::Required);
    assert_eq!(owner.service(), CpuService::Retirement);
    drop(permit);
    Ok(())
}

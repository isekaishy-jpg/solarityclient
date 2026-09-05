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
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(NonZeroUsize::MIN, NonZeroUsize::MIN))?;
    for retired in [false, true] {
        let task = cpu.try_submit(|| panic!("injected GameObject worker failure"))?;
        owner.pending = Some(PendingGeneration {
            request: ResourceRequest {
                kind: RuntimeGameObjectResourceKind::M2,
                path: AssetPath::new("World\\Failed.m2")?,
            },
            eligible: true,
            task,
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

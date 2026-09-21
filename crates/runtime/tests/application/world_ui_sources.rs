//! UI source jobs preserve ownership through saturation and cancelled entry.

use std::{error::Error, num::NonZeroUsize, sync::mpsc};

use solarity_asset::{ArchiveCatalog, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};

use super::WorldUiSourcePreparation;
use crate::test_support::ClientFixture;

/// The loading caller remains nonblocking at capacity and holds exactly one
/// read-only result for a later entry when the current entry is cancelled.
#[test]
fn world_ui_sources_wait_for_capacity_and_retain_one_completed_image() -> Result<(), Box<dyn Error>>
{
    let mut difficulty = b"WDBC".to_vec();
    for field in [1_u32, 23, 92, 8] {
        difficulty.extend_from_slice(&field.to_le_bytes());
    }
    let mut row = [0_u32; 23];
    row[..4].copy_from_slice(&[1, 33, 2, 1]);
    for field in row {
        difficulty.extend_from_slice(&field.to_le_bytes());
    }
    difficulty.extend_from_slice(b"\0Denied\0");
    let fixture = ClientFixture::with_common_files(&[
        ("Interface/FrameXML/Bindings.xml", b"<Bindings/>"),
        ("WTF/DefaultBindings.wtf", b""),
        ("Interface/FrameXML/FrameXML.toc", b""),
        ("DBFilesClient/MapDifficulty.dbc", &difficulty),
        (
            "Textures/Minimap/md5translate.trs",
            b"World\\map0_00.blp\tdigest.blp\n",
        ),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let mut preparation = WorldUiSourcePreparation::default();
    preparation.request(&cpu, &catalog)?;
    preparation.poll()?;
    assert!(preparation.take().is_none());
    assert!(
        preparation.task.is_none(),
        "saturated submission must remain deferred"
    );
    release.send(())?;
    blocker.join()??;
    preparation.request(&cpu, &catalog)?;
    // Explicit shutdown/retirement join: no UI or world lifetime is required.
    preparation.finish()?;
    preparation.request(&cpu, &catalog)?;
    assert!(
        preparation.task.is_none(),
        "ready sources must prevent duplicate loading"
    );
    let sources = preparation.take().ok_or("world sources missing")?;
    assert_eq!(
        sources.transfer_messages.message(33, 8, Some(2))?,
        Some("Denied")
    );
    assert_eq!(sources.transfer_messages.message(33, 8, Some(1))?, None);
    assert_eq!(sources.transfer_messages.message(33, 3, Some(2))?, None);
    assert_eq!(
        sources
            .minimap?
            .texture(&solarity_asset::AssetPath::new("World/map0_00.blp")?),
        Some(&solarity_asset::AssetPath::new(
            "Textures/Minimap/digest.blp"
        )?)
    );
    assert!(preparation.take().is_none());
    preparation.finish()?;
    cpu.shutdown()?;
    Ok(())
}

#[test]
fn optional_world_metadata_errors_wait_for_their_original_consumers() -> Result<(), Box<dyn Error>>
{
    let fixture = ClientFixture::with_common_files(&[
        ("Interface/FrameXML/Bindings.xml", b"<Bindings/>"),
        ("WTF/DefaultBindings.wtf", b""),
        ("Interface/FrameXML/FrameXML.toc", b""),
        ("Textures/Minimap/md5translate.trs", &[0xff]),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let sources = std::thread::spawn(move || {
        let mut store = solarity_asset::AssetStore::mount(catalog)?;
        super::WorldUiSourceImage::load(&mut store)
    })
    .join()
    .map_err(|_| "source worker panicked")??;
    assert!(matches!(
        sources.minimap,
        Err(solarity_asset::AssetError::MinimapDecode { .. })
    ));
    assert!(sources.transfer_messages.message(33, 3, Some(2))?.is_none());
    assert!(sources.transfer_messages.message(33, 8, None)?.is_none());
    let first = sources.transfer_messages.message(33, 8, Some(2));
    let second = sources.transfer_messages.message(33, 8, Some(2));
    let (
        Err(super::super::RuntimeWorldUiError::TransferMetadata(first)),
        Err(super::super::RuntimeWorldUiError::TransferMetadata(second)),
    ) = (first, second)
    else {
        return Err("missing MapDifficulty must fail at the exact difficulty consumer".into());
    };
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert!(matches!(
        &*first,
        solarity_asset::AssetError::AssetNotFound { .. }
    ));
    Ok(())
}

/// A cancelled entry still observes its admitted archive error during shutdown.
#[test]
fn world_ui_source_retirement_observes_worker_failure_once() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let mut preparation = WorldUiSourcePreparation::default();
    preparation.request(&cpu, &catalog)?;
    assert!(preparation.finish().is_err(), "missing FrameXML must fail");
    preparation.finish()?;
    assert!(preparation.take().is_none());
    cpu.shutdown()?;
    Ok(())
}

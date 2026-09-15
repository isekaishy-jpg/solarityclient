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
    let fixture = ClientFixture::with_common_files(&[
        ("Interface/FrameXML/Bindings.xml", b"<Bindings/>"),
        ("WTF/DefaultBindings.wtf", b""),
        ("Interface/FrameXML/FrameXML.toc", b""),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
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
    assert!(preparation.take().is_some());
    assert!(preparation.take().is_none());
    preparation.finish()?;
    cpu.shutdown()?;
    Ok(())
}

/// A cancelled entry still observes its admitted archive error during shutdown.
#[test]
fn world_ui_source_retirement_observes_worker_failure_once() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        NonZeroUsize::MIN,
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

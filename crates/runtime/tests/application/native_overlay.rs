//! Fixed native font coverage survives worker/library retirement unchanged.

use super::*;
use solarity_asset::{ArchiveCatalog, AssetReadBudget, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuService, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize};

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn native_overlay_worker_matches_serial_after_font_owner_retirement() -> Result<(), Box<dyn Error>>
{
    fn transferable<T: Send + Sync>() {}
    transferable::<UiNativeTextAtlas>();
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut stock = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let face = stock.read(&AssetPath::new(FONT_PATH)?)?;
    let fixture =
        crate::test_support::ClientFixture::with_common_files(&[(FONT_PATH, face.bytes())])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let extent = (2560, 1440);
    let serial = PreparedFpsOverlay::load(&mut AssetStore::mount(catalog.clone())?, extent)?
        .ok_or("serial font")?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(8).ok_or("capacity")?,
        CpuStoragePlan::new(16 << 20, 16 << 20, 16 << 20),
    ))?;
    let policy = AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
    let prepared = cpu
        .try_submit(move || {
            AssetStore::mount(catalog)?
                .with_read_budget(&policy, |store| PreparedFpsOverlay::load(store, extent))
        })?
        .join()??
        .ok_or("worker font")?;
    cpu.shutdown()?;
    assert_eq!(prepared.atlas.rgba8(), serial.atlas.rgba8());
    assert_eq!(prepared.atlas.extent(), serial.atlas.extent());
    assert_eq!(prepared.plan.vertices(), serial.plan.vertices());
    assert_eq!(prepared.plan.indices(), serial.plan.indices());
    assert_eq!(prepared.style, serial.style);
    for text in [
        "-- FPS",
        "59.9 FPS",
        "120.0 FPS  RECORDING",
        "60.0 FPS  SAVING",
    ] {
        let mesh = |prepared: &PreparedFpsOverlay| {
            prepared.atlas.native_text_mesh(
                text,
                &prepared.style,
                prepared.logical_extent,
                FPS_TEXT_TOP_LEFT,
                FPS_TEXT_REGION_HEIGHT,
                extent.1,
            )
        };
        assert_eq!(mesh(&prepared)?.vertices(), mesh(&serial)?.vertices());
        assert_eq!(mesh(&prepared)?.indices(), mesh(&serial)?.indices());
        assert_eq!(
            prepared
                .atlas
                .native_text_width(text, &prepared.style, extent.1)?,
            serial
                .atlas
                .native_text_width(text, &serial.style, extent.1)?,
        );
    }
    assert!(
        prepared
            .atlas
            .native_text_width("?", &prepared.style, extent.1)
            .is_err()
    );
    assert!(
        prepared
            .atlas
            .native_text_width("FPS", &prepared.style, 720)
            .is_err()
    );
    Ok(())
}

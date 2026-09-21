//! Dynamic font work shares coverage and exact metrics after each native owner retires.

use super::*;
use crate::test_support::ClientFixture;
use solarity_asset::{AssetPath, ClientDataRoot, Locale};
use solarity_cpu::{
    CpuExecutionPlan, CpuPoolConfig, CpuStorageClass, CpuStorageKind, CpuStoragePlan,
};
use solarity_ui::{FontGlyphRequest, FontRasterization};
use std::{error::Error, num::NonZeroUsize, sync::mpsc};

fn config() -> Result<CpuPoolConfig, CpuError> {
    Ok(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::MIN,
        CpuStoragePlan::new(16 << 20, 64 << 20, 16 << 20),
    ))
}

fn offline(cpu: &CpuExecutor, catalog: ArchiveCatalog) -> FontSystem {
    FontSystem::with_executor(Rc::new(RuntimeFontPreparation {
        cpu: cpu.service_handle(),
        input: None,
        catalog,
        reader: Arc::new(Mutex::new(None)),
    }))
}

#[test]
fn font_source_errors_and_admission_never_fall_back_to_main() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[("Fonts/Invalid.ttf", b"invalid font")])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let cpu = CpuExecutor::new(config()?)?;
    let mut fonts = offline(&cpu, catalog.clone());
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Fonts/Invalid.ttf")?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    assert!(matches!(
        fonts.rasterize(&mut store, &path, 16, 'A', FontRasterization::Antialiased),
        Err(FontError::Asset(AssetError::SourceStorage(
            CpuError::AtCapacity { .. }
        )))
    ));
    release.send(())?;
    blocker.join()??;
    assert!(matches!(
        fonts.rasterize(&mut store, &path, 16, 'A', FontRasterization::Antialiased),
        Err(FontError::Face { .. })
    ));
    assert!(matches!(
        fonts.rasterize(
            &mut store,
            &AssetPath::new("Fonts/Missing.ttf")?,
            16,
            'A',
            FontRasterization::Antialiased
        ),
        Err(FontError::Asset(AssetError::AssetNotFound { .. }))
    ));
    assert_eq!(fonts.loaded_face_count(), 0);
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    drop(cpu);
    assert!(matches!(
        fonts.rasterize(&mut store, &path, 16, 'A', FontRasterization::Antialiased),
        Err(FontError::Asset(AssetError::SourceStorage(
            CpuError::ShuttingDown
        )))
    ));
    Ok(())
}

#[test]
fn retired_native_font_host_reclaims_its_task_and_preserves_input() -> Result<(), Box<dyn Error>> {
    let _sdl = crate::test_support::SDL_TEST_LOCK
        .lock()
        .map_err(|_| "SDL test lock poisoned")?;
    let fixture = ClientFixture::with_common_files(&[("Fonts/Invalid.ttf", b"invalid font")])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut platform = crate::platform::SdlPlatform::start(crate::WindowConfiguration::new(
        96,
        96,
        crate::WindowMode::Windowed,
    ))?;
    while platform.poll_event().is_some() {}
    let input = platform.input_handle();
    let cpu = CpuExecutor::with_notifier(config()?, platform.coordinator_notifier())?;
    let sdl = sdl3::init()?;
    let events = sdl.event()?;
    events.push_event(sdl3::event::Event::Quit { timestamp: 0 })?;
    let task = cpu.try_submit(|| {
        std::thread::sleep(std::time::Duration::from_millis(20));
        7
    })?;
    input.wait_until_ready(|| Ok::<_, PlatformError>(task.is_finished()))?;
    assert_eq!(task.join()?, 7);
    let mut quits = 0;
    while let Some(event) = platform.poll_event() {
        if matches!(event.event, crate::PlatformEvent::QuitRequested) {
            quits += 1;
        }
    }
    assert_eq!(quits, 1);
    let mut fonts = font_system(&cpu, input, catalog.clone());
    drop(platform);
    let mut store = AssetStore::mount(catalog)?;
    assert!(matches!(
        fonts.rasterize(
            &mut store,
            &AssetPath::new("Fonts/Invalid.ttf")?,
            16,
            'A',
            FontRasterization::Antialiased
        ),
        Err(FontError::Execution { .. })
    ));
    assert_eq!(cpu.snapshot()?.in_flight(), 0);
    assert_eq!(cpu.try_submit(|| 42)?.join()?, 42);
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with build-12340 fonts"]
fn stock_worker_fonts_match_serial_and_reuse_cache_under_saturation() -> Result<(), Box<dyn Error>>
{
    let root = ClientDataRoot::new(
        std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("SOLARITY_STOCK_DATA_ROOT")?,
    )?;
    let catalog = ArchiveCatalog::discover(root.clone(), Locale::EnUs)?;
    let mut cpu = CpuExecutor::new(config()?)?;
    let baseline = cpu
        .storage()
        .snapshot()
        .bytes(CpuStorageClass::Required, CpuStorageKind::Result);
    let mut worker = offline(&cpu, catalog.clone());
    let mut serial = FontSystem::new()?;
    let mut store = AssetStore::mount(catalog)?;
    let path = AssetPath::new("Fonts/FRIZQT__.TTF")?;
    for height in [16, 23] {
        for mode in [
            FontRasterization::Antialiased,
            FontRasterization::Monochrome,
        ] {
            let requests = "AV Soap 0123456789"
                .chars()
                .map(|character| FontGlyphRequest::new(path.clone(), height, character, mode))
                .collect::<Vec<_>>();
            let expected = serial
                .rasterize_batch(&mut store, requests.clone())?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            let actual = worker
                .rasterize_batch(&mut store, requests)?
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(actual, expected);
            assert_eq!(
                worker.measure_line_width_26_6(&mut store, &path, height, "AV Soap", mode)?,
                serial.measure_line_width_26_6(&mut store, &path, height, "AV Soap", mode)?
            );
        }
        assert_eq!(
            worker.ascender_26_6(&mut store, &path, height)?,
            serial.ascender_26_6(&mut store, &path, height)?
        );
        assert_eq!(
            worker.kerning_x_26_6(&mut store, &path, height, 'A', 'V')?,
            serial.kerning_x_26_6(&mut store, &path, height, 'A', 'V')?
        );
    }
    assert_eq!(worker.loaded_face_count(), 1);
    assert_eq!(worker.cached_glyph_count(), serial.cached_glyph_count());
    assert_eq!(
        worker.cached_coverage_bytes(),
        serial.cached_coverage_bytes()
    );
    assert!(
        cpu.storage()
            .snapshot()
            .bytes(CpuStorageClass::Required, CpuStorageKind::Result)
            > baseline
    );
    let mut shared = worker.clone();
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let mode = FontRasterization::Antialiased;
    assert_eq!(
        shared.rasterize(&mut store, &path, 16, 'A', mode)?,
        serial.rasterize(&mut store, &path, 16, 'A', mode)?
    );
    assert_eq!(
        shared.measure_line_width_26_6(&mut store, &path, 16, "AV Soap", mode)?,
        serial.measure_line_width_26_6(&mut store, &path, 16, "AV Soap", mode)?
    );
    assert_eq!(
        shared.ascender_26_6(&mut store, &path, 16)?,
        serial.ascender_26_6(&mut store, &path, 16)?
    );
    assert_eq!(
        shared.kerning_x_26_6(&mut store, &path, 16, 'A', 'V')?,
        serial.kerning_x_26_6(&mut store, &path, 16, 'A', 'V')?
    );
    assert!(matches!(
        worker.rasterize(&mut store, &path, 16, 'Z', mode),
        Err(FontError::Asset(AssetError::SourceStorage(
            CpuError::AtCapacity { .. }
        )))
    ));
    release.send(())?;
    blocker.join()??;
    assert_eq!(
        worker.rasterize(&mut store, &path, 16, 'Z', mode)?,
        serial.rasterize(&mut store, &path, 16, 'Z', mode)?
    );
    let mut other_namespace = AssetStore::mount(ArchiveCatalog::discover(root, Locale::EnUs)?)?;
    assert!(matches!(
        worker.rasterize(&mut other_namespace, &path, 16, 'A', mode),
        Err(FontError::Execution { .. })
    ));
    cpu.shutdown()?;
    assert_eq!(
        shared.rasterize(&mut store, &path, 16, 'A', mode)?,
        serial.rasterize(&mut store, &path, 16, 'A', mode)?
    );
    drop(shared);
    drop(worker);
    assert_eq!(
        cpu.storage()
            .snapshot()
            .bytes(CpuStorageClass::Required, CpuStorageKind::Result),
        baseline
    );
    Ok(())
}

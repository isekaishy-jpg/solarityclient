//! Measures the CPU cost of one stock login-screen GlueXML idle update.

use std::error::Error;
use std::hint::black_box;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{AddonCatalog, GlueInitialScreen, GlueManager};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    let frame_count = arguments
        .next()
        .map(|value| value.to_string_lossy().parse::<usize>())
        .transpose()?
        .unwrap_or(10_000);
    if arguments.next().is_some() || frame_count == 0 {
        return Err(usage_error().into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut inspection = AssetStore::mount(catalog.clone())?;
    let addons = AddonCatalog::discover(&mut inspection)?;
    let mut manager = GlueManager::start_shared_with_profile(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1920, 1080),
        false,
        GlueInitialScreen::Login,
        &[
            ("readEULA".to_owned(), "1".to_owned()),
            ("readTOS".to_owned(), "1".to_owned()),
            ("readTerminationWithoutNotice".to_owned(), "1".to_owned()),
            ("readScanning".to_owned(), "1".to_owned()),
            ("readContest".to_owned(), "1".to_owned()),
        ],
        &addons,
    )?;
    for _ in 0..120 {
        black_box(manager.update(1.0 / 1_200.0)?);
    }

    let snapshots = manager.runtime_snapshot_count();
    let mut samples = Vec::with_capacity(frame_count);
    let mut changed = 0_usize;
    for _ in 0..frame_count {
        let started = Instant::now();
        changed += usize::from(black_box(manager.update(1.0 / 1_200.0)?));
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    let total = samples.iter().sum::<Duration>();
    let mean = total.as_secs_f64() * 1_000_000.0 / frame_count as f64;
    let percentile = |numerator: usize| {
        samples[(frame_count.saturating_sub(1) * numerator) / 100].as_secs_f64() * 1_000_000.0
    };
    println!(
        "frames={frame_count} changed={changed} objects={} batches={} snapshots={} mean_us={mean:.3} p50_us={:.3} p95_us={:.3} p99_us={:.3} max_us={:.3}",
        manager.objects().len(),
        manager.render_plan().mesh().batches().len(),
        manager.runtime_snapshot_count() - snapshots,
        percentile(50),
        percentile(95),
        percentile(99),
        samples.last().copied().unwrap_or_default().as_secs_f64() * 1_000_000.0,
    );
    Ok(())
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: benchmark_ui_idle <Data directory> <locale> [frame count]",
    )
}

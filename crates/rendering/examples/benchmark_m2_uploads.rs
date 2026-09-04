//! Measures cold and retained M2 mesh admission against installed stock data.

#![allow(unsafe_code)]

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::time::Instant;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, DecodedM2Model, Locale,
};
use solarity_rendering::{M2MeshPlan, VulkanBootstrap, VulkanPresentMode};

/// Decodes outside the measured region, then interleaves admissions with presents.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    let paths = arguments
        .map(|value| value.into_string().map_err(|_| usage_error()))
        .collect::<Result<Vec<_>, _>>()?;
    if paths.is_empty() {
        return Err(usage_error().into());
    }
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let mut plans = Vec::with_capacity(paths.len());
    for path in paths {
        let model = DecodedM2Model::load(&mut store, &AssetPath::new(path)?)?;
        plans.push(M2MeshPlan::prepare(&model, 0)?);
    }

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video
        .window("Solarity M2 admission benchmark", 1280, 720)
        .vulkan()
        .hidden()
        .build()?;
    let bootstrap = VulkanBootstrap::start(&window.vulkan_instance_extensions()?)?;
    // SAFETY: The live bootstrap enabled the exact extensions required by SDL.
    let surface = unsafe { window.vulkan_create_surface(bootstrap.instance_handle()) }?;
    // SAFETY: Sole surface ownership transfers to its creating instance's renderer.
    let mut renderer = unsafe {
        bootstrap.attach_surface_with_present_mode(
            surface,
            (1280, 720),
            0,
            VulkanPresentMode::Uncapped,
        )
    }?;
    for _ in 0..8 {
        renderer.present_clear([1280.0, 720.0])?;
    }
    println!("device={}", renderer.report().device_name());
    println!("model,vertices,cold_upload_ms,cached_upload_ms,next_present_ms");
    for plan in &plans {
        let started = Instant::now();
        let handle = renderer.upload_m2_mesh(plan)?;
        let uploaded = started.elapsed();
        let cached_started = Instant::now();
        if renderer.upload_m2_mesh(plan)? != handle {
            return Err(IoError::other("retained model upload changed its handle").into());
        }
        let cached = cached_started.elapsed();
        let present_started = Instant::now();
        renderer.present_clear([1280.0, 720.0])?;
        let presented = present_started.elapsed();
        println!(
            "{},{},{:.4},{:.4},{:.4}",
            plan.path(),
            plan.vertices().len(),
            uploaded.as_secs_f64() * 1_000.0,
            cached.as_secs_f64() * 1_000.0,
            presented.as_secs_f64() * 1_000.0,
        );
    }
    Ok(())
}

/// Reports the explicit installed-data and model inputs needed by this benchmark.
fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: benchmark_m2_uploads <Data> <locale> <model.m2> ...",
    )
}

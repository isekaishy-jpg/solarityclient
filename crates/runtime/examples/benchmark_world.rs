//! Measures installed World presentation with an explicitly offline player fixture.

use std::error::Error;
use std::ffi::OsString;
use std::fs::File;
use std::io::{BufWriter, Error as IoError, ErrorKind, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, PlayerAppearance, PlayerEquipment, PlayerMoney,
    PlayerProgression, PlayerViewState, UnitAnimationTier, UnitFlags, UnitIdentity,
    UnitPresentation, UnitSheathState, UnitStats, UnitVitals, WorldBootstrap, WorldMapId,
};
use solarity_network::WorldTimeSpeed;
use solarity_runtime::{ClientApplication, RealmClock, RuntimeConfiguration};

/// Creates a controlled world fixture and writes every measured frame, including residency churn.
fn main() -> Result<(), Box<dyn Error>> {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env()?;
    let _subscriber = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    let mut args = std::env::args_os().skip(1);
    let usage = || {
        IoError::new(
            ErrorKind::InvalidInput,
            format!(
                "usage: benchmark_world <frames per phase> <output.csv> <map> <x> <y> <z> \
                 [--travel-offset <dx> <dy> <dz>] [--camera-distance <yards>] \
                 [--camera-pitch <radians>] [--camera-yaw <radians>] [--realm-hour <0..23>] \
                 [--screen-effect <ScreenEffect.dbc ID>] {}",
                RuntimeConfiguration::usage()
            ),
        )
    };
    let frames: NonZeroUsize = args
        .next()
        .and_then(|v| v.into_string().ok())
        .ok_or_else(usage)?
        .parse()?;
    let output = PathBuf::from(args.next().ok_or_else(usage)?);
    let map = args
        .next()
        .and_then(|v| v.into_string().ok())
        .ok_or_else(usage)?
        .parse()?;
    let mut position = [0_f32; 3];
    for coordinate in &mut position {
        *coordinate = args
            .next()
            .and_then(|v| v.into_string().ok())
            .ok_or_else(usage)?
            .parse()?;
    }
    let mut travel_offset = None;
    let mut distance = PlayerViewState::STOCK_VIEW_2.distance();
    let mut pitch = PlayerViewState::STOCK_VIEW_2.pitch_radians();
    let mut yaw = 0.0;
    let mut hour = 12_u32;
    let mut screen_effect = None;
    let mut runtime_args = Vec::new();
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--travel-offset") => {
                travel_offset = Some(Vec3::new(
                    finite_argument(&mut args)?,
                    finite_argument(&mut args)?,
                    finite_argument(&mut args)?,
                ));
            }
            Some("--camera-distance") => distance = finite_argument(&mut args)?,
            Some("--camera-pitch") => pitch = finite_argument(&mut args)?,
            Some("--camera-yaw") => yaw = finite_argument(&mut args)?,
            Some("--realm-hour") => {
                hour = args
                    .next()
                    .and_then(|v| v.into_string().ok())
                    .ok_or_else(usage)?
                    .parse()?;
            }
            Some("--screen-effect") => {
                screen_effect = Some(
                    args.next()
                        .and_then(|value| value.into_string().ok())
                        .ok_or_else(usage)?
                        .parse()?,
                );
            }
            _ => runtime_args.push(argument),
        }
    }
    if distance <= 0. || pitch.abs() >= std::f32::consts::FRAC_PI_2 || hour > 23 {
        return Err(usage().into());
    }
    let configuration = RuntimeConfiguration::from_arguments(runtime_args)?;
    // A level-one human warrior with empty equipment and an explicit realm hour.
    // No server identity, authentication, movement input, or remote population is supplied.
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(map),
        1,
        "WorldBenchmark",
        Vec3::from_array(position),
        0.,
    ));
    world.set_local_player_view(PlayerViewState::new(distance, pitch, yaw, 2))?;
    let player = world.local_player();
    world.storage_mut().add_component(
        player,
        (
            ObjectKind::Player,
            ObjectPresentation::new(0, 1.),
            UnitIdentity::new(1, 1, 0, 1, 1, 1),
            UnitPresentation::new(
                49,
                49,
                0,
                0,
                UnitAnimationTier::Ground,
                UnitSheathState::Unarmed,
            ),
            UnitFlags::default(),
            PlayerAppearance::default(),
            PlayerEquipment::default(),
        ),
    );
    world.storage_mut().add_component(
        player,
        (
            PlayerMoney::new(0),
            PlayerProgression::new(0, 400),
            UnitVitals::new(100, 100, [0; 7], [100; 7]),
            UnitStats::new([20; 5], [0; 5], [0; 5]),
        ),
    );
    let clock = RealmClock::new(WorldTimeSpeed::new(hour << 6, 0., 0)?);
    let mut application = ClientApplication::start(configuration)?;
    println!(
        "adapter={} extent={:?}; offline fixture, real installed terrain/FrameXML/Vulkan; no network, movement solver, remote units, audio or overlays",
        application.vulkan_report().device_name(),
        application.vulkan_report().extent()
    );
    let capture_directory = std::env::var_os("SOLARITY_WORLD_CAPTURE_DIR").map(PathBuf::from);
    if capture_directory.is_some() {
        println!(
            "framebuffer capture enabled; use a separate uncaptured run for performance measurements"
        );
    }
    let result = application.benchmark_world(
        &mut world,
        &clock,
        frames,
        capture_directory.as_deref(),
        travel_offset,
        screen_effect,
    );
    let shutdown = application.shutdown();
    let samples = result?;
    shutdown?;
    let mut writer = BufWriter::new(File::create(output)?);
    writeln!(
        writer,
        "phase,frame,resident_tiles,total_ms,service_ms,streaming_ms,ui_ms,camera_ms,present_ms,admitted_tiles,evicted_tiles,x,y,z,ground_detail_draws,primary_shadow_draws"
    )?;
    for sample in &samples {
        writeln!(
            writer,
            "{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{},{:.6},{:.6},{:.6},{},{}",
            sample.phase,
            sample.frame,
            sample.resident_tiles,
            ms(sample.total),
            ms(sample.service),
            ms(sample.streaming),
            ms(sample.ui),
            ms(sample.camera),
            ms(sample.present),
            sample.admitted_tiles,
            sample.evicted_tiles,
            sample.position.x,
            sample.position.y,
            sample.position.z,
            sample.ground_detail_draws,
            sample.primary_shadow_draws,
        )?;
    }
    writer.flush()?;
    for phase in [
        "streaming",
        "stationary",
        "orbit",
        "pointer",
        "travel_out",
        "travel_back",
        "settled",
    ] {
        let mut intervals = samples
            .iter()
            .filter(|s| s.phase == phase)
            .map(|s| s.total)
            .collect::<Vec<_>>();
        intervals.sort_unstable();
        if intervals.is_empty() {
            continue;
        }
        let mean = intervals.iter().map(|&d| ms(d)).sum::<f64>() / intervals.len() as f64;
        println!(
            "phase={phase} frames={} mean_ms={mean:.6} p50_ms={:.6} p95_ms={:.6} max_ms={:.6}",
            intervals.len(),
            ms(intervals[intervals.len() / 2]),
            ms(intervals[(intervals.len() - 1) * 95 / 100]),
            ms(*intervals.last().ok_or_else(usage)?)
        );
    }
    Ok(())
}

/// Parses finite fixture coordinates and view settings before starting the renderer.
fn finite_argument(args: &mut impl Iterator<Item = OsString>) -> Result<f32, Box<dyn Error>> {
    let value: f32 = args
        .next()
        .and_then(|v| v.into_string().ok())
        .ok_or_else(|| IoError::new(ErrorKind::InvalidInput, "missing numeric fixture argument"))?
        .parse()?;
    if !value.is_finite() {
        return Err(
            IoError::new(ErrorKind::InvalidInput, "fixture argument must be finite").into(),
        );
    }
    Ok(value)
}

/// Converts the retained high-resolution interval for human-readable CSV output.
fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.
}

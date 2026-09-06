//! Measures installed World presentation with an explicitly offline player fixture.

use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Error as IoError, ErrorKind, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use glam::Vec3;
use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, PlayerAppearance, PlayerEquipment, PlayerMoney,
    PlayerProgression, UnitAnimationTier, UnitFlags, UnitIdentity, UnitPresentation,
    UnitSheathState, UnitStats, UnitVitals, WorldBootstrap, WorldMapId,
};
use solarity_network::WorldTimeSpeed;
use solarity_runtime::{ClientApplication, RealmClock, RuntimeConfiguration};

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
                "usage: benchmark_world <frames per phase> <output.csv> <map> <x> <y> <z> {}",
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
    let configuration = RuntimeConfiguration::from_arguments(args)?;
    // A level-one human warrior with empty equipment and explicit noon realm time.
    // No server identity, authentication, movement input, or remote population is supplied.
    let mut world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(map),
        1,
        "WorldBenchmark",
        Vec3::from_array(position),
        0.,
    ));
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
    let clock = RealmClock::new(WorldTimeSpeed::new(12 << 6, 0., 0)?);
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
    let result = application.benchmark_world(&world, &clock, frames, capture_directory.as_deref());
    let shutdown = application.shutdown();
    let samples = result?;
    shutdown?;
    let mut writer = BufWriter::new(File::create(output)?);
    writeln!(
        writer,
        "phase,frame,resident_tiles,total_ms,service_ms,streaming_ms,ui_ms,camera_ms,present_ms"
    )?;
    for sample in &samples {
        writeln!(
            writer,
            "{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
            sample.phase,
            sample.frame,
            sample.resident_tiles,
            ms(sample.total),
            ms(sample.service),
            ms(sample.streaming),
            ms(sample.ui),
            ms(sample.camera),
            ms(sample.present)
        )?;
    }
    writer.flush()?;
    for phase in ["streaming", "stationary", "orbit", "pointer"] {
        let mut intervals = samples
            .iter()
            .filter(|s| s.phase == phase)
            .map(|s| s.total)
            .collect::<Vec<_>>();
        intervals.sort_unstable();
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

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.
}

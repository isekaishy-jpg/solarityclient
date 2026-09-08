//! Measures resident-world camera collision queries without a window or server.

use std::error::Error;
use std::hint::black_box;
use std::io::{Error as IoError, ErrorKind};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use glam::Vec3;
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, MapCatalog};
use solarity_ecs::{ActiveWorld, PlayerViewState, WorldBootstrap, WorldMapId, WorldTransform};
use solarity_runtime::RuntimeTerrainCoordinator;
use solarity_systems::{
    CameraSubjectGeometry, resolve_camera_subject_height, resolve_player_camera_pose,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let usage = || {
        IoError::new(
            ErrorKind::InvalidInput,
            "usage: benchmark_world_camera <Data directory> <locale> <map> <x> <y> <z> <sample count>",
        )
    };
    let root = ClientDataRoot::new(arguments.next().ok_or_else(usage)?)?;
    let locale = arguments.next().ok_or_else(usage)?.parse()?;
    let map = arguments.next().ok_or_else(usage)?.parse()?;
    let position = Vec3::new(
        arguments.next().ok_or_else(usage)?.parse()?,
        arguments.next().ok_or_else(usage)?.parse()?,
        arguments.next().ok_or_else(usage)?.parse()?,
    );
    let samples: NonZeroUsize = arguments.next().ok_or_else(usage)?.parse()?;
    if arguments.next().is_some() || !position.is_finite() {
        return Err(usage().into());
    }
    let started = Instant::now();
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, locale)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let mut terrain = RuntimeTerrainCoordinator::new(AssetStoreHandle::new(store), maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(map),
        1,
        "Camera benchmark",
        position,
        0.,
    ));
    let residency = terrain.synchronize(Some(&world))?;
    println!(
        "map={map} position={position:?} residency={residency:?} load_ms={:.3} m2_placements={} m2_collision={} wmo_placements={}",
        started.elapsed().as_secs_f64() * 1000.,
        terrain.resident_m2_count(),
        terrain.resident_m2_collision_count(),
        terrain.resident_world_model_count()
    );
    println!(
        "Single resident tile, static authored geometry, 16:9 aspect, synthetic 1.75 subject marker; no rendering, FrameXML, remote units or adjacent-tile streaming."
    );
    let height = resolve_camera_subject_height(CameraSubjectGeometry::new(Some(1.75), 2., 1.))?;
    for (name, distance, orbit) in [
        ("stationary", 5.55, false),
        ("orbit", 5.55, true),
        ("long-orbit", 15., true),
    ] {
        let mut durations = Vec::with_capacity(samples.get());
        let mut obstructed = 0;
        for index in 0..samples.get() + 32 {
            let yaw = if orbit {
                (index % 360) as f32 * std::f32::consts::TAU / 360.
            } else {
                0.
            };
            let pose = resolve_player_camera_pose(
                WorldTransform::new(position, 0.),
                PlayerViewState::new(distance, 0.174_532_92, yaw, 2),
                height,
            )?;
            let start = Instant::now();
            let resolved = match terrain.resolve_player_camera(
                black_box(pose),
                16. / 9.,
                solarity_systems::PlayerCameraObstructionSettings::default(),
            ) {
                Ok(resolved) => resolved,
                Err(error) => {
                    eprintln!("scenario={name} sample={index} yaw={yaw:?} pose={pose:?}");
                    return Err(error.into());
                }
            };
            let elapsed = start.elapsed();
            black_box(resolved);
            if index >= 32 {
                durations.push(elapsed);
                obstructed +=
                    usize::from((resolved.eye() - pose.eye()).length_squared() > 0.000_001);
            }
        }
        let total: Duration = durations.iter().sum();
        durations.sort_unstable();
        let percentile =
            |percent: usize| durations[(durations.len() - 1) * percent / 100].as_secs_f64() * 1000.;
        println!(
            "scenario={name} samples={} changed_eye={obstructed} mean_ms={:.6} p50_ms={:.6} p95_ms={:.6} p99_ms={:.6} max_ms={:.6}",
            samples.get(),
            total.as_secs_f64() * 1000. / samples.get() as f64,
            percentile(50),
            percentile(95),
            percentile(99),
            percentile(100)
        );
    }
    Ok(())
}

//! Validates one installed-client world position through runtime terrain residency.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale, MapCatalog,
};
use solarity_ecs::{ActiveWorld, WorldBootstrap, WorldMapId};
use solarity_runtime::{RuntimeTerrainCoordinator, RuntimeTerrainPoll};

fn main() -> Result<(), Box<dyn Error>> {
    let _subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .try_init();
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
    let map_id = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<u32>()?;
    let mut position = [0.0_f32; 3];
    for coordinate in &mut position {
        *coordinate = arguments
            .next()
            .and_then(|v| v.into_string().ok())
            .ok_or_else(usage_error)?
            .parse()?;
    }
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let maps = MapCatalog::load(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let mut terrain = RuntimeTerrainCoordinator::new(assets, maps);
    let world = ActiveWorld::enter(WorldBootstrap::new(
        WorldMapId::new(map_id),
        1,
        "WorldSceneValidation",
        Vec3::from_array(position),
        0.0,
    ));
    let poll = terrain.synchronize(Some(&world))?;
    if !matches!(
        poll,
        RuntimeTerrainPoll::TileLoaded { .. } | RuntimeTerrainPoll::GlobalWorldModelLoaded { .. }
    ) {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("map {map_id} did not publish residency: {poll:?}"),
        )
        .into());
    }
    println!(
        "validated map {map_id} at {position:?}: area={:?} WMO={}/{} M2={}/{} M2_collision={}",
        terrain.current_area_id(&world)?,
        terrain.resident_world_model_count(),
        terrain.resident_world_model_source_count(),
        terrain.resident_m2_count(),
        terrain.resident_m2_source_count(),
        terrain.resident_m2_collision_count(),
    );
    Ok(())
}

/// Returns the exact required command-line shape.
fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_world_scene <Data> <locale> <map-id> <x> <y> <z>",
    )
}

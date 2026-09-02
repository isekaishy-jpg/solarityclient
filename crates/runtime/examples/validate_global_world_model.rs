//! Validates one installed-client global-WMO map through runtime residency.

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
        "GlobalWorldModelValidation",
        Vec3::ZERO,
        0.0,
    ));
    let poll = terrain.synchronize(Some(&world))?;
    if poll != (RuntimeTerrainPoll::GlobalWorldModelLoaded { map_id }) {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!("map {map_id} did not enter global-WMO residency: {poll:?}"),
        )
        .into());
    }
    println!(
        "validated global-WMO map {map_id}: area={:?} WMO={}/{} M2={}/{} M2_collision={}",
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
        "usage: validate_global_world_model <Data> <locale> <map-id>",
    )
}

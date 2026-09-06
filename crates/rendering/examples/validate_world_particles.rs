//! Replays particles from an installed ADT neighborhood without a window or server.

use std::collections::HashSet;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

use glam::Mat4;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, DecodedM2Model, DecodedWorldModel, M2ModelCache,
    MapCatalog, TerrainMap, TerrainTileIndex,
};
use solarity_rendering::{M2AnimationClock, M2ParticlePose, M2ParticleSimulation};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let usage = || {
        IoError::new(
            ErrorKind::InvalidInput,
            "usage: validate_world_particles <Data directory> <locale> <map> <tile x> <tile y>",
        )
    };
    let root = ClientDataRoot::new(args.next().ok_or_else(usage)?)?;
    let locale = args.next().ok_or_else(usage)?.parse()?;
    let map = args.next().ok_or_else(usage)?.parse()?;
    let x: u8 = args.next().ok_or_else(usage)?.parse()?;
    let y: u8 = args.next().ok_or_else(usage)?.parse()?;
    if args.next().is_some() || x >= 64 || y >= 64 {
        return Err(usage().into());
    }
    let mut store = AssetStore::mount(ArchiveCatalog::discover(root, locale)?)?;
    let maps = MapCatalog::load(&mut store)?;
    let terrain = TerrainMap::load(&mut store, maps.map(map).ok_or_else(usage)?)?;
    let mut paths = HashSet::new();
    let mut world_models = HashSet::new();
    for ty in y.saturating_sub(1)..=y.saturating_add(1).min(63) {
        for tx in x.saturating_sub(1)..=x.saturating_add(1).min(63) {
            let index = TerrainTileIndex::new(tx, ty).ok_or_else(usage)?;
            if !terrain.tile(index).exists() {
                continue;
            }
            let tile = terrain.load_tile(&mut store, index)?;
            for doodad in tile.doodads() {
                paths.insert(M2ModelCache::canonical_path(doodad.path())?);
            }
            for placement in tile.world_models() {
                if !world_models.insert((placement.path().clone(), placement.doodad_set())) {
                    continue;
                }
                let wmo = DecodedWorldModel::load(&mut store, placement.path())?;
                for index in wmo.active_doodad_indices(placement.doodad_set())? {
                    paths.insert(M2ModelCache::canonical_path(wmo.doodads()[index].path())?);
                }
            }
        }
    }
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut emitters = 0;
    let mut failures = 0;
    for path in &paths {
        let model = DecodedM2Model::load(&mut store, path)?;
        for (index, emitter) in model.animations().particles().iter().enumerate() {
            emitters += 1;
            if !matches!(emitter.emitter_type(), 1 | 2)
                || M2ParticleSimulation::unsupported_behavior_flags(emitter) != 0
            {
                println!(
                    "unsupported model={path} emitter={index} type={} flags={:#x}",
                    emitter.emitter_type(),
                    emitter.flags()
                );
                continue;
            }
            let mut simulation = M2ParticleSimulation::new(index as u32);
            for tick in 1..=3_600 {
                let time_ms = tick as f32 * (1_000.0 / 60.0);
                let clock = M2AnimationClock::new(0, time_ms, time_ms);
                let pose = M2ParticlePose::sample(model.animations(), emitter, clock)?;
                let result = match emitter.emitter_type() {
                    1 => simulation.advance_planar_bounded(
                        emitter,
                        pose,
                        1.0 / 60.0,
                        Mat4::IDENTITY,
                        1.0,
                    ),
                    _ => simulation.advance_sphere_bounded(
                        emitter,
                        pose,
                        1.0 / 60.0,
                        Mat4::IDENTITY,
                        1.0,
                    ),
                };
                if let Err(error) = result {
                    failures += 1;
                    eprintln!(
                        "FAIL model={path} emitter={index} tick={tick} time_ms={time_ms} rate={} rate_variation={} lifespan={} lifespan_variation={} capacity={} error={error}",
                        pose.emission_rate(),
                        emitter.emission_rate_variation(),
                        pose.lifespan(),
                        emitter.lifespan_variation(),
                        simulation.capacity()
                    );
                    break;
                }
            }
        }
    }
    println!(
        "models={} emitters={emitters} failures={failures}; sequence 0, sixty seconds at 60 Hz, identity emitter transforms, fixed seeds, full density",
        paths.len()
    );
    if failures != 0 {
        return Err(IoError::other("world particle replay failed").into());
    }
    Ok(())
}

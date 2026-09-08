//! Validate installed LightSkybox declarations and their authored model paths.

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, ClientDataRoot, LightCatalog, Locale, M2ModelCache,
    WdbcTable,
};
use solarity_rendering::M2MeshPlan;
use std::collections::{BTreeMap, BTreeSet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("expected Data directory")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let table = WdbcTable::load(
        &mut store,
        &AssetPath::new("DBFilesClient/LightSkybox.dbc")?,
    )?;
    let lights = LightCatalog::load(&mut store)?;
    let mut models = M2ModelCache::new();
    let mut paths = BTreeMap::<_, Vec<_>>::new();
    let mut failures = 0;
    for row in 0..table.header().record_count() {
        let record = table.record(row).ok_or("truncated skybox row")?;
        let id = u32::from_le_bytes(record[..4].try_into()?);
        let sky = lights.skybox(id).ok_or("missing skybox row")?;
        println!(
            "skybox={id} flags={:#x} path={:?}",
            sky.flags(),
            sky.model_path()
        );
        if sky.model_path().is_empty() {
            continue;
        }
        let path = AssetPath::new(sky.model_path())?;
        paths
            .entry(path.clone())
            .or_default()
            .push((id, sky.flags()));
        let model = match models.load(&mut store, &path) {
            Ok(model) => model,
            Err(error) => {
                println!("  LOAD FAILED: {error}");
                failures += 1;
                continue;
            }
        };
        let plan = M2MeshPlan::prepare(&model, 0)?;
        let materials = model
            .materials()
            .iter()
            .map(|material| (material.flags(), format!("{:?}", material.blend_mode())))
            .collect::<BTreeSet<_>>();
        println!(
            "  vertices={} draws={} bones={} particles={} ribbons={} lights={} materials={materials:?}",
            plan.vertices().len(),
            plan.draws().len(),
            model.animations().bones().len(),
            model.animations().particles().len(),
            model.animations().ribbons().len(),
            model.animations().lights().len()
        );
        for sequence in model.animations().sequences() {
            println!(
                "  animation={} duration={}",
                sequence.animation_id(),
                sequence.duration_ms()
            );
        }
        for texture in model.textures() {
            println!("  texture={:?} {:?}", texture.kind(), texture.filename());
        }
    }
    for (path, rows) in paths.iter().filter(|(_, rows)| rows.len() > 1) {
        println!("duplicate path {path}: {rows:?}");
    }
    println!(
        "Checked {} rows, {} unique paths, {failures} failures",
        table.header().record_count(),
        paths.len()
    );
    Ok(())
}

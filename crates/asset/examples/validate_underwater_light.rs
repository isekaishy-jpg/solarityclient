//! Validates every authored underwater preset against the installed client tables.

use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::str::FromStr;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, LightCatalog, LiquidTypeCatalog, Locale,
    WorldLightCondition, WorldLightQuery,
};

/// Exercises underwater volume banks and direct LiquidType.LightID overrides
/// across a complete day, including the parameter-213 crash regression.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let root = arguments.next().ok_or("expected client Data directory")?;
    let locale = arguments.next().ok_or("expected locale")?;
    if arguments.next().is_some() {
        return Err("usage: validate_underwater_light <Data directory> <locale>".into());
    }
    let locale = Locale::from_str(locale.to_str().ok_or("locale is not UTF-8")?)?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(PathBuf::from(root))?,
        locale,
    )?)?;
    let lights = LightCatalog::load(&mut store)?;
    let liquids = LiquidTypeCatalog::load(&mut store)?;
    let underwater = WorldLightCondition::UNDERWATER;
    let mut parameters = BTreeSet::from([213]);
    for light in lights.lights() {
        let id = light.parameter_ids()[usize::from(underwater.value())];
        if id != 0 {
            parameters.insert(id);
        }
    }
    for liquid in liquids.entries() {
        if liquid.light_id() != 0 {
            parameters.insert(liquid.light_id());
        }
    }
    let mut samples = 0;
    for id in &parameters {
        for time in (0..2880).step_by(30) {
            let base = lights.sample_parameter(*id, time)?;
            for liquid in liquids
                .entries()
                .iter()
                .filter(|liquid| liquid.light_id() == *id)
            {
                for depth in [-0.01, 0., 0.25, 1., 5., 20., 100.] {
                    let sample = base.with_liquid_depth(liquid, depth);
                    if !sample.ambient_color().is_finite()
                        || !sample.diffuse_color().is_finite()
                        || !sample.fog_color().is_finite()
                    {
                        return Err(format!("nonfinite liquid light for parameter {id}").into());
                    }
                    samples += 1;
                }
            }
            samples += 1;
        }
    }
    // This also exercises the map-to-fallback selection used by an interior.
    for map in [0, 1, 389, 530, 571] {
        lights.sample(WorldLightQuery::new(map, Vec3::ZERO, 720).with_condition(underwater))?;
        samples += 1;
    }
    let regression = lights.sample_parameter(213, 720)?;
    if regression.specular_color() != Vec3::ZERO {
        return Err("parameter 213 specular band 3826 did not resolve to stock black".into());
    }
    println!(
        "validated {} underwater parameters and {samples} samples; band 3826 resolves to black",
        parameters.len()
    );
    Ok(())
}

//! Local validation of build-12340 light tables and one exterior sample.

use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use glam::Vec3;
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, LightCatalog, Locale, WorldLightQuery,
    exterior_light_direction,
};

/// Mounts real client data and samples one map position and time of day.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "validate_world_light".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = text_argument(arguments.next(), "locale", &executable)?;
    let map_id = number_argument::<u32>(arguments.next(), "map ID", &executable)?;
    let world_x = number_argument::<f32>(arguments.next(), "world X", &executable)?;
    let world_y = number_argument::<f32>(arguments.next(), "world Y", &executable)?;
    let world_z = number_argument::<f32>(arguments.next(), "world Z", &executable)?;
    let half_minutes = number_argument::<u32>(arguments.next(), "half-minutes", &executable)?;
    if arguments.next().is_some() {
        return Err(usage_error(&executable).into());
    }

    let root = ClientDataRoot::new(PathBuf::from(data_root))?;
    let catalog = ArchiveCatalog::discover(root, Locale::from_str(&locale)?)?;
    let mut store = AssetStore::mount(catalog)?;
    let lights = LightCatalog::load(&mut store)?;
    let sample = lights.sample(WorldLightQuery::new(
        map_id,
        Vec3::new(world_x, world_y, world_z),
        half_minutes,
    ))?;
    println!(
        "lights={} map={} position=[{world_x}, {world_y}, {world_z}] half_minutes={half_minutes}",
        lights.lights().len(),
        map_id,
    );
    println!(
        "ambient={:?} diffuse={:?} specular={:?} direction={:?}",
        sample.ambient_color(),
        sample.diffuse_color(),
        sample.specular_color(),
        exterior_light_direction(half_minutes),
    );
    println!(
        "fog_range={:?} fog_color={:?} skyboxes={:?}",
        sample.fog_range(),
        sample.fog_color(),
        sample.skyboxes(),
    );
    Ok(())
}

/// Reads one required UTF-8 argument.
fn text_argument(
    value: Option<std::ffi::OsString>,
    name: &str,
    executable: &std::ffi::OsStr,
) -> Result<String, io::Error> {
    let value = value.ok_or_else(|| usage_error(executable))?;
    value
        .into_string()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("{name} is not UTF-8")))
}

/// Parses one required numeric argument without accepting omitted state.
fn number_argument<T>(
    value: Option<std::ffi::OsString>,
    name: &str,
    executable: &std::ffi::OsStr,
) -> Result<T, io::Error>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let value = value.ok_or_else(|| usage_error(executable))?;
    let value = value.to_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("{name} is not UTF-8"))
    })?;
    value
        .parse::<T>()
        .map_err(|source| io::Error::new(io::ErrorKind::InvalidInput, source))
}

/// Returns the exact command contract for incomplete input.
fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale> <map ID> <world X> <world Y> <world Z> <half-minutes>",
            PathBuf::from(executable).display()
        ),
    )
}

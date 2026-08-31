//! Local validation command for a real client installation.
//!
//! This example reads only caller-selected files and never copies client data
//! into the repository.

use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::str::FromStr;

use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetPath, AssetStore, BlsShaderStage, ClientDataRoot,
    DecodedBlpTexture, DecodedBlsShader, Locale, RealmCategoryCatalog, RealmConfigurationCatalog,
    WdbcTable,
};

/// Mounts a real client archive set and reads every requested internal path.
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let executable = arguments
        .next()
        .unwrap_or_else(|| "validate_client_data".into());
    let data_root = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let locale = arguments.next().ok_or_else(|| usage_error(&executable))?;
    let requested_paths = arguments.collect::<Vec<_>>();
    if requested_paths.is_empty() {
        return Err(usage_error(&executable).into());
    }

    let locale = locale
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "locale is not UTF-8"))?;
    let data_root = ClientDataRoot::new(PathBuf::from(data_root))?;
    let catalog = ArchiveCatalog::discover(data_root, Locale::from_str(locale)?)?;
    println!("discovered {} stock archives", catalog.descriptors().len());

    let mut store = AssetStore::mount(catalog)?;
    for requested_path in requested_paths {
        let requested_path = requested_path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "asset path is not UTF-8")
        })?;
        let path = AssetPath::new(requested_path)?;
        if path.as_str() == "DBFILESCLIENT\\ANIMATIONDATA.DBC" {
            let animations = AnimationDataCatalog::load(&mut store)?;
            println!(
                "{} typed animation definitions\t{}",
                animations.definitions().len(),
                path
            );
            for animation in animations.definitions() {
                println!(
                    "  id={} name={} weapon_flags={:#010x} body_flags={:#010x} flags={:#010x} fallback={} behavior={} tier={}",
                    animation.id(),
                    animation.name(),
                    animation.weapon_flags(),
                    animation.body_flags(),
                    animation.flags(),
                    animation.fallback_id(),
                    animation.behavior_id(),
                    animation.behavior_tier()
                );
            }
            continue;
        }
        if path.as_str() == "DBFILESCLIENT\\CFG_CATEGORIES.DBC" {
            let categories = RealmCategoryCatalog::load(&mut store)?;
            println!(
                "{} typed realm categories\t{}",
                categories.categories().len(),
                path
            );
            for category in categories.categories() {
                println!(
                    "  id={} locale={:#010x} charset={:#010x} flags={:#010x} name_flags={:#010x} name={}",
                    category.id(),
                    category.locale_mask(),
                    category.character_set_mask(),
                    category.flags(),
                    category.name_flags(),
                    category.name()
                );
            }
            continue;
        }
        if path.as_str() == "DBFILESCLIENT\\CFG_CONFIGS.DBC" {
            let configurations = RealmConfigurationCatalog::load(&mut store)?;
            println!(
                "{} typed realm configurations\t{}",
                configurations.configurations().len(),
                path
            );
            for configuration in configurations.configurations() {
                println!(
                    "  id={} realm_type={} pvp={} rp={}",
                    configuration.id(),
                    configuration.realm_type(),
                    configuration.player_killing_allowed(),
                    configuration.roleplaying()
                );
            }
            continue;
        }
        if path.as_str().ends_with(".DBC") {
            let table = WdbcTable::load(&mut store, &path)?;
            println!(
                "{} records x {} bytes\t{}\t{}",
                table.header().record_count(),
                table.header().record_size(),
                table.source().relative_path().display(),
                path
            );
            continue;
        }
        if path.as_str().ends_with(".BLP") {
            let texture = DecodedBlpTexture::load(&mut store, &path)?;
            println!(
                "{}x{} RGBA8\t{}\t{}",
                texture.width(),
                texture.height(),
                texture.source().relative_path().display(),
                path
            );
            continue;
        }
        if path.as_str().ends_with(".BLS") {
            let stage = if path.as_str().contains("\\VERTEX\\") {
                BlsShaderStage::Vertex
            } else if path.as_str().contains("\\PIXEL\\") {
                BlsShaderStage::Pixel
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "BLS path does not identify its vertex or pixel stage",
                )
                .into());
            };
            let shader = DecodedBlsShader::load(&mut store, &path, stage)?;
            println!(
                "{} {:?} permutations\t{}\t{}",
                shader.permutations().len(),
                shader.stage(),
                shader.source().relative_path().display(),
                path
            );
            continue;
        }

        let read = store.read(&path)?;
        println!(
            "{} bytes\t{}\t{}",
            read.bytes().len(),
            read.source().relative_path().display(),
            path
        );
    }

    Ok(())
}

/// Creates one consistent usage failure for incomplete command lines.
fn usage_error(executable: &std::ffi::OsStr) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "usage: {} <Data directory> <locale> <archive path> [archive path ...]",
            PathBuf::from(executable).display()
        ),
    )
}

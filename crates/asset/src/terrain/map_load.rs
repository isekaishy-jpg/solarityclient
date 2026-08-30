//! Strict build-12340 WDT loading through the mounted file stack.

use std::io::Cursor;

use wow_wdt::version::WowVersion;
use wow_wdt::{WdtFile, WdtReader};

use crate::archive::{AssetError, AssetPath};
use crate::database::MapDefinition;
use crate::file_stack::AssetStore;

use super::map::TerrainMap;
use super::map_area::{TerrainTile, TerrainTileIndex};

impl TerrainMap {
    /// Loads and validates the selected map's exact WDT manifest.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] when resolution, WDT parsing, build-era layout,
    /// global-WMO structure, or map directory paths are invalid.
    pub fn load(store: &mut AssetStore, definition: &MapDefinition) -> Result<Self, AssetError> {
        let path = Self::wdt_path(definition)?;
        let read = store.read(&path)?;
        validate_chunk_stream(&path, read.bytes())?;
        let mut reader = WdtReader::new(Cursor::new(read.bytes()), WowVersion::WotLK);
        let wdt = reader.read().map_err(|error| terrain_error(&path, error))?;
        validate_build_layout(&path, &wdt)?;
        let global_world_model = global_world_model(&path, &wdt)?;
        let mut tiles = Vec::with_capacity(64 * 64);
        for y in 0_u8..64 {
            for x in 0_u8..64 {
                let info = wdt
                    .get_tile(usize::from(x), usize::from(y))
                    .ok_or_else(|| AssetError::TerrainDecode {
                        path: path.clone(),
                        message: format!("WDT MAIN omits tile [{x}, {y}]"),
                    })?;
                let index =
                    TerrainTileIndex::new(x, y).ok_or_else(|| AssetError::TerrainDecode {
                        path: path.clone(),
                        message: format!("WDT MAIN has invalid tile [{x}, {y}]"),
                    })?;
                tiles.push(TerrainTile::new(index, info.flags, info.area_id));
            }
        }
        TerrainMap::new(
            definition,
            read.source().clone(),
            wdt.mphd.flags.bits(),
            global_world_model,
            tiles,
        )
    }
}

fn validate_chunk_stream(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    const ALLOWED_CHUNKS: [[u8; 4]; 5] = [*b"REVM", *b"DHPM", *b"NIAM", *b"OMWM", *b"FDOM"];
    let mut offset = 0_usize;
    let mut seen = Vec::with_capacity(ALLOWED_CHUNKS.len());
    while offset < bytes.len() {
        let header_end = offset
            .checked_add(8)
            .ok_or_else(|| terrain_message(path, "WDT chunk header offset overflow"))?;
        let header = bytes
            .get(offset..header_end)
            .ok_or_else(|| terrain_message(path, "WDT ends inside a chunk header"))?;
        let magic: [u8; 4] = header[0..4]
            .try_into()
            .map_err(|_| terrain_message(path, "WDT chunk magic is truncated"))?;
        if !ALLOWED_CHUNKS.contains(&magic) {
            return Err(terrain_message(
                path,
                format!(
                    "build-12340 WDT contains unknown chunk {:?}",
                    String::from_utf8_lossy(&magic)
                ),
            ));
        }
        if seen.contains(&magic) {
            return Err(terrain_message(
                path,
                format!(
                    "build-12340 WDT repeats chunk {:?}",
                    String::from_utf8_lossy(&magic)
                ),
            ));
        }
        seen.push(magic);
        let size = u32::from_le_bytes(
            header[4..8]
                .try_into()
                .map_err(|_| terrain_message(path, "WDT chunk size is truncated"))?,
        );
        offset = header_end
            .checked_add(size as usize)
            .ok_or_else(|| terrain_message(path, "WDT chunk extent overflow"))?;
        if offset > bytes.len() {
            return Err(terrain_message(
                path,
                format!("WDT chunk extends to {offset} beyond {} bytes", bytes.len()),
            ));
        }
    }
    Ok(())
}

fn validate_build_layout(path: &AssetPath, wdt: &WdtFile) -> Result<(), AssetError> {
    if wdt.maid.is_some() || wdt.mphd.has_maid() {
        return Err(AssetError::TerrainDecode {
            path: path.clone(),
            message: "build-12340 WDT cannot contain a MAID file-data-id table".to_owned(),
        });
    }
    if wdt.is_wmo_only() {
        if wdt.mwmo.as_ref().map(|chunk| chunk.filenames.len()) != Some(1)
            || wdt.modf.as_ref().map(|chunk| chunk.entries.len()) != Some(1)
        {
            return Err(AssetError::TerrainDecode {
                path: path.clone(),
                message: "global-WMO WDT requires exactly one MWMO name and one MODF placement"
                    .to_owned(),
            });
        }
    } else if wdt.modf.is_some() {
        return Err(AssetError::TerrainDecode {
            path: path.clone(),
            message: "terrain WDT contains a global MODF placement".to_owned(),
        });
    }
    Ok(())
}

fn global_world_model(path: &AssetPath, wdt: &WdtFile) -> Result<Option<AssetPath>, AssetError> {
    if !wdt.is_wmo_only() {
        return Ok(None);
    }
    let filename = &wdt
        .mwmo
        .as_ref()
        .ok_or_else(|| AssetError::TerrainDecode {
            path: path.clone(),
            message: "global-WMO WDT omits MWMO".to_owned(),
        })?
        .filenames[0];
    AssetPath::new(filename)
        .map(Some)
        .map_err(|error| AssetError::TerrainDecode {
            path: path.clone(),
            message: format!("invalid global WMO path {filename:?}: {error}"),
        })
}

fn terrain_error(path: &AssetPath, error: impl std::fmt::Display) -> AssetError {
    terrain_message(path, error.to_string())
}

fn terrain_message(path: &AssetPath, message: impl Into<String>) -> AssetError {
    AssetError::TerrainDecode {
        path: path.clone(),
        message: message.into(),
    }
}

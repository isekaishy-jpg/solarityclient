//! Minimap.cpp's logical tile names joined to packed texture names.

use std::collections::HashMap;

use crate::{AssetError, AssetPath, AssetStore, TerrainTileIndex};

const TABLE_PATH: &str = "Textures\\Minimap\\md5translate.trs";

/// The archive-selected terrain and WMO minimap texture translations.
#[derive(Default)]
pub struct MinimapTextureCatalog {
    textures: HashMap<AssetPath, AssetPath>,
}

impl MinimapTextureCatalog {
    /// Loads the translation table using normal archive precedence.
    ///
    /// Like build 12340's `0x007F6540`, an absent table produces an empty
    /// catalog, directory records and unpaired lines are ignored, and later
    /// entries replace earlier entries with the same case-insensitive name.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for archive read failures other than absence,
    /// or for text and path entries that cannot enter the typed asset boundary.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let path = AssetPath::new(TABLE_PATH)?;
        let read = match store.read(&path) {
            Ok(read) => read,
            Err(AssetError::AssetNotFound { .. }) => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let text =
            std::str::from_utf8(read.bytes()).map_err(|source| AssetError::MinimapDecode {
                path: path.clone(),
                message: source.to_string(),
            })?;
        let mut textures = HashMap::new();
        let text = text.split('\0').next().unwrap_or_default();
        for (index, line) in text.split(['\r', '\n']).enumerate() {
            if line.starts_with("dir:") {
                continue;
            }
            let Some((name, texture)) = line.split_once('\t') else {
                continue;
            };
            let entry = || -> Result<_, AssetError> {
                let texture = AssetPath::new(texture)?;
                Ok((
                    AssetPath::new(name)?,
                    AssetPath::new(format!("Textures\\Minimap\\{texture}"))?,
                ))
            };
            let (name, texture) = entry().map_err(|source| AssetError::MinimapDecode {
                path: path.clone(),
                message: format!("record {}: {source}", index + 1),
            })?;
            textures.insert(name, texture);
        }
        Ok(Self { textures })
    }

    /// Resolves an authored terrain or WMO tile name to its packed BLP path.
    #[must_use]
    pub fn texture(&self, logical_name: &AssetPath) -> Option<&AssetPath> {
        self.textures.get(logical_name)
    }

    /// Joins a Map.dbc directory and an ADT index through the native name format.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] if the map directory cannot form an asset path.
    pub fn terrain_texture(
        &self,
        map_directory: &str,
        tile: TerrainTileIndex,
    ) -> Result<Option<&AssetPath>, AssetError> {
        // 0x007F5240 uses AF4E5C: "%s\\map%d_%02d.blp".
        let path = AssetPath::new(format!(
            "{map_directory}\\map{}_{:02}.blp",
            tile.x(),
            tile.y(),
        ))?;
        Ok(self.texture(&path))
    }
}

//! Weather.dbc declarations consumed by the native SMSG_WEATHER receiver.

use super::{WdbcTable, localized::database_error};
use crate::{AssetError, AssetPath, AssetStore};
use std::collections::BTreeMap;

/// One eight-field weather declaration, including presentation resource inputs.
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherDefinition {
    id: u32,
    ambient_sound_id: i32,
    precipitation_type: u32,
    light_weight: f32,
    color: [f32; 3],
    texture: String,
}

impl WeatherDefinition {
    /// Returns the server-selected Weather.dbc key.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
    /// Returns the authored ambient sound key, including nonpositive sentinels.
    #[must_use]
    pub const fn ambient_sound_id(&self) -> i32 {
        self.ambient_sound_id
    }
    /// Returns zero for clear, one for rain, two for snow, three for a storm.
    #[must_use]
    pub const fn precipitation_type(&self) -> u32 {
        self.precipitation_type
    }
    /// Returns the weather palette's intensity multiplier.
    #[must_use]
    pub const fn light_weight(&self) -> f32 {
        self.light_weight
    }
    /// Returns the authored particle RGB inputs.
    #[must_use]
    pub const fn color(&self) -> [f32; 3] {
        self.color
    }
    /// Returns the authored precipitation texture, which may be empty.
    #[must_use]
    pub fn texture(&self) -> &str {
        &self.texture
    }
}

/// Exact weather declarations indexed by server-selected primary key.
#[derive(Default)]
pub struct WeatherCatalog {
    definitions: BTreeMap<u32, WeatherDefinition>,
}

impl WeatherCatalog {
    /// Loads the native eight-word schema through archive precedence.
    ///
    /// # Errors
    /// Returns an error for malformed schema, fields, strings or duplicate keys.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient/Weather.dbc")?)?;
        if table.header().field_count() != 8 || table.header().record_size() != 32 {
            return Err(database_error(
                &table,
                "build 12340 Weather.dbc requires eight fields and 32-byte records".to_owned(),
            ));
        }
        let mut definitions = BTreeMap::new();
        for row in 0..table.header().record_count() {
            let mut words = [0; 8];
            for (field, word) in words.iter_mut().enumerate() {
                *word = table.field_u32(row, field as u32).ok_or_else(|| {
                    database_error(&table, format!("truncated record {row}, field {field}"))
                })?;
            }
            let floats = [words[3], words[4], words[5], words[6]].map(f32::from_bits);
            if !floats.iter().all(|value| value.is_finite()) {
                return Err(database_error(
                    &table,
                    format!("record {row} has a non-finite weather value"),
                ));
            }
            let texture = table.string_bytes(words[7]).ok_or_else(|| {
                database_error(
                    &table,
                    format!(
                        "record {row} has invalid texture string offset {}",
                        words[7]
                    ),
                )
            })?;
            let texture = String::from_utf8(texture.to_vec())
                .map_err(|error| database_error(&table, error.to_string()))?;
            let definition = WeatherDefinition {
                id: words[0],
                ambient_sound_id: words[1] as i32,
                precipitation_type: words[2],
                light_weight: floats[0],
                color: [floats[1], floats[2], floats[3]],
                texture,
            };
            if definitions.insert(words[0], definition).is_some() {
                return Err(database_error(
                    &table,
                    format!("duplicate weather primary key {}", words[0]),
                ));
            }
        }
        Ok(Self { definitions })
    }

    /// Finds the exact server-selected record; native unknown IDs mean clear.
    #[must_use]
    pub fn definition(&self, id: u32) -> Option<&WeatherDefinition> {
        self.definitions.get(&id)
    }
}

//! Authored entry volumes from build-12340 AreaTrigger.dbc.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::localized::database_error;
use super::wow_client_db::WdbcTable;

/// Geometry selected by the row's nonzero radius, before box dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AreaTriggerShape {
    /// A sphere centered on the row's world position.
    Sphere {
        /// Authored radius, squared by the native containment predicate.
        radius: f32,
    },
    /// An oriented box with full dimensions and a Z-axis rotation.
    Box {
        /// Full width, length and height in world units.
        dimensions: [f32; 3],
        /// Rotation about the world's vertical axis.
        rotation_radians: f32,
    },
}

/// One notification identity and its authored world-space volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaTriggerDefinition {
    id: u32,
    map_id: u32,
    position: [f32; 3],
    shape: AreaTriggerShape,
}

impl AreaTriggerDefinition {
    /// Returns the identifier sent in `CMSG_AREATRIGGER`.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the map containing this volume.
    #[must_use]
    pub const fn map_id(self) -> u32 {
        self.map_id
    }

    /// Returns the world-space center.
    #[must_use]
    pub const fn position(self) -> [f32; 3] {
        self.position
    }

    /// Returns the radius-selected sphere or oriented box.
    #[must_use]
    pub const fn shape(self) -> AreaTriggerShape {
        self.shape
    }
}

/// AreaTrigger rows retained in the order scanned by native `6D2F70`.
pub struct AreaTriggerCatalog {
    entries: Vec<AreaTriggerDefinition>,
}

impl AreaTriggerCatalog {
    /// Loads the exact ten-word, forty-byte build-12340 layout.
    ///
    /// # Errors
    ///
    /// Returns [`AssetError`] for missing data, invalid layout or nonfinite geometry.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = WdbcTable::load(store, &AssetPath::new("DBFilesClient\\AreaTrigger.dbc")?)?;
        if table.header().field_count() != 10 || table.header().record_size() != 40 {
            return Err(database_error(
                &table,
                "build-12340 AreaTrigger.dbc requires 10 fields and 40-byte records".to_owned(),
            ));
        }
        let mut entries = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let field = |column| {
                table.field_u32(row, column).ok_or_else(|| {
                    database_error(&table, format!("record {row} field {column} is truncated"))
                })
            };
            let mut geometry = [0.; 8];
            for (index, value) in geometry.iter_mut().enumerate() {
                *value = f32::from_bits(field(index as u32 + 2)?);
                if !value.is_finite() {
                    return Err(database_error(
                        &table,
                        format!("record {row} geometry field {} is not finite", index + 2),
                    ));
                }
            }
            let [x, y, z, radius, width, length, height, rotation_radians] = geometry;
            entries.push(AreaTriggerDefinition {
                id: field(0)?,
                map_id: field(1)?,
                position: [x, y, z],
                shape: if radius != 0. {
                    AreaTriggerShape::Sphere { radius }
                } else {
                    AreaTriggerShape::Box {
                        dimensions: [width, length, height],
                        rotation_radians,
                    }
                },
            });
        }
        Ok(Self { entries })
    }

    /// Returns the original table order, including distinct overlapping entries.
    #[must_use]
    pub fn entries(&self) -> &[AreaTriggerDefinition] {
        &self.entries
    }
}

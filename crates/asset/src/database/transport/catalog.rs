//! Transport database ownership and native stored-order lookup.

use crate::archive::{AssetError, AssetPath};
use crate::file_stack::AssetStore;

use super::super::{WdbcTable, localized::database_error};
use super::rows::{
    TaxiPathNode, TransportAnimationNode, TransportPhysicsRecord, TransportRotationNode,
};

/// Exact transport records from the mounted stock archive stack.
pub struct TransportCatalog {
    paths: Vec<TaxiPathNode>,
    physics: Vec<TransportPhysicsRecord>,
    animations: Vec<TransportAnimationNode>,
    rotations: Vec<TransportRotationNode>,
}

impl TransportCatalog {
    /// Loads TaxiPathNode, TransportPhysics, TransportAnimation, and TransportRotation.
    ///
    /// Native 7F7AD0/70C8C0/70C930 lower-bound the stored signed owner keys.
    /// Rows within each owner remain in file order, including endpoint controls.
    ///
    /// # Errors
    /// Returns [`AssetError`] for missing tables, incompatible layouts, or owner
    /// key order that cannot satisfy the original client's binary lookup.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let paths = load_rows::<11>(store, "DBFilesClient\\TaxiPathNode.dbc", Some(1))?
            .into_iter()
            .map(|r| TaxiPathNode {
                id: r[0],
                path_id: r[1],
                node_index: r[2],
                map_id: r[3],
                position: [r[4], r[5], r[6]].map(f32::from_bits),
                flags: r[7],
                delay_seconds: r[8],
                arrival_event: r[9],
                departure_event: r[10],
            })
            .collect();
        let physics = load_rows::<11>(store, "DBFilesClient\\TransportPhysics.dbc", None)?
            .into_iter()
            .map(|r| TransportPhysicsRecord {
                id: r[0],
                parameters: std::array::from_fn(|i| f32::from_bits(r[i + 1])),
            })
            .collect();
        let animations = load_rows::<7>(store, "DBFilesClient\\TransportAnimation.dbc", Some(1))?
            .into_iter()
            .map(|r| TransportAnimationNode {
                id: r[0],
                entry: r[1],
                time_ms: r[2],
                position: [r[3], r[4], r[5]].map(f32::from_bits),
                sequence_id: r[6],
            })
            .collect();
        let rotations = load_rows::<7>(store, "DBFilesClient\\TransportRotation.dbc", Some(1))?
            .into_iter()
            .map(|r| TransportRotationNode {
                id: r[0],
                entry: r[1],
                time_ms: r[2],
                rotation: [r[3], r[4], r[5], r[6]].map(f32::from_bits),
            })
            .collect();
        Ok(Self {
            paths,
            physics,
            animations,
            rotations,
        })
    }

    /// Returns a route's contiguous stored controls; unknown routes are empty.
    #[must_use]
    pub fn path(&self, id: u32) -> &[TaxiPathNode] {
        let start = self
            .paths
            .partition_point(|row| (row.path_id as i32) < id as i32);
        let count = self.paths[start..].partition_point(|row| row.path_id == id);
        &self.paths[start..start + count]
    }

    /// Finds an optional physics row, preserving native absence for missing keys.
    #[must_use]
    pub fn physics(&self, id: u32) -> Option<&TransportPhysicsRecord> {
        self.physics.iter().find(|row| row.id == id)
    }

    /// Returns the entry's contiguous position keys in their original order.
    #[must_use]
    pub fn animation(&self, entry: u32) -> &[TransportAnimationNode] {
        let start = self
            .animations
            .partition_point(|row| (row.entry as i32) < entry as i32);
        let count = self.animations[start..].partition_point(|row| row.entry == entry);
        &self.animations[start..start + count]
    }

    /// Returns the entry's contiguous quaternion keys in their original order.
    #[must_use]
    pub fn rotation(&self, entry: u32) -> &[TransportRotationNode] {
        let start = self
            .rotations
            .partition_point(|row| (row.entry as i32) < entry as i32);
        let count = self.rotations[start..].partition_point(|row| row.entry == entry);
        &self.rotations[start..start + count]
    }
}

/// Validates the fixed schema before copying exact words, including float bits.
fn load_rows<const N: usize>(
    store: &mut AssetStore,
    path: &str,
    owner_column: Option<usize>,
) -> Result<Vec<[u32; N]>, AssetError> {
    let table = WdbcTable::load(store, &AssetPath::new(path)?)?;
    if table.header().field_count() != N as u32 || table.header().record_size() != (N * 4) as u32 {
        return Err(database_error(
            &table,
            format!(
                "build-12340 transport table requires {N} fields and {}-byte records",
                N * 4
            ),
        ));
    }
    let mut rows: Vec<[u32; N]> = Vec::with_capacity(table.header().record_count() as usize);
    for index in 0..table.header().record_count() {
        let mut row = [0; N];
        for (column, value) in row.iter_mut().enumerate() {
            *value = table.field_u32(index, column as u32).ok_or_else(|| {
                database_error(
                    &table,
                    format!("record {index} field {column} is truncated"),
                )
            })?;
        }
        if let Some(column) = owner_column
            && rows
                .last()
                .is_some_and(|previous| (previous[column] as i32) > row[column] as i32)
        {
            return Err(database_error(
                &table,
                format!("record {index} descends in native owner-key order"),
            ));
        }
        rows.push(row);
    }
    Ok(rows)
}

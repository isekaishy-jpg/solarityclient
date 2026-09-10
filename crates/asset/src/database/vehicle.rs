//! Exact build-12340 vehicle-to-seat joins used by unit presentation.

use super::wow_client_db::WdbcTable;
use crate::{AssetError, AssetPath, AssetStore};

/// Vehicle.dbc owner definition; empty seat slots retain identifier zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VehicleDefinition {
    id: u32,
    seats: [u32; 8],
}

impl VehicleDefinition {
    /// Returns the vehicle identifier carried by create movement.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the authored seat identifier for a valid passenger seat byte.
    /// An empty slot returns zero; an invalid index has no slot.
    #[must_use]
    pub fn seat_id(self, index: i8) -> Option<u32> {
        self.seats.get(index as u8 as usize).copied()
    }
}

/// VehicleSeat.dbc fields consumed by the native passenger attachment policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VehicleSeatDefinition {
    id: u32,
    flags: u32,
    attachment_id: i32,
}

impl VehicleSeatDefinition {
    /// Returns the seat record identifier, distinct from the passenger seat index.
    #[must_use]
    pub const fn id(self) -> u32 {
        self.id
    }

    /// Returns the first authored seat flags word at row +4.
    #[must_use]
    pub const fn flags(self) -> u32 {
        self.flags
    }

    /// Returns the signed attachment identifier at row +8. Native 716650 tests
    /// this field's sign when deciding whether a passenger may fade in.
    #[must_use]
    pub const fn attachment_id(self) -> i32 {
        self.attachment_id
    }
}

/// Immutable vehicle and passenger-seat tables under normal archive precedence.
#[derive(Default)]
pub struct VehicleCatalog {
    vehicles: Vec<VehicleDefinition>,
    seats: Vec<VehicleSeatDefinition>,
}

impl VehicleCatalog {
    /// Loads the 40-field Vehicle and 58-field VehicleSeat build-12340 tables.
    /// Missing seat references remain unresolved, matching native indexed lookup.
    ///
    /// # Errors
    /// Returns an asset failure for missing tables, incompatible schemas or
    /// duplicate identifiers.
    pub fn load(store: &mut AssetStore) -> Result<Self, AssetError> {
        let table = load_table(store, "DBFilesClient/Vehicle.dbc", 40)?;
        let mut vehicles = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            let mut seats = [0; 8];
            for (slot, id) in seats.iter_mut().enumerate() {
                *id = field(&table, row, 6 + slot as u32)?;
            }
            vehicles.push(VehicleDefinition {
                id: field(&table, row, 0)?,
                seats,
            });
        }
        sort_unique(&table, &mut vehicles, |row| row.id)?;
        let table = load_table(store, "DBFilesClient/VehicleSeat.dbc", 58)?;
        let mut seats = Vec::with_capacity(table.header().record_count() as usize);
        for row in 0..table.header().record_count() {
            seats.push(VehicleSeatDefinition {
                id: field(&table, row, 0)?,
                flags: field(&table, row, 1)?,
                attachment_id: field(&table, row, 2)? as i32,
            });
        }
        sort_unique(&table, &mut seats, |row| row.id)?;
        Ok(Self { vehicles, seats })
    }

    /// Finds an exact Vehicle.dbc row, without replacing missing or zero IDs.
    #[must_use]
    pub fn vehicle(&self, id: u32) -> Option<VehicleDefinition> {
        self.vehicles
            .binary_search_by_key(&id, |row| row.id)
            .ok()
            .map(|i| self.vehicles[i])
    }

    /// Finds an exact VehicleSeat.dbc row.
    #[must_use]
    pub fn seat(&self, id: u32) -> Option<VehicleSeatDefinition> {
        self.seats
            .binary_search_by_key(&id, |row| row.id)
            .ok()
            .map(|i| self.seats[i])
    }

    /// Native 5D3340/756EC0 joins the parent's vehicle row and seat-index byte.
    #[must_use]
    pub fn passenger_seat(&self, vehicle_id: u32, index: i8) -> Option<VehicleSeatDefinition> {
        self.seat(self.vehicle(vehicle_id)?.seat_id(index)?)
    }
}

fn load_table(store: &mut AssetStore, path: &str, fields: u32) -> Result<WdbcTable, AssetError> {
    let table = WdbcTable::load(store, &AssetPath::new(path)?)?;
    if table.header().field_count() != fields || table.header().record_size() != fields * 4 {
        return Err(error(
            &table,
            format!(
                "build-12340 requires {fields} fields and {}-byte records",
                fields * 4
            ),
        ));
    }
    Ok(table)
}

fn field(table: &WdbcTable, row: u32, column: u32) -> Result<u32, AssetError> {
    table
        .field_u32(row, column)
        .ok_or_else(|| error(table, format!("record {row} field {column} is truncated")))
}

fn sort_unique<T>(
    table: &WdbcTable,
    rows: &mut [T],
    key: impl Fn(&T) -> u32,
) -> Result<(), AssetError> {
    rows.sort_unstable_by_key(&key);
    if rows.windows(2).any(|pair| key(&pair[0]) == key(&pair[1])) {
        return Err(error(table, "duplicate identifier".to_owned()));
    }
    Ok(())
}

fn error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}

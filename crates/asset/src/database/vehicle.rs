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
    offset: [u32; 3],
    rotation: [u32; 3],
    passenger_attachment_id: i32,
    flags_b: u32,
    enter: [u32; 7],
    exit: [u32; 7],
    enter_animations: [u32; 2],
    seated_animations: [u32; 2],
    secondary_animations: [u32; 2],
    exit_animations: [u32; 2],
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

    /// Seat attachment offset in model units (row +0C).
    #[must_use]
    pub fn attachment_offset(self) -> [f32; 3] {
        self.offset.map(f32::from_bits)
    }

    /// Passenger yaw, pitch and roll in radians (row +74).
    #[must_use]
    pub fn passenger_rotation(self) -> [f32; 3] {
        self.rotation.map(f32::from_bits)
    }

    /// Attachment on the passenger model, whose static position anchors the seat.
    #[must_use]
    pub const fn passenger_attachment_id(self) -> i32 {
        self.passenger_attachment_id
    }

    /// Second flags word at row +B4.
    #[must_use]
    pub const fn flags_b(self) -> u32 {
        self.flags_b
    }

    /// Pre-delay, speed, gravity, minimum/maximum duration, and minimum/maximum arc.
    #[must_use]
    pub fn enter_transition(self) -> [f32; 7] {
        self.enter.map(f32::from_bits)
    }

    /// Exit counterpart of the entry transition parameters, starting at row +4C.
    #[must_use]
    pub fn exit_transition(self) -> [f32; 7] {
        self.exit.map(f32::from_bits)
    }

    /// Initial and looping passenger entry animations at row +34/+38; -1 is absent.
    #[must_use]
    pub fn enter_animations(self) -> [i32; 2] {
        self.enter_animations.map(|animation| animation as i32)
    }

    /// Initial and looping seated body animations at row +3C/+40.
    #[must_use]
    pub fn seated_animations(self) -> [i32; 2] {
        self.seated_animations.map(|animation| animation as i32)
    }

    /// Initial and looping seated secondary animations at row +44/+48.
    #[must_use]
    pub fn secondary_animations(self) -> [i32; 2] {
        self.secondary_animations.map(|animation| animation as i32)
    }

    /// Initial and looping passenger exit animations at row +68/+6C.
    #[must_use]
    pub fn exit_animations(self) -> [i32; 2] {
        self.exit_animations.map(|animation| animation as i32)
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
                offset: fields(&table, row, 3)?,
                enter: fields(&table, row, 6)?,
                exit: fields(&table, row, 19)?,
                enter_animations: fields(&table, row, 13)?,
                seated_animations: fields(&table, row, 15)?,
                secondary_animations: fields(&table, row, 17)?,
                exit_animations: fields(&table, row, 26)?,
                rotation: fields(&table, row, 29)?,
                passenger_attachment_id: field(&table, row, 32)? as i32,
                flags_b: field(&table, row, 45)?,
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

fn fields<const N: usize>(
    table: &WdbcTable,
    row: u32,
    column: u32,
) -> Result<[u32; N], AssetError> {
    let mut result = [0; N];
    for (index, value) in result.iter_mut().enumerate() {
        *value = field(table, row, column + index as u32)?;
    }
    Ok(result)
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

//! Bounded build-12340 world-object update decoding.

use std::io::Read;

use flate2::read::ZlibDecoder;
use thiserror::Error;

use super::movement::{ObjectMovementContext, ObjectMovementFall, ObjectMovementTransport};
use super::movement_spline::{MovementSplineFacing, MovementSplineSnapshot};

const SMSG_COMPRESSED_UPDATE_OBJECT: u16 = 0x01F6;
const MAX_UPDATE_BODY_BYTES: usize = 0x7F_FFFD;

const UPDATE_FLAG_SELF: u16 = 0x0001;
const UPDATE_FLAG_TRANSPORT: u16 = 0x0002;
const UPDATE_FLAG_ATTACKING_TARGET: u16 = 0x0004;
const UPDATE_FLAG_LOW_GUID: u16 = 0x0008;
const UPDATE_FLAG_HIGH_GUID: u16 = 0x0010;
const UPDATE_FLAG_LIVING: u16 = 0x0020;
const UPDATE_FLAG_HAS_POSITION: u16 = 0x0040;
const UPDATE_FLAG_VEHICLE: u16 = 0x0080;
const UPDATE_FLAG_POSITION: u16 = 0x0100;
const UPDATE_FLAG_ROTATION: u16 = 0x0200;
const KNOWN_UPDATE_FLAGS: u16 = 0x03FF;

const MOVEMENT_ON_TRANSPORT: u64 = 0x0000_0000_0200;
const MOVEMENT_FALLING: u64 = 0x0000_0000_1000;
const MOVEMENT_SWIMMING: u64 = 0x0000_0020_0000;
const MOVEMENT_FLYING: u64 = 0x0000_0200_0000;
const MOVEMENT_SPLINE_ELEVATION: u64 = 0x0000_0400_0000;
const MOVEMENT_SPLINE_ENABLED: u64 = 0x0000_0800_0000;
const MOVEMENT_ALWAYS_ALLOW_PITCHING: u64 = 0x0020_0000_0000;
const MOVEMENT_INTERPOLATED: u64 = 0x0400_0000_0000;

const SPLINE_FINAL_POINT: u32 = 0x0000_8000;
const SPLINE_FINAL_TARGET: u32 = 0x0001_0000;
const SPLINE_FINAL_ANGLE: u32 = 0x0002_0000;

/// Stock object category carried by create updates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldObjectKind {
    /// Base object.
    Object,
    /// Inventory item.
    Item,
    /// Inventory container.
    Container,
    /// Creature or unit.
    Unit,
    /// Player.
    Player,
    /// Static or interactive game object.
    GameObject,
    /// Dynamic spell object.
    DynamicObject,
    /// Corpse.
    Corpse,
}

/// One raw update-field word indexed by the build-12340 field table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectFieldUpdate {
    index: u16,
    value: u32,
}

impl ObjectFieldUpdate {
    /// Returns the build-12340 update-field index.
    #[must_use]
    pub const fn index(self) -> u16 {
        self.index
    }

    /// Returns the exact four-byte field word.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.value
    }
}

/// Position and facing present in a movement update block.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectMovementUpdate {
    update_flags: u16,
    movement_flags: Option<u64>,
    transport_guid: Option<u64>,
    context: Option<ObjectMovementContext>,
    spline: Option<MovementSplineSnapshot>,
    speeds: Option<ObjectMovementSpeeds>,
    position: Option<[f32; 3]>,
    orientation: Option<f32>,
    position_transport: Option<ObjectPositionTransport>,
    packed_rotation: Option<u64>,
    transport_progress_ms: Option<u32>,
}

/// Non-living passenger position carried by `UPDATEFLAG_POSITION`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectPositionTransport {
    /// Exact parent GUID; zero denotes an unattached position.
    pub guid: u64,
    /// Position in the parent's coordinate system.
    pub position: [f32; 3],
    /// Final orientation word, also used by corpse placement.
    pub orientation: f32,
}

impl ObjectMovementUpdate {
    /// Returns the path clock carried by `UPDATEFLAG_TRANSPORT`, including zero.
    #[must_use]
    pub const fn transport_progress_ms(&self) -> Option<u32> {
        self.transport_progress_ms
    }

    /// Returns creation `UpdateFlag` bits, or zero for a movement-only operation.
    #[must_use]
    pub const fn update_flags(&self) -> u16 {
        self.update_flags
    }

    /// Returns the exact 48-bit movement flags for a living block.
    #[must_use]
    pub const fn movement_flags(&self) -> Option<u64> {
        self.movement_flags
    }

    /// Returns the exact associated transport GUID carried by this block.
    #[must_use]
    pub const fn transport_guid(&self) -> Option<u64> {
        self.transport_guid
    }

    /// Returns the complete conditional living movement snapshot.
    #[must_use]
    pub const fn context(&self) -> Option<ObjectMovementContext> {
        self.context
    }

    /// Returns the retained path when the living movement enables a spline.
    #[must_use]
    pub const fn spline(&self) -> Option<&MovementSplineSnapshot> {
        self.spline.as_ref()
    }

    /// Returns all nine ordered speed values supplied by a living block.
    #[must_use]
    pub const fn speeds(&self) -> Option<ObjectMovementSpeeds> {
        self.speeds
    }

    /// Returns the world position when this block supplies one.
    #[must_use]
    pub const fn position(&self) -> Option<[f32; 3]> {
        self.position
    }

    /// Returns facing when this block supplies it.
    #[must_use]
    pub const fn orientation(&self) -> Option<f32> {
        self.orientation
    }

    /// Returns the non-living passenger offset without discarding a zero GUID.
    #[must_use]
    pub const fn position_transport(&self) -> Option<ObjectPositionTransport> {
        self.position_transport
    }

    /// Returns the exact packed local quaternion when `UPDATEFLAG_ROTATION` is set.
    #[must_use]
    pub const fn packed_rotation(&self) -> Option<u64> {
        self.packed_rotation
    }

    /// Returns whether this block identifies the controlled object.
    #[must_use]
    pub const fn is_self(&self) -> bool {
        self.update_flags & UPDATE_FLAG_SELF != 0
    }
}

/// The complete speed vector appended to a living object update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectMovementSpeeds {
    values: [f32; 9],
}

impl ObjectMovementSpeeds {
    /// Returns walk, run, run-back, swim, swim-back, flight, flight-back,
    /// turn-rate, and pitch-rate values in exact wire order.
    #[must_use]
    pub const fn values(self) -> [f32; 9] {
        self.values
    }
}

/// One stock object-update operation in packet order.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldObjectUpdate {
    /// Changed update-field words for an existing object.
    Values {
        /// Target GUID.
        guid: u64,
        /// Changed field words.
        fields: Vec<ObjectFieldUpdate>,
    },
    /// Movement-only update for an existing object.
    Movement {
        /// Target GUID.
        guid: u64,
        /// Movement state.
        movement: ObjectMovementUpdate,
    },
    /// Creation of an object entering client visibility.
    Create {
        /// Created GUID.
        guid: u64,
        /// Stock object category.
        kind: WorldObjectKind,
        /// Whether the server used `CREATE_OBJECT2`.
        second_form: bool,
        /// Initial movement state.
        movement: ObjectMovementUpdate,
        /// Initial field words.
        fields: Vec<ObjectFieldUpdate>,
    },
    /// Objects that left update range and must be removed.
    OutOfRange(Vec<u64>),
    /// Nearby GUID hints without object creation data.
    Near(Vec<u64>),
}

/// Ordered object operations from one normal or compressed server packet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldObjectUpdateBatch {
    updates: Vec<WorldObjectUpdate>,
}

impl WorldObjectUpdateBatch {
    pub(crate) fn decode(opcode: u16, payload: &[u8]) -> Result<Self, ObjectUpdateError> {
        let decompressed;
        let body = if opcode == SMSG_COMPRESSED_UPDATE_OBJECT {
            decompressed = decompress(payload)?;
            decompressed.as_slice()
        } else {
            payload
        };
        let mut cursor = UpdateCursor::new(body);
        let count = cursor.read_u32("object-update count is truncated")? as usize;
        if count > cursor.remaining() {
            return Err(cursor.error("object-update count exceeds the packet"));
        }
        let mut updates = Vec::with_capacity(count);
        for _ in 0..count {
            updates.push(decode_update(&mut cursor)?);
        }
        if cursor.remaining() != 0 {
            return Err(cursor.error("object-update packet has trailing bytes"));
        }
        Ok(Self { updates })
    }

    /// Returns operations in exact server order.
    #[must_use]
    pub fn updates(&self) -> &[WorldObjectUpdate] {
        &self.updates
    }
}

/// A malformed or invalid compressed object-update packet.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("malformed object update at byte {offset}: {message}")]
pub struct ObjectUpdateError {
    offset: usize,
    message: String,
}

impl ObjectUpdateError {
    /// Returns the byte offset at which decoding failed.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// Returns stable decoding context.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

fn decompress(payload: &[u8]) -> Result<Vec<u8>, ObjectUpdateError> {
    if payload.len() < 4 {
        return Err(ObjectUpdateError {
            offset: payload.len(),
            message: "compressed update size is truncated".to_owned(),
        });
    }
    let expected = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    if expected > MAX_UPDATE_BODY_BYTES {
        return Err(ObjectUpdateError {
            offset: 0,
            message: "decompressed update exceeds the world-packet bound".to_owned(),
        });
    }
    let compressed = &payload[4..];
    let mut decoder = ZlibDecoder::new(compressed);
    let mut body = Vec::with_capacity(expected);
    decoder
        .by_ref()
        .take((expected as u64) + 1)
        .read_to_end(&mut body)
        .map_err(|error| ObjectUpdateError {
            offset: 4 + decoder.total_in() as usize,
            message: format!("zlib object update is invalid: {error}"),
        })?;
    if body.len() != expected {
        return Err(ObjectUpdateError {
            offset: 4 + decoder.total_in() as usize,
            message: "decompressed update size does not match its declaration".to_owned(),
        });
    }
    if decoder.total_in() as usize != compressed.len() {
        return Err(ObjectUpdateError {
            offset: 4 + decoder.total_in() as usize,
            message: "compressed update has trailing bytes".to_owned(),
        });
    }
    Ok(body)
}

fn decode_update(cursor: &mut UpdateCursor<'_>) -> Result<WorldObjectUpdate, ObjectUpdateError> {
    match cursor.read_u8("object update type is truncated")? {
        0 => Ok(WorldObjectUpdate::Values {
            guid: cursor.read_packed_guid("value-update GUID is truncated")?,
            fields: cursor.read_fields()?,
        }),
        1 => Ok(WorldObjectUpdate::Movement {
            guid: cursor.read_packed_guid("movement-update GUID is truncated")?,
            movement: cursor.read_living_movement()?,
        }),
        update_type @ (2 | 3) => {
            let guid = cursor.read_packed_guid("create-update GUID is truncated")?;
            let kind_value = cursor.read_u8("create-update object type is truncated")?;
            let kind = decode_kind(kind_value).ok_or_else(|| {
                let mut error = cursor.error("unknown create-update object type");
                error.offset = error.offset.saturating_sub(1);
                error
            })?;
            let movement = cursor.read_create_movement()?;
            let fields = cursor.read_fields()?;
            Ok(WorldObjectUpdate::Create {
                guid,
                kind,
                second_form: update_type == 3,
                movement,
                fields,
            })
        }
        4 => Ok(WorldObjectUpdate::OutOfRange(cursor.read_guid_list()?)),
        5 => Ok(WorldObjectUpdate::Near(cursor.read_guid_list()?)),
        _ => Err(cursor.error("unknown object update type")),
    }
}

const fn decode_kind(value: u8) -> Option<WorldObjectKind> {
    Some(match value {
        0 => WorldObjectKind::Object,
        1 => WorldObjectKind::Item,
        2 => WorldObjectKind::Container,
        3 => WorldObjectKind::Unit,
        4 => WorldObjectKind::Player,
        5 => WorldObjectKind::GameObject,
        6 => WorldObjectKind::DynamicObject,
        7 => WorldObjectKind::Corpse,
        _ => return None,
    })
}

struct UpdateCursor<'a> {
    body: &'a [u8],
    offset: usize,
}

impl<'a> UpdateCursor<'a> {
    const fn new(body: &'a [u8]) -> Self {
        Self { body, offset: 0 }
    }

    fn error(&self, message: &'static str) -> ObjectUpdateError {
        ObjectUpdateError {
            offset: self.offset,
            message: message.to_owned(),
        }
    }

    const fn remaining(&self) -> usize {
        self.body.len() - self.offset
    }

    fn take(&mut self, count: usize, message: &'static str) -> Result<&'a [u8], ObjectUpdateError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| self.error(message))?;
        let bytes = self
            .body
            .get(self.offset..end)
            .ok_or_else(|| self.error(message))?;
        self.offset = end;
        Ok(bytes)
    }

    fn skip(&mut self, count: usize, message: &'static str) -> Result<(), ObjectUpdateError> {
        self.take(count, message).map(|_| ())
    }

    fn read_u8(&mut self, message: &'static str) -> Result<u8, ObjectUpdateError> {
        Ok(self.take(1, message)?[0])
    }

    fn read_u16(&mut self, message: &'static str) -> Result<u16, ObjectUpdateError> {
        let bytes: [u8; 2] = self
            .take(2, message)?
            .try_into()
            .map_err(|_| self.error(message))?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn read_u32(&mut self, message: &'static str) -> Result<u32, ObjectUpdateError> {
        let bytes: [u8; 4] = self
            .take(4, message)?
            .try_into()
            .map_err(|_| self.error(message))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn read_f32(&mut self, message: &'static str) -> Result<f32, ObjectUpdateError> {
        Ok(f32::from_bits(self.read_u32(message)?))
    }

    fn read_packed_guid(&mut self, message: &'static str) -> Result<u64, ObjectUpdateError> {
        let mask = self.read_u8(message)?;
        let mut guid = 0_u64;
        for byte_index in 0..8 {
            if mask & (1 << byte_index) != 0 {
                guid |= u64::from(self.read_u8(message)?) << (byte_index * 8);
            }
        }
        Ok(guid)
    }

    fn read_guid_list(&mut self) -> Result<Vec<u64>, ObjectUpdateError> {
        let count = self.read_u32("object GUID count is truncated")? as usize;
        if count > self.remaining() {
            return Err(self.error("object GUID count exceeds the packet"));
        }
        let mut guids = Vec::with_capacity(count);
        for _ in 0..count {
            guids.push(self.read_packed_guid("object GUID list is truncated")?);
        }
        Ok(guids)
    }

    fn read_fields(&mut self) -> Result<Vec<ObjectFieldUpdate>, ObjectUpdateError> {
        let block_count = usize::from(self.read_u8("update-mask block count is truncated")?);
        let mut blocks = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            blocks.push(self.read_u32("update-mask header is truncated")?);
        }
        let field_count = blocks.iter().map(|block| block.count_ones() as usize).sum();
        if field_count > self.remaining() / 4 {
            return Err(self.error("update-mask values exceed the packet"));
        }
        let mut fields = Vec::with_capacity(field_count);
        for (block_index, block) in blocks.into_iter().enumerate() {
            for bit in 0..32_u16 {
                if block & (1_u32 << bit) != 0 {
                    fields.push(ObjectFieldUpdate {
                        index: (block_index as u16) * 32 + bit,
                        value: self.read_u32("update-mask value is truncated")?,
                    });
                }
            }
        }
        Ok(fields)
    }

    fn read_position(&mut self) -> Result<[f32; 3], ObjectUpdateError> {
        Ok([
            self.read_f32("movement position is truncated")?,
            self.read_f32("movement position is truncated")?,
            self.read_f32("movement position is truncated")?,
        ])
    }

    fn read_create_movement(&mut self) -> Result<ObjectMovementUpdate, ObjectUpdateError> {
        let update_flags = self.read_u16("movement update flags are truncated")?;
        if update_flags & !KNOWN_UPDATE_FLAGS != 0 {
            return Err(self.error("movement update contains unknown flags"));
        }
        let mut movement = if update_flags & UPDATE_FLAG_LIVING != 0 {
            self.read_living_movement()?
        } else {
            ObjectMovementUpdate {
                update_flags: 0,
                movement_flags: None,
                transport_guid: None,
                context: None,
                spline: None,
                speeds: None,
                position: None,
                orientation: None,
                position_transport: None,
                packed_rotation: None,
                transport_progress_ms: None,
            }
        };
        movement.update_flags = update_flags;
        if update_flags & (UPDATE_FLAG_LIVING | UPDATE_FLAG_POSITION) == UPDATE_FLAG_POSITION {
            let guid = self.read_packed_guid("position transport GUID is truncated")?;
            movement.transport_guid = Some(guid);
            movement.position = Some(self.read_position()?);
            let offset = self.read_position()?;
            movement.orientation = Some(self.read_f32("position orientation is truncated")?);
            movement.position_transport = Some(ObjectPositionTransport {
                guid,
                position: offset,
                orientation: self.read_f32("corpse orientation is truncated")?,
            });
        } else if update_flags & (UPDATE_FLAG_LIVING | UPDATE_FLAG_HAS_POSITION)
            == UPDATE_FLAG_HAS_POSITION
        {
            movement.position = Some(self.read_position()?);
            movement.orientation = Some(self.read_f32("movement orientation is truncated")?);
        }
        if update_flags & UPDATE_FLAG_HIGH_GUID != 0 {
            self.skip(4, "movement high GUID value is truncated")?;
        }
        if update_flags & UPDATE_FLAG_LOW_GUID != 0 {
            self.skip(4, "movement low GUID value is truncated")?;
        }
        if update_flags & UPDATE_FLAG_ATTACKING_TARGET != 0 {
            self.read_packed_guid("attacking-target GUID is truncated")?;
        }
        if update_flags & UPDATE_FLAG_TRANSPORT != 0 {
            movement.transport_progress_ms =
                Some(self.read_u32("transport progress is truncated")?);
        }
        if update_flags & UPDATE_FLAG_VEHICLE != 0 {
            self.skip(8, "vehicle movement is truncated")?;
        }
        movement.packed_rotation = if update_flags & UPDATE_FLAG_ROTATION != 0 {
            let bytes = self.take(8, "packed local rotation is truncated")?;
            Some(u64::from_le_bytes(bytes.try_into().map_err(|_| {
                self.error("packed local rotation is truncated")
            })?))
        } else {
            None
        };
        Ok(movement)
    }

    /// Native 0x004F5090: MovementInfo, nine speeds, then the optional spline.
    /// Opcode-one blocks enter here directly; only creates prefix UpdateFlag.
    fn read_living_movement(&mut self) -> Result<ObjectMovementUpdate, ObjectUpdateError> {
        let low = u64::from(self.read_u32("living movement flags are truncated")?);
        let high = u64::from(self.read_u16("living movement flags are truncated")?);
        let flags = low | (high << 32);
        let timestamp_ms = self.read_u32("living movement timestamp is truncated")?;
        let position = self.read_position()?;
        let orientation = self.read_f32("living movement orientation is truncated")?;
        let transport = if flags & MOVEMENT_ON_TRANSPORT != 0 {
            let transport = self.read_transport_info(flags)?;
            Some(transport)
        } else {
            None
        };
        let pitch_radians = if flags
            & (MOVEMENT_SWIMMING | MOVEMENT_FLYING | MOVEMENT_ALWAYS_ALLOW_PITCHING)
            != 0
        {
            Some(self.read_f32("movement pitch is truncated")?)
        } else {
            None
        };
        let fall_time_ms = self.read_u32("movement fall time is truncated")?;
        let falling = if flags & MOVEMENT_FALLING != 0 {
            Some(ObjectMovementFall {
                vertical_speed: self.read_f32("falling vertical speed is truncated")?,
                direction_cos: self.read_f32("falling direction cosine is truncated")?,
                direction_sin: self.read_f32("falling direction sine is truncated")?,
                horizontal_speed: self.read_f32("falling horizontal speed is truncated")?,
            })
        } else {
            None
        };
        let spline_elevation = if flags & MOVEMENT_SPLINE_ELEVATION != 0 {
            Some(self.read_f32("spline elevation is truncated")?)
        } else {
            None
        };
        let context = ObjectMovementContext {
            timestamp_ms,
            transport,
            pitch_radians,
            fall_time_ms,
            falling,
            spline_elevation,
        };
        let speeds = ObjectMovementSpeeds {
            values: [
                self.read_f32("walk speed is truncated")?,
                self.read_f32("run speed is truncated")?,
                self.read_f32("run-back speed is truncated")?,
                self.read_f32("swim speed is truncated")?,
                self.read_f32("swim-back speed is truncated")?,
                self.read_f32("flight speed is truncated")?,
                self.read_f32("flight-back speed is truncated")?,
                self.read_f32("turn rate is truncated")?,
                self.read_f32("pitch rate is truncated")?,
            ],
        };
        let spline = if flags & MOVEMENT_SPLINE_ENABLED != 0 {
            Some(self.read_spline()?)
        } else {
            None
        };
        Ok(ObjectMovementUpdate {
            update_flags: 0,
            movement_flags: Some(flags),
            transport_guid: context.transport.map(|transport| transport.guid),
            context: Some(context),
            spline,
            speeds: Some(speeds),
            position: Some(position),
            orientation: Some(orientation),
            position_transport: None,
            packed_rotation: None,
            transport_progress_ms: None,
        })
    }

    /// Reads the conditional MovementInfo attachment without discarding its clocks or offsets.
    fn read_transport_info(
        &mut self,
        flags: u64,
    ) -> Result<ObjectMovementTransport, ObjectUpdateError> {
        let guid = self.read_packed_guid("transport GUID is truncated")?;
        let position = self.read_position()?;
        let orientation = self.read_f32("transport orientation is truncated")?;
        let time_ms = self.read_u32("transport time is truncated")?;
        let seat = self.read_u8("transport seat is truncated")? as i8;
        let interpolated_time_ms = if flags & MOVEMENT_INTERPOLATED != 0 {
            Some(self.read_u32("interpolated transport time is truncated")?)
        } else {
            None
        };
        Ok(ObjectMovementTransport {
            guid,
            position,
            orientation,
            time_ms,
            seat,
            interpolated_time_ms,
        })
    }

    /// Retains the complete native 004F4B50 snapshot and 004F4AE0 path array.
    fn read_spline(&mut self) -> Result<MovementSplineSnapshot, ObjectUpdateError> {
        let flags = self.read_u32("spline flags are truncated")?;
        let facing = if flags & SPLINE_FINAL_ANGLE != 0 {
            MovementSplineFacing::Angle(self.read_f32("spline final angle is truncated")?)
        } else if flags & SPLINE_FINAL_TARGET != 0 {
            let low = u64::from(self.read_u32("spline final target is truncated")?);
            let high = u64::from(self.read_u32("spline final target is truncated")?);
            MovementSplineFacing::Target(low | (high << 32))
        } else if flags & SPLINE_FINAL_POINT != 0 {
            MovementSplineFacing::Point(self.read_position()?)
        } else {
            MovementSplineFacing::Direction
        };
        let elapsed_ms = self.read_u32("spline elapsed time is truncated")?;
        let duration_ms = self.read_u32("spline duration is truncated")?;
        let id = self.read_u32("spline ID is truncated")?;
        let timing_parameters = [
            self.read_f32("spline timing is truncated")?,
            self.read_f32("spline timing is truncated")?,
            self.read_f32("spline timing is truncated")?,
        ];
        let effect_start_ms = self.read_u32("spline effect time is truncated")?;
        let node_count = self.read_u32("spline node count is truncated")? as usize;
        if node_count > self.remaining() / 12 {
            return Err(self.error("spline nodes are truncated"));
        }
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            nodes.push(self.read_position()?);
        }
        let mode = self.read_u8("spline mode is truncated")?;
        let destination = self.read_position()?;
        Ok(MovementSplineSnapshot {
            flags,
            facing,
            elapsed_ms,
            duration_ms,
            id,
            timing_parameters,
            effect_start_ms,
            nodes,
            mode,
            destination,
        })
    }
}

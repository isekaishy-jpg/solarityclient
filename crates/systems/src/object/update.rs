//! Projects the stock build-12340 update table into typed ECS components.

use solarity_ecs::{
    ActiveWorld, ObjectKind, ObjectPresentation, PlayerAppearance, UnitFlags, UnitIdentity,
    UnitPresentation, UnitVitals,
};
use thiserror::Error;

// These indices are the 3.3.5a build-12340 UpdateFields table. The dense raw
// field component remains authoritative; this list only defines hot typed
// views consumed by later gameplay and rendering systems.
const OBJECT_FIELD_ENTRY: u16 = 3;
const OBJECT_FIELD_SCALE_X: u16 = 4;
const UNIT_FIELD_BYTES_0: u16 = 23;
const UNIT_FIELD_HEALTH: u16 = 24;
const UNIT_FIELD_POWER_START: u16 = 25;
const UNIT_FIELD_POWER_END: u16 = 31;
const UNIT_FIELD_MAX_HEALTH: u16 = 32;
const UNIT_FIELD_MAX_POWER_START: u16 = 33;
const UNIT_FIELD_MAX_POWER_END: u16 = 39;
const UNIT_FIELD_LEVEL: u16 = 54;
const UNIT_FIELD_FACTION_TEMPLATE: u16 = 55;
const UNIT_FIELD_FLAGS: u16 = 59;
const UNIT_FIELD_FLAGS_2: u16 = 60;
const UNIT_FIELD_DISPLAY_ID: u16 = 67;
const UNIT_FIELD_NATIVE_DISPLAY_ID: u16 = 68;
const UNIT_FIELD_MOUNT_DISPLAY_ID: u16 = 69;
const UNIT_FIELD_BYTES_1: u16 = 74;
const UNIT_DYNAMIC_FLAGS: u16 = 79;
const PLAYER_FIELD_BYTES: u16 = 153;
const PLAYER_BYTES_2: u16 = 154;

/// Failure while projecting authoritative update words into component views.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ObjectProjectionError {
    /// The field update references an entity absent from the GUID registry.
    #[error("field projection references unknown object {guid:#018X}")]
    UnknownObject {
        /// Referenced server GUID.
        guid: u64,
    },
    /// The indexed entity has not received its stock object category.
    #[error("field projection references untyped object {guid:#018X}")]
    MissingObjectKind {
        /// Referenced server GUID.
        guid: u64,
    },
}

/// Applies sparse words to typed views without allocating another field list.
///
/// The caller must first apply the same words to [`solarity_ecs::ObjectFields`]
/// so the complete stock table remains the authoritative backing state.
/// Missing typed components are initialized to the field table's stock zeroed
/// state, which also permits the local-player bootstrap to be enriched by its
/// first create update.
///
/// # Errors
///
/// Returns [`ObjectProjectionError`] when the GUID is unknown or its create
/// update has not supplied an object category.
pub fn project_object_fields<I>(
    world: &mut ActiveWorld,
    guid: u64,
    fields: I,
) -> Result<(), ObjectProjectionError>
where
    I: IntoIterator<Item = (u16, u32)>,
{
    let entity = world
        .entity_by_guid(guid)
        .ok_or(ObjectProjectionError::UnknownObject { guid })?;
    let kind = {
        let kind = world
            .storage()
            .get::<&ObjectKind>(entity)
            .map_err(|_| ObjectProjectionError::MissingObjectKind { guid })?;
        **kind
    };

    // Snapshot existing typed views before taking mutable access to Shipyard.
    // A sparse values update may modify only one word, so all other projected
    // values must survive unchanged.
    let object = world
        .storage()
        .get::<&ObjectPresentation>(entity)
        .map(|component| **component)
        .ok();
    let mut object_changed = false;
    let object_state = object.unwrap_or_default();
    let mut entry_id = object_state.entry_id();
    let mut scale = object_state.scale();

    let unit_identity = world
        .storage()
        .get::<&UnitIdentity>(entity)
        .map(|component| **component)
        .ok();
    let mut identity_changed = false;
    let unit_identity_state = unit_identity.unwrap_or_default();
    let mut race_id = unit_identity_state.race_id();
    let mut class_id = unit_identity_state.class_id();
    let mut gender_id = unit_identity_state.gender_id();
    let mut power_type_id = unit_identity_state.power_type_id();
    let mut level = unit_identity_state.level();
    let mut faction_template_id = unit_identity_state.faction_template_id();

    let unit_vitals = world
        .storage()
        .get::<&UnitVitals>(entity)
        .map(|component| **component)
        .ok();
    let mut vitals_changed = false;
    let unit_vitals_state = unit_vitals.unwrap_or_default();
    let mut health = unit_vitals_state.health();
    let mut max_health = unit_vitals_state.max_health();
    let mut powers = unit_vitals_state.powers();
    let mut max_powers = unit_vitals_state.max_powers();

    let unit_presentation = world
        .storage()
        .get::<&UnitPresentation>(entity)
        .map(|component| **component)
        .ok();
    let mut presentation_changed = false;
    let unit_presentation_state = unit_presentation.unwrap_or_default();
    let mut display_id = unit_presentation_state.display_id();
    let mut native_display_id = unit_presentation_state.native_display_id();
    let mut mount_display_id = unit_presentation_state.mount_display_id();
    let mut stand_state = unit_presentation_state.stand_state();

    let unit_flags = world
        .storage()
        .get::<&UnitFlags>(entity)
        .map(|component| **component)
        .ok();
    let mut flags_changed = false;
    let unit_flags_state = unit_flags.unwrap_or_default();
    let mut primary_flags = unit_flags_state.primary();
    let mut secondary_flags = unit_flags_state.secondary();
    let mut dynamic_flags = unit_flags_state.dynamic();

    let player_appearance = world
        .storage()
        .get::<&PlayerAppearance>(entity)
        .map(|component| **component)
        .ok();
    let mut appearance_changed = false;
    let player_appearance_state = player_appearance.unwrap_or_default();
    let mut skin_id = player_appearance_state.skin_id();
    let mut face_id = player_appearance_state.face_id();
    let mut hair_style_id = player_appearance_state.hair_style_id();
    let mut hair_color_id = player_appearance_state.hair_color_id();
    let mut facial_hair_style_id = player_appearance_state.facial_hair_style_id();

    for (index, value) in fields {
        match index {
            OBJECT_FIELD_ENTRY => {
                entry_id = value;
                object_changed = true;
            }
            OBJECT_FIELD_SCALE_X => {
                scale = f32::from_bits(value);
                object_changed = true;
            }
            UNIT_FIELD_BYTES_0 if is_unit(kind) => {
                [race_id, class_id, gender_id, power_type_id] = value.to_le_bytes();
                identity_changed = true;
            }
            UNIT_FIELD_HEALTH if is_unit(kind) => {
                health = value;
                vitals_changed = true;
            }
            UNIT_FIELD_POWER_START..=UNIT_FIELD_POWER_END if is_unit(kind) => {
                powers[usize::from(index - UNIT_FIELD_POWER_START)] = value;
                vitals_changed = true;
            }
            UNIT_FIELD_MAX_HEALTH if is_unit(kind) => {
                max_health = value;
                vitals_changed = true;
            }
            UNIT_FIELD_MAX_POWER_START..=UNIT_FIELD_MAX_POWER_END if is_unit(kind) => {
                max_powers[usize::from(index - UNIT_FIELD_MAX_POWER_START)] = value;
                vitals_changed = true;
            }
            UNIT_FIELD_LEVEL if is_unit(kind) => {
                level = value;
                identity_changed = true;
            }
            UNIT_FIELD_FACTION_TEMPLATE if is_unit(kind) => {
                faction_template_id = value;
                identity_changed = true;
            }
            UNIT_FIELD_FLAGS if is_unit(kind) => {
                primary_flags = value;
                flags_changed = true;
            }
            UNIT_FIELD_FLAGS_2 if is_unit(kind) => {
                secondary_flags = value;
                flags_changed = true;
            }
            UNIT_FIELD_DISPLAY_ID if is_unit(kind) => {
                display_id = value;
                presentation_changed = true;
            }
            UNIT_FIELD_NATIVE_DISPLAY_ID if is_unit(kind) => {
                native_display_id = value;
                presentation_changed = true;
            }
            UNIT_FIELD_MOUNT_DISPLAY_ID if is_unit(kind) => {
                mount_display_id = value;
                presentation_changed = true;
            }
            UNIT_FIELD_BYTES_1 if is_unit(kind) => {
                stand_state = value.to_le_bytes()[0];
                presentation_changed = true;
            }
            UNIT_DYNAMIC_FLAGS if is_unit(kind) => {
                dynamic_flags = value;
                flags_changed = true;
            }
            PLAYER_FIELD_BYTES if kind == ObjectKind::Player => {
                [skin_id, face_id, hair_style_id, hair_color_id] = value.to_le_bytes();
                appearance_changed = true;
            }
            PLAYER_BYTES_2 if kind == ObjectKind::Player => {
                facial_hair_style_id = value.to_le_bytes()[0];
                appearance_changed = true;
            }
            _ => {}
        }
    }

    if object.is_none() || object_changed {
        world
            .storage_mut()
            .add_component(entity, (ObjectPresentation::new(entry_id, scale),));
    }
    if is_unit(kind) {
        if unit_identity.is_none() || identity_changed {
            world.storage_mut().add_component(
                entity,
                (UnitIdentity::new(
                    race_id,
                    class_id,
                    gender_id,
                    power_type_id,
                    level,
                    faction_template_id,
                ),),
            );
        }
        if unit_vitals.is_none() || vitals_changed {
            world.storage_mut().add_component(
                entity,
                (UnitVitals::new(health, max_health, powers, max_powers),),
            );
        }
        if unit_presentation.is_none() || presentation_changed {
            world.storage_mut().add_component(
                entity,
                (UnitPresentation::new(
                    display_id,
                    native_display_id,
                    mount_display_id,
                    stand_state,
                ),),
            );
        }
        if unit_flags.is_none() || flags_changed {
            world.storage_mut().add_component(
                entity,
                (UnitFlags::new(
                    primary_flags,
                    secondary_flags,
                    dynamic_flags,
                ),),
            );
        }
    }
    if kind == ObjectKind::Player && (player_appearance.is_none() || appearance_changed) {
        world.storage_mut().add_component(
            entity,
            (PlayerAppearance::new(
                skin_id,
                face_id,
                hair_style_id,
                hair_color_id,
                facial_hair_style_id,
            ),),
        );
    }
    Ok(())
}

/// Identifies the two object categories backed by the stock unit field range.
const fn is_unit(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Unit | ObjectKind::Player)
}

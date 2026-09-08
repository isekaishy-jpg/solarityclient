//! Projects the stock build-12340 update table into typed ECS components.

use solarity_ecs::{
    ActiveWorld, GameObjectPresentation, ObjectKind, ObjectPresentation, PlayerAppearance,
    PlayerEquipment, PlayerMoney, PlayerProgression, UnitAnimationTier, UnitFlags, UnitIdentity,
    UnitPresentation, UnitSheathState, UnitStats, UnitVitals, VisibleEquipmentItem,
};
use thiserror::Error;

// These indices are the 3.3.5a build-12340 UpdateFields table. The dense raw
// field component remains authoritative; this list only defines hot typed
// views consumed by later gameplay and rendering systems.
const OBJECT_FIELD_ENTRY: u16 = 3;
const OBJECT_FIELD_SCALE_X: u16 = 4;
const GAME_OBJECT_DISPLAY_ID: u16 = 8;
const GAME_OBJECT_FLAGS: u16 = 9;
const GAME_OBJECT_PARENT_ROTATION_START: u16 = 10;
const GAME_OBJECT_PARENT_ROTATION_END: u16 = 13;
const GAME_OBJECT_DYNAMIC: u16 = 14;
const GAME_OBJECT_LEVEL: u16 = 16;
const GAME_OBJECT_BYTES_1: u16 = 17;
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
const UNIT_FIELD_STAT_START: u16 = 84;
const UNIT_FIELD_STAT_END: u16 = 88;
const UNIT_FIELD_POSITIVE_STAT_START: u16 = 89;
const UNIT_FIELD_POSITIVE_STAT_END: u16 = 93;
const UNIT_FIELD_NEGATIVE_STAT_START: u16 = 94;
const UNIT_FIELD_NEGATIVE_STAT_END: u16 = 98;
const UNIT_FIELD_BYTES_2: u16 = 122;
const PLAYER_FIELD_BYTES: u16 = 153;
const PLAYER_BYTES_2: u16 = 154;
const PLAYER_VISIBLE_ITEM_START: u16 = 283;
const PLAYER_VISIBLE_ITEM_LAST: u16 = 320;
// Stock reads these adjacent private words in `UnitXP` and `UnitXPMax`.
const PLAYER_XP: u16 = 0x027A;
const PLAYER_NEXT_LEVEL_XP: u16 = 0x027B;
// `UNIT_END` is absolute word 0x0094. Build 12340 defines private player
// coinage at `UNIT_END + 0x03FE`, yielding update-mask word 0x0492.
const PLAYER_FIELD_COINAGE: u16 = 0x0492;

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
    /// The server supplied a sheath byte outside build 12340's closed range.
    #[error("unit {guid:#018X} has invalid sheath state {state}")]
    InvalidSheathState {
        /// Unit carrying the malformed byte.
        guid: u64,
        /// Unrecognized byte zero of `UNIT_FIELD_BYTES_2`.
        state: u8,
    },
    /// The server supplied an animation-tier byte outside build 12340's range.
    #[error("unit {guid:#018X} has invalid animation tier {tier}")]
    InvalidAnimationTier {
        /// Unit carrying the malformed byte.
        guid: u64,
        /// Unrecognized byte three of `UNIT_FIELD_BYTES_1`.
        tier: u8,
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

    let game_object_presentation = world
        .storage()
        .get::<&GameObjectPresentation>(entity)
        .map(|component| **component)
        .ok();
    let mut game_object_presentation_changed = false;
    let game_object_state = game_object_presentation.unwrap_or_default();
    let mut game_object_display_id = game_object_state.display_id();
    let mut game_object_bytes_1 = game_object_state.bytes_1();
    let mut game_object_flags = game_object_state.flags();
    let mut game_object_dynamic = game_object_state.dynamic_word();
    let mut game_object_transport_period_ms = game_object_state.transport_period_ms();
    let mut game_object_parent_rotation_bits = game_object_state.parent_rotation_bits();

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
    let mut health_changed = false;
    let unit_vitals_state = unit_vitals.unwrap_or_default();
    let mut health = unit_vitals_state.health();
    let mut max_health = unit_vitals_state.max_health();
    let mut powers = unit_vitals_state.powers();
    let mut max_powers = unit_vitals_state.max_powers();

    let unit_stats = world
        .storage()
        .get::<&UnitStats>(entity)
        .map(|component| **component)
        .ok();
    let mut stats_changed = false;
    let unit_stats_state = unit_stats.unwrap_or_default();
    let mut stats = unit_stats_state.values();
    let mut positive_stats = unit_stats_state.positive_modifiers();
    let mut negative_stats = unit_stats_state.negative_modifiers();

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
    let mut animation_tier = unit_presentation_state.animation_tier();
    let mut sheath_state = unit_presentation_state.sheath_state();

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

    let player_equipment = world
        .storage()
        .get::<&PlayerEquipment>(entity)
        .map(|component| **component)
        .ok();
    let mut equipment_changed = false;
    let mut equipment_items = player_equipment.unwrap_or_default().items();
    let player_money = world
        .storage()
        .get::<&PlayerMoney>(entity)
        .map(|component| **component)
        .ok();
    let mut player_money_update = None;
    let player_progression = world
        .storage()
        .get::<&PlayerProgression>(entity)
        .map(|component| **component)
        .ok();
    let player_progression_state = player_progression.unwrap_or_default();
    let mut experience = player_progression_state.experience();
    let mut next_level_experience = player_progression_state.next_level_experience();
    let mut progression_changed = false;

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
            GAME_OBJECT_DISPLAY_ID if kind == ObjectKind::GameObject => {
                game_object_display_id = value;
                game_object_presentation_changed = true;
            }
            GAME_OBJECT_BYTES_1 if kind == ObjectKind::GameObject => {
                game_object_bytes_1 = value;
                game_object_presentation_changed = true;
            }
            GAME_OBJECT_FLAGS if kind == ObjectKind::GameObject => {
                game_object_flags = value;
                game_object_presentation_changed = true;
            }
            GAME_OBJECT_PARENT_ROTATION_START..=GAME_OBJECT_PARENT_ROTATION_END
                if kind == ObjectKind::GameObject =>
            {
                game_object_parent_rotation_bits
                    [usize::from(index - GAME_OBJECT_PARENT_ROTATION_START)] = value;
                game_object_presentation_changed = true;
            }
            GAME_OBJECT_DYNAMIC if kind == ObjectKind::GameObject => {
                game_object_dynamic = value;
                game_object_presentation_changed = true;
            }
            GAME_OBJECT_LEVEL if kind == ObjectKind::GameObject => {
                game_object_transport_period_ms = value;
                game_object_presentation_changed = true;
            }
            UNIT_FIELD_BYTES_0 if is_unit(kind) => {
                [race_id, class_id, gender_id, power_type_id] = value.to_le_bytes();
                identity_changed = true;
            }
            UNIT_FIELD_HEALTH if is_unit(kind) => {
                health_changed |= health != value;
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
                let bytes = value.to_le_bytes();
                stand_state = bytes[0];
                animation_tier = UnitAnimationTier::try_from(bytes[3])
                    .map_err(|tier| ObjectProjectionError::InvalidAnimationTier { guid, tier })?;
                presentation_changed = true;
            }
            UNIT_DYNAMIC_FLAGS if is_unit(kind) => {
                dynamic_flags = value;
                flags_changed = true;
            }
            UNIT_FIELD_STAT_START..=UNIT_FIELD_STAT_END if is_unit(kind) => {
                stats[usize::from(index - UNIT_FIELD_STAT_START)] = value as i32;
                stats_changed = true;
            }
            UNIT_FIELD_POSITIVE_STAT_START..=UNIT_FIELD_POSITIVE_STAT_END if is_unit(kind) => {
                positive_stats[usize::from(index - UNIT_FIELD_POSITIVE_STAT_START)] =
                    f32::from_bits(value) as i32;
                stats_changed = true;
            }
            UNIT_FIELD_NEGATIVE_STAT_START..=UNIT_FIELD_NEGATIVE_STAT_END if is_unit(kind) => {
                negative_stats[usize::from(index - UNIT_FIELD_NEGATIVE_STAT_START)] =
                    f32::from_bits(value) as i32;
                stats_changed = true;
            }
            UNIT_FIELD_BYTES_2 if is_unit(kind) => {
                let state = value.to_le_bytes()[0];
                sheath_state = UnitSheathState::try_from(state)
                    .map_err(|state| ObjectProjectionError::InvalidSheathState { guid, state })?;
                presentation_changed = true;
            }
            PLAYER_FIELD_BYTES if kind == ObjectKind::Player => {
                [skin_id, face_id, hair_style_id, hair_color_id] = value.to_le_bytes();
                appearance_changed = true;
            }
            PLAYER_BYTES_2 if kind == ObjectKind::Player => {
                facial_hair_style_id = value.to_le_bytes()[0];
                appearance_changed = true;
            }
            PLAYER_VISIBLE_ITEM_START..=PLAYER_VISIBLE_ITEM_LAST if kind == ObjectKind::Player => {
                let offset = usize::from(index - PLAYER_VISIBLE_ITEM_START);
                let slot = offset / 2;
                let previous = equipment_items[slot];
                equipment_items[slot] = if offset % 2 == 0 {
                    VisibleEquipmentItem::new(value, previous.enchantment_word())
                } else {
                    VisibleEquipmentItem::new(previous.entry_id(), value)
                };
                equipment_changed = true;
            }
            PLAYER_XP if entity == world.local_player() => {
                experience = value;
                progression_changed = true;
            }
            PLAYER_NEXT_LEVEL_XP if entity == world.local_player() => {
                next_level_experience = value;
                progression_changed = true;
            }
            PLAYER_FIELD_COINAGE if kind == ObjectKind::Player => {
                player_money_update = Some(PlayerMoney::new(value));
            }
            _ => {}
        }
    }

    if object.is_none() || object_changed {
        world
            .storage_mut()
            .add_component(entity, (ObjectPresentation::new(entry_id, scale),));
    }
    if kind == ObjectKind::GameObject
        && (game_object_presentation.is_none() || game_object_presentation_changed)
    {
        world.storage_mut().add_component(
            entity,
            (GameObjectPresentation::from_fields(
                game_object_display_id,
                game_object_flags,
                game_object_bytes_1,
            )
            .with_dynamic_word(game_object_dynamic)
            .with_transport_period_ms(game_object_transport_period_ms)
            .with_parent_rotation_bits(game_object_parent_rotation_bits),),
        );
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
        // 73F330 reconciles FB0 when replicated health changes; unrelated
        // resource updates must retain prediction from admitted combat logs.
        if unit_vitals.is_none() || health_changed {
            world.storage_mut().add_component(
                entity,
                (solarity_ecs::UnitHealthPrediction::new(health as i32),),
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
                    animation_tier,
                    sheath_state,
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
    if kind == ObjectKind::Player && (player_equipment.is_none() || equipment_changed) {
        world
            .storage_mut()
            .add_component(entity, (PlayerEquipment::new(equipment_items),));
    }
    // The create mask omits zero-valued words, but the stock dense player
    // field array is already initialized to zero. Materialize the controlled
    // player's private zero values on its first typed projection so a max-level
    // character (zero XP and next-level XP) and a character carrying no money
    // remain authoritative inputs to synchronous FrameXML bootstrap.
    if kind == ObjectKind::Player && entity == world.local_player() {
        if player_money.is_none() || player_money_update.is_some() {
            world.storage_mut().add_component(
                entity,
                (player_money_update.unwrap_or(PlayerMoney::new(0)),),
            );
        }
        if unit_stats.is_none() || stats_changed {
            world.storage_mut().add_component(
                entity,
                (UnitStats::new(stats, positive_stats, negative_stats),),
            );
        }
        if player_progression.is_none() || progression_changed {
            world.storage_mut().add_component(
                entity,
                (PlayerProgression::new(experience, next_level_experience),),
            );
        }
    } else if let Some(player_money) = player_money_update {
        // Remote-player updates do not normally contain private coinage, but
        // preserve an explicitly supplied protocol word without inventing it.
        world.storage_mut().add_component(entity, (player_money,));
    }
    Ok(())
}

/// Identifies the two object categories backed by the stock unit field range.
const fn is_unit(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::Unit | ObjectKind::Player)
}

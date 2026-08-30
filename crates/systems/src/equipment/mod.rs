//! Equipment-set selection and mutation behavior evidenced by `EquipmentManager.cpp`.

mod equipment_manager;

pub use equipment_manager::{
    PlayerEquipmentAppearance, PlayerEquipmentAppearanceError, ResolvedEquipmentItem,
    resolve_player_equipment,
};

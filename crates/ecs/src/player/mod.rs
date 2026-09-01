//! Player-specific identity, selection, interaction, and local-control state.
//!
//! The boundary is evidenced by `Player_C.cpp`, `PlayerName.cpp`, and
//! `PlayerSound_C.cpp`; sound playback and player behavior remain outside this
//! state module.

mod economy;
mod equipment;
mod player_c;
mod player_name;

pub use economy::PlayerMoney;
pub use equipment::{
    PLAYER_EQUIPMENT_SLOT_COUNT, PlayerEquipment, PlayerEquipmentSlot, VisibleEquipmentItem,
};
pub use player_c::{LocalPlayer, PlayerAppearance, PlayerIdentity};

//! Loot-window item and roll presentation behavior evidenced by `LootFrame.cpp`.

mod loot_frame;
mod state;

pub use state::UiLootState;
pub(crate) use state::register_globals;

//! Player-specific identity, selection, interaction, and local-control state.
//!
//! The boundary is evidenced by `Player_C.cpp`, `PlayerName.cpp`, and
//! `PlayerSound_C.cpp`; sound playback and player behavior remain outside this
//! state module.

mod player_c;
mod player_name;

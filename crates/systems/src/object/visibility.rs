//! Ordinary player override of Unit_C model visibility in build 12340.

/// 6DE980's player flags can hide a model on every map or only in an arena.
/// Missing Map.dbc rows follow the non-arena branch.
#[must_use]
pub const fn player_flags_hide_model(flags: u32, arena: bool) -> bool {
    flags & 0x0008_0000 != 0 && (flags & 0x0040_0000 != 0 || arena)
}

#[cfg(test)]
#[path = "../../tests/object/visibility.rs"]
mod tests;

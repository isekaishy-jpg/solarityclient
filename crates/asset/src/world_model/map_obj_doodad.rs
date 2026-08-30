//! Root WMO doodad-set ranges and exact MODD placement records.

use crate::AssetPath;

/// One named MODS range into the root's MODD placement table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldModelDoodadSet {
    name: String,
    first_doodad: u32,
    doodad_count: u32,
    padding: u32,
}

impl WorldModelDoodadSet {
    pub(super) const fn new(
        name: String,
        first_doodad: u32,
        doodad_count: u32,
        padding: u32,
    ) -> Self {
        Self {
            name,
            first_doodad,
            doodad_count,
            padding,
        }
    }

    /// Returns the fixed-width MODS name without trailing null bytes.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the first MODD table index owned by this set.
    #[must_use]
    pub const fn first_doodad(&self) -> u32 {
        self.first_doodad
    }

    /// Returns the number of consecutive MODD entries owned by this set.
    #[must_use]
    pub const fn doodad_count(&self) -> u32 {
        self.doodad_count
    }

    /// Returns the final uninterpreted MODS word.
    #[must_use]
    pub const fn padding(&self) -> u32 {
        self.padding
    }
}

/// One root-local MODD M2 placement with its resolved MODN path.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldModelDoodad {
    path: AssetPath,
    name_offset: u32,
    flags: u8,
    position: [f32; 3],
    orientation: [f32; 4],
    scale: f32,
    color: [u8; 4],
}

impl WorldModelDoodad {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn new(
        path: AssetPath,
        name_offset: u32,
        flags: u8,
        position: [f32; 3],
        orientation: [f32; 4],
        scale: f32,
        color: [u8; 4],
    ) -> Self {
        Self {
            path,
            name_offset,
            flags,
            position,
            orientation,
            scale,
            color,
        }
    }

    /// Returns the resolved MODN model path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the exact lower 24-bit MODN byte offset.
    #[must_use]
    pub const fn name_offset(&self) -> u32 {
        self.name_offset
    }

    /// Returns the upper eight MODD name/flag bits.
    #[must_use]
    pub const fn flags(&self) -> u8 {
        self.flags
    }

    /// Returns the authored root-local position.
    #[must_use]
    pub const fn position(&self) -> [f32; 3] {
        self.position
    }

    /// Returns the authored root-local quaternion in X/Y/Z/W order.
    #[must_use]
    pub const fn orientation(&self) -> [f32; 4] {
        self.orientation
    }

    /// Returns the authored uniform scale.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// Returns the exact MODD BGRA color bytes.
    #[must_use]
    pub const fn color(&self) -> [u8; 4] {
        self.color
    }
}

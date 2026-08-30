//! Owned build-12340 WMO root state and group identity.

use crate::{ArchiveDescriptor, AssetPath};

use super::map_obj_group::DecodedWorldModelGroup;

/// One completely admitted WMO root and its independently resolved groups.
pub struct DecodedWorldModel {
    path: AssetPath,
    source: ArchiveDescriptor,
    flags: u16,
    world_model_id: u32,
    bounds: [[f32; 3]; 2],
    groups: Vec<DecodedWorldModelGroup>,
}

impl DecodedWorldModel {
    pub(super) const fn new(
        path: AssetPath,
        source: ArchiveDescriptor,
        flags: u16,
        world_model_id: u32,
        bounds: [[f32; 3]; 2],
        groups: Vec<DecodedWorldModelGroup>,
    ) -> Self {
        Self {
            path,
            source,
            flags,
            world_model_id,
            bounds,
            groups,
        }
    }

    /// Returns the canonical root-WMO archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected independently for the root file.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns the raw build-12340 MOHD flags.
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.flags
    }

    /// Returns the WMO identifier joined through `WMOAreaTable.dbc`.
    #[must_use]
    pub const fn world_model_id(&self) -> u32 {
        self.world_model_id
    }

    /// Returns root-local lower and upper authored bounds.
    #[must_use]
    pub const fn bounds(&self) -> [[f32; 3]; 2] {
        self.bounds
    }

    /// Returns every group in exact numeric file order.
    #[must_use]
    pub fn groups(&self) -> &[DecodedWorldModelGroup] {
        &self.groups
    }
}

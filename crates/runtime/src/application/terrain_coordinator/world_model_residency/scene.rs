//! Ordered MODF membership shares immutable source products across placements.
use super::ResidentWorldModelSource;
use glam::Vec3;
use solarity_asset::TerrainWorldModelPlacement;

/// One unique, chunk-referenced MODF instance.
pub(in crate::application) struct ResidentWorldModelPlacement {
    pub(super) source_index: usize,
    pub(super) unique_id: u32,
    pub(super) position: Vec3,
    pub(super) rotation_degrees: Vec3,
    pub(super) name_set: u16,
}

impl ResidentWorldModelPlacement {
    /// Returns the MODF name set used by the WMOAreaTable tuple lookup.
    pub(in crate::application) const fn name_set(&self) -> u16 {
        self.name_set
    }
    /// Returns the MODF identity shared by references in neighboring ADTs.
    pub(in crate::application) const fn unique_id(&self) -> u32 {
        self.unique_id
    }

    /// Returns the source-table slot shared by this placement.
    pub(in crate::application) const fn source_index(&self) -> usize {
        self.source_index
    }

    /// Returns the authored WoW world position.
    pub(in crate::application) const fn position(&self) -> Vec3 {
        self.position
    }

    /// Returns the authored MODF Euler angles in degrees.
    pub(in crate::application) const fn rotation_degrees(&self) -> Vec3 {
        self.rotation_degrees
    }
}

/// Immutable source table and unique MODF instance list for one ADT.
#[derive(Default)]
pub(in crate::application) struct ResidentWorldModelScene {
    pub(super) sources: Vec<ResidentWorldModelSource>,
    pub(super) placements: Vec<ResidentWorldModelPlacement>,
}

impl ResidentWorldModelScene {
    /// Returns distinct root-WMO generations in first-reference order.
    pub(in crate::application) fn sources(&self) -> &[ResidentWorldModelSource] {
        &self.sources
    }

    /// Returns unique MODF instances in first MCRF reference order.
    pub(in crate::application) fn placements(&self) -> &[ResidentWorldModelPlacement] {
        &self.placements
    }

    /// Returns the number of shared decoded root/group generations.
    pub(in super::super) const fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Returns the number of independently transformed instances.
    pub(in super::super) const fn placement_count(&self) -> usize {
        self.placements.len()
    }
}

/// Conflicting duplicate MODF IDs fail before any replacement is published.
pub(in super::super) fn same_world_model_placement(
    left: &TerrainWorldModelPlacement,
    right: &TerrainWorldModelPlacement,
) -> bool {
    left.path() == right.path()
        && left.position().map(f32::to_bits) == right.position().map(f32::to_bits)
        && left.rotation().map(f32::to_bits) == right.rotation().map(f32::to_bits)
        && left.bounds().map(|point| point.map(f32::to_bits))
            == right.bounds().map(|point| point.map(f32::to_bits))
        && left.flags() == right.flags()
        && left.doodad_set() == right.doodad_set()
        && left.name_set() == right.name_set()
}

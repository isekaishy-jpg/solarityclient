//! Ordinary residency publication and attachment membership.

use super::super::{M2Frame, M2GpuPlacementOwner};

impl M2Frame {
    /// Publishes a changed ordinary scene before animation visits any owner.
    pub(in super::super) fn publish_placement_topology(&mut self) {
        if !self.placement_topology_dirty {
            return;
        }
        let mut profile = solarity_profiling::profile!("M2 placement topology");
        // Changes to body residency can append a parent after retained
        // CEffects. Keep every effect behind its current parent pose.
        self.placements.order_effects_last();
        profile.mark("effect ordering");
        self.placement_visibility
            .rebuild(&mut self.placements, &self.sources);
        profile.mark("visibility publication");
        self.requested_items.clear();
        self.requested_items.extend(
            self.placement_visibility
                .dynamic_indices()
                .iter()
                .map(|&index| &self.placements[index])
                .filter_map(|placement| match placement.owner {
                    M2GpuPlacementOwner::Retired(_) => None,
                    M2GpuPlacementOwner::UnitItem { guid, point } => Some((guid, point)),
                    M2GpuPlacementOwner::UnitEffect { .. }
                    | M2GpuPlacementOwner::Static(_)
                    | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                    | M2GpuPlacementOwner::GlueModel { .. }
                    | M2GpuPlacementOwner::GluePet
                    | M2GpuPlacementOwner::PlayerBody { .. }
                    | M2GpuPlacementOwner::PlayerMount { .. }
                    | M2GpuPlacementOwner::RemotePlayerBody { .. }
                    | M2GpuPlacementOwner::RemotePlayerMount { .. }
                    | M2GpuPlacementOwner::CreatureBody { .. }
                    | M2GpuPlacementOwner::CreatureMount { .. }
                    | M2GpuPlacementOwner::GameObject { .. }
                    | M2GpuPlacementOwner::UnitItemVisual { .. } => None,
                }),
        );
        self.requested_visuals.clear();
        self.requested_visuals.extend(
            self.placement_visibility
                .dynamic_indices()
                .iter()
                .map(|&index| &self.placements[index])
                .filter_map(|placement| match placement.owner {
                    M2GpuPlacementOwner::Retired(_) => None,
                    M2GpuPlacementOwner::UnitItemVisual {
                        guid,
                        item_point,
                        effect_point,
                    } => Some((guid, item_point, effect_point)),
                    M2GpuPlacementOwner::UnitEffect { .. }
                    | M2GpuPlacementOwner::Static(_)
                    | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                    | M2GpuPlacementOwner::GlueModel { .. }
                    | M2GpuPlacementOwner::GluePet
                    | M2GpuPlacementOwner::PlayerBody { .. }
                    | M2GpuPlacementOwner::PlayerMount { .. }
                    | M2GpuPlacementOwner::RemotePlayerBody { .. }
                    | M2GpuPlacementOwner::RemotePlayerMount { .. }
                    | M2GpuPlacementOwner::CreatureBody { .. }
                    | M2GpuPlacementOwner::CreatureMount { .. }
                    | M2GpuPlacementOwner::GameObject { .. }
                    | M2GpuPlacementOwner::UnitItem { .. } => None,
                }),
        );
        self.mounted_guids.clear();
        self.mounted_guids.extend(
            self.placement_visibility
                .dynamic_indices()
                .iter()
                .map(|&index| &self.placements[index])
                .filter_map(|placement| match placement.owner {
                    M2GpuPlacementOwner::PlayerMount { guid }
                    | M2GpuPlacementOwner::RemotePlayerMount { guid }
                    | M2GpuPlacementOwner::CreatureMount { guid } => Some(guid),
                    _ => None,
                }),
        );
        self.glue_attachment_ids.clear();
        self.glue_attachment_ids.extend(
            self.placement_visibility
                .dynamic_indices()
                .iter()
                .map(|&index| &self.placements[index])
                .filter_map(|placement| placement.glue_parent_attachment),
        );
        self.glue_attachment_ids.sort_unstable();
        self.glue_attachment_ids.dedup();
        profile.mark("attachment membership");
        self.vehicle_passengers.invalidate();
        self.placement_topology_dirty = false;
    }
}

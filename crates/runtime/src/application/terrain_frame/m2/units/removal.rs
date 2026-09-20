//! Remove withdrawn unit families while retaining native placement order.

use super::super::{M2Frame, M2GpuPlacementOwner};
use std::collections::HashSet;

impl M2Frame {
    /// Drops local references to the previous player generation.
    pub(in super::super) fn remove_player(&mut self) {
        let topology_before = self.placements.len();
        self.placement_topology_dirty = true;
        let local_guid = self
            .placements
            .dynamic_indices()
            .iter()
            .map(|&index| &self.placements[index])
            .find_map(|placement| match placement.owner {
                M2GpuPlacementOwner::PlayerBody { guid } => Some(guid),
                _ => None,
            });
        let mut player_sources = Vec::new();
        self.placements.retain_dynamic(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::Retired(_) => false,
                M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::GluePet => true,
                M2GpuPlacementOwner::UnitItem { guid, .. }
                | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => local_guid == Some(guid),
                M2GpuPlacementOwner::UnitEffect { .. }
                | M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::RemotePlayerBody { .. }
                | M2GpuPlacementOwner::RemotePlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::CreatureMount { .. }
                | M2GpuPlacementOwner::GameObject { .. } => false,
            };
            if owned {
                player_sources.push(placement.source_index);
            }
            !owned
        });
        for source_index in player_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
        solarity_profiling::profile_event_value!(
            "m2.topology.local.removed",
            topology_before - self.placements.len()
        );
    }

    /// Drops local references to the previous visible-creature generation.
    pub(in super::super) fn remove_creatures(&mut self, retained: &[u64]) {
        let retained = retained.iter().copied().collect::<HashSet<_>>();
        let topology_before = self.placements.len();
        self.placement_topology_dirty = true;
        let removed_guids = self
            .placements
            .dynamic_indices()
            .iter()
            .map(|&index| &self.placements[index])
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::CreatureBody { guid } if !retained.contains(&guid) => {
                    Some(guid)
                }
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut creature_sources = Vec::new();
        self.placements.retain_dynamic(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::CreatureBody { guid }
                | M2GpuPlacementOwner::CreatureMount { guid } => !retained.contains(&guid),
                M2GpuPlacementOwner::UnitItem { guid, .. }
                | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => removed_guids.contains(&guid),
                _ => false,
            };
            if owned {
                creature_sources.push(placement.source_index);
            }
            !owned
        });
        for source_index in creature_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
        solarity_profiling::profile_event_value!(
            "m2.topology.creatures.removed",
            topology_before - self.placements.len()
        );
    }

    /// Drops remote character bodies and every child placement they own.
    pub(in super::super) fn remove_remote_players(&mut self, retained: &[u64]) {
        let retained = retained.iter().copied().collect::<HashSet<_>>();
        let topology_before = self.placements.len();
        self.placement_topology_dirty = true;
        let remote_guids = self
            .placements
            .dynamic_indices()
            .iter()
            .map(|&index| &self.placements[index])
            .filter_map(|placement| match placement.owner {
                M2GpuPlacementOwner::RemotePlayerBody { guid } if !retained.contains(&guid) => {
                    Some(guid)
                }
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut remote_sources = Vec::new();
        self.placements.retain_dynamic(|placement| {
            let owned = match placement.owner {
                M2GpuPlacementOwner::Retired(_) => false,
                M2GpuPlacementOwner::RemotePlayerBody { guid }
                | M2GpuPlacementOwner::RemotePlayerMount { guid } => !retained.contains(&guid),
                M2GpuPlacementOwner::UnitItem { guid, .. }
                | M2GpuPlacementOwner::UnitItemVisual { guid, .. } => remote_guids.contains(&guid),
                M2GpuPlacementOwner::UnitEffect { .. }
                | M2GpuPlacementOwner::Static(_)
                | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
                | M2GpuPlacementOwner::GlueModel { .. }
                | M2GpuPlacementOwner::GluePet
                | M2GpuPlacementOwner::PlayerBody { .. }
                | M2GpuPlacementOwner::PlayerMount { .. }
                | M2GpuPlacementOwner::CreatureBody { .. }
                | M2GpuPlacementOwner::CreatureMount { .. }
                | M2GpuPlacementOwner::GameObject { .. } => false,
            };
            if owned {
                remote_sources.push(placement.source_index);
            }
            !owned
        });
        for source_index in remote_sources {
            if let Some(source) = self.sources.get_mut(source_index) {
                *source = None;
            }
        }
        solarity_profiling::profile_event_value!(
            "m2.topology.remote_players.removed",
            topology_before - self.placements.len()
        );
    }
}

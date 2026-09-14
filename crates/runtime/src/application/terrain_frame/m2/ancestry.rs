//! Ordered attachment lookup without rescanning resident terrain for each child.

use std::collections::HashMap;
use std::rc::Rc;

use super::{M2GpuPlacement, M2GpuPlacementOwner};
use crate::application::unit_animation::UnitAnimationBehavior;

/// Keys reproduce the preceding-placement searches used by model attachment,
/// retirement and lighting. Inserting after lookup retains the last preceding
/// match, including duplicate owners, and never admits a forward parent.
#[derive(Default)]
pub(super) struct PlacementAncestry {
    owners: HashMap<M2GpuPlacementOwner, usize>,
    bodies: HashMap<u64, usize>,
    animations: HashMap<*const UnitAnimationBehavior, usize>,
    glue: Option<usize>,
}

/// Each attachment family has one existing parent-selection rule.
enum ParentBinding {
    None,
    Owner(M2GpuPlacementOwner),
    Body(u64),
    Animation(*const UnitAnimationBehavior),
    Glue,
}

impl PlacementAncestry {
    /// Rebuilds attachment owners from the compact dynamic membership. Static
    /// placements have no parent binding or animation owner, so effect changes
    /// can retain their existing parent slots without revisiting scenery.
    pub(super) fn rebuild_dynamic(
        &mut self,
        placements: &[M2GpuPlacement],
        indices: &[usize],
        parents: &mut Vec<Option<usize>>,
    ) {
        self.owners.clear();
        self.bodies.clear();
        self.animations.clear();
        self.glue = None;
        parents.resize(placements.len(), None);
        for &index in indices {
            let placement = &placements[index];
            parents[index] = self.parent(binding(placement));
            self.insert(
                placement.owner,
                placement.unit_animation.as_ref().map(Rc::as_ptr),
                index,
            );
        }
    }

    /// Resolves only declarations already encountered in native placement order.
    fn parent(&self, binding: ParentBinding) -> Option<usize> {
        match binding {
            ParentBinding::None => None,
            ParentBinding::Owner(owner) => self.owners.get(&owner).copied(),
            ParentBinding::Body(guid) => self.bodies.get(&guid).copied(),
            ParentBinding::Animation(owner) => self.animations.get(&owner).copied(),
            ParentBinding::Glue => self.glue,
        }
    }

    /// Pointer keys are consulted only during a rebuild, while placements own
    /// their Rc/Weak allocations. They are never dereferenced, and every rebuild
    /// clears the old maps before examining the current placement lifetimes.
    fn insert(
        &mut self,
        owner: M2GpuPlacementOwner,
        animation: Option<*const UnitAnimationBehavior>,
        index: usize,
    ) {
        if let Some(animation) = animation {
            self.animations.insert(animation, index);
        }
        match owner {
            M2GpuPlacementOwner::PlayerBody { guid }
            | M2GpuPlacementOwner::RemotePlayerBody { guid }
            | M2GpuPlacementOwner::CreatureBody { guid } => {
                self.bodies.insert(guid, index);
            }
            M2GpuPlacementOwner::PlayerMount { .. }
            | M2GpuPlacementOwner::RemotePlayerMount { .. }
            | M2GpuPlacementOwner::CreatureMount { .. }
            | M2GpuPlacementOwner::UnitItem { .. }
            | M2GpuPlacementOwner::Retired(_) => {
                self.owners.insert(owner, index);
            }
            M2GpuPlacementOwner::GlueModel { .. } => self.glue = Some(index),
            M2GpuPlacementOwner::Static(_)
            | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
            | M2GpuPlacementOwner::GluePet
            | M2GpuPlacementOwner::GameObject { .. }
            | M2GpuPlacementOwner::UnitItemVisual { .. }
            | M2GpuPlacementOwner::UnitEffect { .. } => {}
        }
    }
}

/// Retirement replaces parent identity before ordinary live-owner rules apply.
fn binding(placement: &M2GpuPlacement) -> ParentBinding {
    if let Some(retired) = &placement.retirement {
        return retired
            .parent()
            .map_or(ParentBinding::None, ParentBinding::Owner);
    }
    if placement.glue_parent_attachment.is_some() {
        return ParentBinding::Glue;
    }
    match placement.owner {
        M2GpuPlacementOwner::UnitEffect { .. } => placement
            .unit_effect
            .as_ref()
            .and_then(super::unit_effects::UnitEffectPlacement::attachment_owner)
            .map_or(ParentBinding::None, ParentBinding::Animation),
        M2GpuPlacementOwner::PlayerBody { guid } => {
            ParentBinding::Owner(M2GpuPlacementOwner::PlayerMount { guid })
        }
        M2GpuPlacementOwner::RemotePlayerBody { guid } => {
            ParentBinding::Owner(M2GpuPlacementOwner::RemotePlayerMount { guid })
        }
        M2GpuPlacementOwner::CreatureBody { guid } => {
            ParentBinding::Owner(M2GpuPlacementOwner::CreatureMount { guid })
        }
        M2GpuPlacementOwner::UnitItem { guid, .. } => ParentBinding::Body(guid),
        M2GpuPlacementOwner::UnitItemVisual {
            guid, item_point, ..
        } => ParentBinding::Owner(M2GpuPlacementOwner::UnitItem {
            guid,
            point: item_point,
        }),
        M2GpuPlacementOwner::Static(_)
        | M2GpuPlacementOwner::GameObjectWorldModelDoodad { .. }
        | M2GpuPlacementOwner::GlueModel { .. }
        | M2GpuPlacementOwner::GluePet
        | M2GpuPlacementOwner::PlayerMount { .. }
        | M2GpuPlacementOwner::RemotePlayerMount { .. }
        | M2GpuPlacementOwner::CreatureMount { .. }
        | M2GpuPlacementOwner::GameObject { .. }
        | M2GpuPlacementOwner::Retired(_) => ParentBinding::None,
    }
}

/// Retirement renames one hierarchy at a time. Its occasional point query must
/// observe those intervening owner changes rather than a pre-retirement snapshot.
pub(super) fn placement_parent(placements: &[M2GpuPlacement], index: usize) -> Option<usize> {
    let preceding = &placements[..index];
    match binding(&placements[index]) {
        ParentBinding::None => None,
        ParentBinding::Owner(owner) => preceding.iter().rposition(|entry| entry.owner == owner),
        ParentBinding::Body(guid) => preceding.iter().rposition(|entry| {
            matches!(entry.owner,
                M2GpuPlacementOwner::PlayerBody { guid: owner }
                | M2GpuPlacementOwner::RemotePlayerBody { guid: owner }
                | M2GpuPlacementOwner::CreatureBody { guid: owner } if owner == guid)
        }),
        ParentBinding::Animation(owner) => preceding.iter().rposition(|entry| {
            entry
                .unit_animation
                .as_ref()
                .is_some_and(|animation| Rc::as_ptr(animation) == owner)
        }),
        ParentBinding::Glue => preceding
            .iter()
            .rposition(|entry| matches!(entry.owner, M2GpuPlacementOwner::GlueModel { .. })),
    }
}

#[cfg(test)]
#[path = "../../../../tests/application/placement_ancestry.rs"]
mod tests;

//! First-visited WMO groups and all local portal clips for scene drawing.

use std::collections::HashMap;

use solarity_systems::{
    PlacedWorldModelCollision, WorldModelSceneFog, WorldModelSceneGroupVisit,
    WorldModelVisibilityError, WorldSceneFrustum,
};

use super::super::RuntimeWorldModelMovementOwner;

/// One native 799310 group entry, retained in first-callback order.
pub(in crate::application) struct WorldModelSceneGroup {
    pub(in crate::application) owner: RuntimeWorldModelMovementOwner,
    pub(in crate::application) group: usize,
    /// Any indoor callback sets the group's native 0x8000 fog bit.
    pub(in crate::application) indoor_fog: bool,
    /// Ordered regions transformed with the collision owner's retained inverse.
    pub(in crate::application) frusta: Vec<WorldSceneFrustum>,
}

/// Reuses group entries, clip allocations and their per-frame identity lookup.
#[derive(Default)]
pub(super) struct WorldModelSceneGraphics {
    groups: Vec<WorldModelSceneGroup>,
    active: usize,
    indices: HashMap<(RuntimeWorldModelMovementOwner, usize), usize>,
    /// CFBEB8 starts at zero and survives callbacks that perform no fog write.
    fog: bool,
}

impl WorldModelSceneGraphics {
    /// Starts a collection while retaining the preceding graphics fog bank.
    pub(super) fn begin(&mut self) {
        self.active = 0;
        self.indices.clear();
    }

    /// Appends every 799310 clip while deduplicating only the group queue entry.
    pub(super) fn record(
        &mut self,
        root: &PlacedWorldModelCollision,
        owner: RuntimeWorldModelMovementOwner,
        visit: WorldModelSceneGroupVisit,
    ) -> Result<(), WorldModelVisibilityError> {
        match visit.fog {
            WorldModelSceneFog::Inherited => {}
            WorldModelSceneFog::Indoor => self.fog = true,
            WorldModelSceneFog::Outdoor => self.fog = false,
        }
        let index = if let Some(&index) = self.indices.get(&(owner, visit.group)) {
            index
        } else {
            let index = self.active;
            if index == self.groups.len() {
                self.groups.push(WorldModelSceneGroup {
                    owner,
                    group: visit.group,
                    indoor_fog: false,
                    frusta: Vec::new(),
                });
            } else {
                let entry = &mut self.groups[index];
                entry.owner = owner;
                entry.group = visit.group;
                entry.indoor_fog = false;
                entry.frusta.clear();
            }
            self.indices.insert((owner, visit.group), index);
            self.active += 1;
            index
        };
        let entry = &mut self.groups[index];
        entry.indoor_fog |= self.fog;
        entry
            .frusta
            .push(visit.frustum.transformed(root.inverse_transform())?);
        Ok(())
    }

    /// Exposes only this frame's entries while retaining unused high-water storage.
    pub(super) fn groups(&self) -> &[WorldModelSceneGroup] {
        &self.groups[..self.active]
    }

    /// 796799 restores each submitted group's accumulated bank before drawing.
    /// If no group was submitted, the last callback's bank remains untouched.
    pub(super) fn complete(&mut self, last: Option<usize>) {
        if let Some(index) = last {
            self.fog = self.groups[index].indoor_fog;
        }
    }
}

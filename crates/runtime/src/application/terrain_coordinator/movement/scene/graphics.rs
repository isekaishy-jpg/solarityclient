//! First-visited WMO groups and all local portal clips for scene drawing.

use std::collections::HashMap;

use solarity_systems::{
    PlacedWorldModelCollision, WorldModelSceneFog, WorldModelSceneGroupVisit,
    WorldModelVisibilityError, WorldSceneDepthFrame, WorldSceneFrustum,
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
    pub(in crate::application) doodads: WorldModelSceneDoodads,
}

/// Doodads use world-space spheres, the full view depth and 79A260's group gate.
#[derive(Default)]
pub(in crate::application) struct WorldModelSceneDoodads {
    world_frusta: Vec<WorldSceneFrustum>,
    references: Vec<u16>,
    depth: f32,
    allowed: bool,
}

/// 799F80 submits models immediately, before later callbacks extend the group.
struct DirectDoodadVisit {
    group: usize,
    clip_count: usize,
    indoor_fog: bool,
}

struct OutdoorDoodadGroup {
    owner: RuntimeWorldModelMovementOwner,
    bin: u8,
    references: Vec<u16>,
}

/// Reuses group entries, clip allocations and their per-frame identity lookup.
#[derive(Default)]
pub(super) struct WorldModelSceneGraphics {
    groups: Vec<WorldModelSceneGroup>,
    active: usize,
    indices: HashMap<(RuntimeWorldModelMovementOwner, usize), usize>,
    /// CFBEB8 starts at zero and survives callbacks that perform no fog write.
    fog: bool,
    direct_doodad_visits: Vec<DirectDoodadVisit>,
    outdoor_doodad_groups: Vec<OutdoorDoodadGroup>,
    outdoor_active: usize,
    outdoor_camera: Option<(WorldSceneDepthFrame, WorldSceneFrustum)>,
}

impl WorldModelSceneGraphics {
    /// Starts a collection while retaining the preceding graphics fog bank.
    pub(super) fn begin(&mut self) {
        self.active = 0;
        self.indices.clear();
        self.direct_doodad_visits.clear();
        self.outdoor_active = 0;
        self.outdoor_camera = None;
    }

    /// 79A160/7998A0 enqueue the entered group's MODR list, independently of
    /// which recursive groups 7B3A10 emitted for later surface drawing.
    pub(super) fn record_outdoor_doodads(
        &mut self,
        root: &PlacedWorldModelCollision,
        owner: RuntimeWorldModelMovementOwner,
        group: usize,
        bin: u8,
        camera: (WorldSceneDepthFrame, WorldSceneFrustum),
    ) {
        if self.outdoor_active == self.outdoor_doodad_groups.len() {
            self.outdoor_doodad_groups.push(OutdoorDoodadGroup {
                owner,
                bin,
                references: Vec::new(),
            });
        }
        let entry = &mut self.outdoor_doodad_groups[self.outdoor_active];
        entry.owner = owner;
        entry.bin = bin;
        entry.references.clear();
        entry
            .references
            .extend_from_slice(root.model().groups()[group].doodad_references());
        self.outdoor_active += 1;
        self.outdoor_camera = Some(camera);
    }

    pub(super) fn outdoor_doodads(
        &self,
    ) -> impl Iterator<
        Item = (
            WorldSceneDepthFrame,
            WorldSceneFrustum,
            u8,
            RuntimeWorldModelMovementOwner,
            &[u16],
        ),
    > {
        self.outdoor_camera
            .into_iter()
            .flat_map(|(depth, frustum)| {
                self.outdoor_doodad_groups[..self.outdoor_active]
                    .iter()
                    .map(move |group| {
                        (
                            depth,
                            frustum,
                            group.bin,
                            group.owner,
                            group.references.as_slice(),
                        )
                    })
            })
    }

    /// Appends every 799310 clip while deduplicating only the group queue entry.
    pub(super) fn record(
        &mut self,
        root: &PlacedWorldModelCollision,
        owner: RuntimeWorldModelMovementOwner,
        visit: WorldModelSceneGroupVisit,
        models_allowed: bool,
        depth: f32,
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
                    doodads: WorldModelSceneDoodads::default(),
                });
            } else {
                let entry = &mut self.groups[index];
                entry.owner = owner;
                entry.group = visit.group;
                entry.indoor_fog = false;
                entry.frusta.clear();
            }
            let doodads = &mut self.groups[index].doodads;
            doodads.world_frusta.clear();
            doodads.references.clear();
            doodads
                .references
                .extend_from_slice(root.model().groups()[visit.group].doodad_references());
            doodads.depth = depth;
            doodads.allowed = models_allowed;
            self.indices.insert((owner, visit.group), index);
            self.active += 1;
            index
        };
        let entry = &mut self.groups[index];
        entry.indoor_fog |= self.fog;
        entry.doodads.world_frusta.push(visit.frustum);
        entry
            .frusta
            .push(visit.frustum.transformed(root.inverse_transform())?);
        Ok(())
    }

    /// Captures only the state that existed when 799F80 reached 799B70.
    pub(super) fn record_direct_doodads(
        &mut self,
        owner: RuntimeWorldModelMovementOwner,
        group: usize,
    ) {
        if let Some(&index) = self.indices.get(&(owner, group)) {
            let entry = &self.groups[index];
            self.direct_doodad_visits.push(DirectDoodadVisit {
                group: index,
                clip_count: entry.doodads.world_frusta.len(),
                indoor_fog: entry.indoor_fog,
            });
        }
    }

    /// Visits direct overlap submissions before the final ordered group drain.
    pub(super) fn visit_doodads(
        &self,
        mut visitor: impl FnMut(RuntimeWorldModelMovementOwner, &[u16], &[WorldSceneFrustum], f32, bool),
    ) {
        for direct in &self.direct_doodad_visits {
            let group = &self.groups[direct.group];
            visitor(
                group.owner,
                &group.doodads.references,
                &group.doodads.world_frusta[..direct.clip_count],
                group.doodads.depth,
                direct.indoor_fog,
            );
        }
        for group in self.groups().iter().filter(|group| group.doodads.allowed) {
            visitor(
                group.owner,
                &group.doodads.references,
                &group.doodads.world_frusta,
                group.doodads.depth,
                group.indoor_fog,
            );
        }
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

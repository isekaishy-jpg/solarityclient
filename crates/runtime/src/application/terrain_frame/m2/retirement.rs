//! Detached models keep scene state and attachments without a live object GUID.

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use glam::Mat4;
use solarity_rendering::{M2AnimationClock, M2BonePose, M2FingerPoseHands};
use solarity_systems::EntityRetirement;

use super::{
    GameObjectFrameInput, M2Frame, M2GpuPlacement, M2GpuPlacementOwner, M2PlaybackStorage,
    RuntimeTerrainFrameError, held_item_finger_pose, placement_parent_index,
};
use crate::application::unit_animation::UnitRetiredPose;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct RetiredModelKey {
    serial: u64,
    member: usize,
}

struct RetiredHierarchy {
    envelope: EntityRetirement,
    ready: Cell<bool>,
    opacity: Cell<f32>,
    transport_guid: Cell<u64>,
    relative: Cell<Option<Mat4>>,
    transform: Cell<Mat4>,
}

pub(super) struct RetiredM2Placement {
    group: Rc<RetiredHierarchy>,
    pub original_owner: M2GpuPlacementOwner,
    pub unit_pose: Option<UnitRetiredPose>,
    pub finger_hands: Option<M2FingerPoseHands>,
    parent: Option<(RetiredModelKey, u32, Mat4)>,
    attachments: Vec<u32>,
}

impl RetiredM2Placement {
    pub fn opacity(&self) -> f32 {
        self.group.opacity.get()
    }

    pub fn parent(&self) -> Option<M2GpuPlacementOwner> {
        self.parent
            .map(|(key, ..)| M2GpuPlacementOwner::Retired(key))
    }
}

#[derive(Default)]
pub(super) struct M2RetirementScene {
    next_serial: u64,
    groups: BTreeMap<u64, Rc<RetiredHierarchy>>,
    attachments: HashMap<(RetiredModelKey, u32), Option<Mat4>>,
}

impl M2Frame {
    /// Only the removed identity can transfer its hierarchy. Display/material
    /// replacement leaves its opacity owner live and follows ordinary replacement.
    pub(super) fn retire_removed_models(&mut self) {
        let mut groups = HashMap::new();
        let indices = if self.placement_topology_dirty {
            super::visibility::PlacementStateIndices::All(0..self.placements.len())
        } else {
            super::visibility::PlacementStateIndices::Cached(
                self.placement_visibility.dynamic_indices().iter().copied(),
            )
        };
        for index in indices {
            let placement = &self.placements[index];
            if let Some(opacity) = &placement.entity_opacity
                && opacity.retirement().is_some()
            {
                groups
                    .entry(Rc::as_ptr(opacity))
                    .or_insert_with(Vec::new)
                    .push(index);
            }
        }
        if groups.is_empty() {
            return;
        }
        // Preserve placement order (including the mount-main root) when more
        // than one identity disappears in the same publication.
        let mut groups = groups.into_values().collect::<Vec<_>>();
        groups.sort_by_key(|members| members[0]);
        let mut rejected = Vec::new();
        for members in groups {
            let root = members[0];
            let Some((now, initial, transport)) = self.placements[root]
                .entity_opacity
                .as_ref()
                .and_then(|owner| owner.retirement())
            else {
                continue;
            };
            if initial < 0.01
                || members
                    .iter()
                    .any(|&index| self.sources[self.placements[index].source_index].is_none())
            {
                rejected.extend(members);
                continue;
            }
            self.retirement.next_serial += 1;
            let serial = self.retirement.next_serial;
            let hierarchy = Rc::new(RetiredHierarchy {
                envelope: EntityRetirement::new(now, initial),
                ready: Cell::new(true),
                opacity: Cell::new(initial),
                transport_guid: Cell::new(transport),
                relative: Cell::new(None),
                transform: Cell::new(self.placements[root].transform),
            });
            let parents = members
                .iter()
                .map(|&index| {
                    placement_parent_index(&self.placements, index, &self.placements[index])
                        .filter(|parent| members.contains(parent))
                })
                .collect::<Vec<_>>();
            let requested_items = members
                .iter()
                .filter_map(|&index| match self.placements[index].owner {
                    M2GpuPlacementOwner::PlayerItem { guid, point } => Some((guid, point)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let mut requests: HashMap<usize, Vec<u32>> = HashMap::new();
            for (&index, parent) in members.iter().zip(&parents) {
                let placement = &mut self.placements[index];
                let owner = placement.owner;
                let parent = parent.map(|member| {
                    let (attachment, local) = match owner {
                        M2GpuPlacementOwner::PlayerItem { point, .. } => {
                            (point.id(), placement.orientation.local_transform())
                        }
                        M2GpuPlacementOwner::PlayerItemVisual { effect_point, .. } => {
                            (effect_point, Mat4::IDENTITY)
                        }
                        _ => (
                            0,
                            Mat4::from_scale(glam::Vec3::splat(placement.rider_scale)),
                        ),
                    };
                    requests.entry(member).or_default().push(attachment);
                    (RetiredModelKey { serial, member }, attachment, local)
                });
                let unit_pose = placement
                    .unit_animation
                    .take()
                    .map(|owner| owner.retirement_pose());
                if let Some(playback) = placement.playback.take() {
                    // Timers and variation/event intervals belong to CM2Model.
                    // The former gameplay owner can no longer mutate this copy.
                    placement.playback = Some(M2PlaybackStorage::Local(playback.borrow().clone()));
                }
                placement.ground_placement = None;
                placement.entity_opacity = None;
                placement.unit_presentation = None;
                placement.owner = M2GpuPlacementOwner::Retired(RetiredModelKey {
                    serial,
                    member: index,
                });
                placement.retirement = Some(Box::new(RetiredM2Placement {
                    group: Rc::clone(&hierarchy),
                    original_owner: owner,
                    unit_pose,
                    finger_hands: held_item_finger_pose(&requested_items, owner),
                    parent,
                    attachments: Vec::new(),
                }));
            }
            for (member, mut attachments) in requests {
                attachments.sort_unstable();
                attachments.dedup();
                if let Some(retired) = &mut self.placements[member].retirement {
                    retired.attachments = attachments;
                }
            }
            self.retirement.groups.insert(serial, hierarchy);
        }
        if !rejected.is_empty() {
            let mut index = 0;
            self.placements.retain(|_| {
                let keep = !rejected.contains(&index);
                index += 1;
                keep
            });
            self.compact_sources();
        }
        self.placement_topology_dirty = true;
    }

    pub(super) fn advance_retired_models(
        &mut self,
        now: u32,
        objects: Option<GameObjectFrameInput<'_>>,
    ) {
        self.retirement.attachments.clear();
        if self.retirement.groups.is_empty() {
            return;
        }
        for group in self.retirement.groups.values() {
            group.ready.set(true);
        }
        // Recursive readiness needs only the detached models, once each. A
        // retirement storm must not scan static scenery once per hierarchy.
        let indices = if self.placement_topology_dirty {
            super::visibility::PlacementStateIndices::All(0..self.placements.len())
        } else {
            self.placement_visibility.retired_indices()
        };
        for index in indices {
            let placement = &self.placements[index];
            if let Some(retired) = &placement.retirement
                && self.sources[placement.source_index].is_none()
            {
                retired.group.ready.set(false);
            }
        }
        let mut expired = Vec::new();
        for (&serial, group) in &self.retirement.groups {
            let Some(opacity) = group.envelope.sample(now, group.ready.get()) else {
                expired.push(serial);
                continue;
            };
            group.opacity.set(opacity);
            let guid = group.transport_guid.get();
            if guid != 0 {
                if let Some(parent) = objects.and_then(|objects| objects.retirement_parent(guid)) {
                    let relative = group
                        .relative
                        .get()
                        .unwrap_or_else(|| parent.inverse() * group.transform.get());
                    group.relative.set(Some(relative));
                    group.transform.set(parent * relative);
                } else {
                    // Losing the parent permanently freezes the last world pose.
                    group.transport_guid.set(0);
                }
            }
        }
        if !expired.is_empty() {
            self.placements.retain(|placement| {
                !matches!(placement.owner, M2GpuPlacementOwner::Retired(key) if expired.contains(&key.serial))
            });
            self.retirement
                .groups
                .retain(|serial, _| !expired.contains(serial));
            self.placement_topology_dirty = true;
            self.compact_sources();
        }
    }
}

impl M2RetirementScene {
    pub fn prepare_attachment(&self, placement: &mut M2GpuPlacement) -> bool {
        let Some(retired) = &placement.retirement else {
            return true;
        };
        if let Some((parent, attachment, local)) = retired.parent {
            let Some(Some(transform)) = self.attachments.get(&(parent, attachment)) else {
                return false;
            };
            placement.transform = *transform * local;
        } else {
            placement.transform = retired.group.transform.get();
        }
        true
    }

    pub fn publish_attachments(
        &mut self,
        placement: &M2GpuPlacement,
        model: &solarity_asset::DecodedM2Model,
        bones: &M2BonePose,
        clock: M2AnimationClock,
    ) -> Result<(), RuntimeTerrainFrameError> {
        if let (M2GpuPlacementOwner::Retired(key), Some(retired)) =
            (placement.owner, &placement.retirement)
        {
            for &id in &retired.attachments {
                let transform = model
                    .attachment(id)
                    .map(|attachment| {
                        bones.attachment_transform(
                            model.animations(),
                            attachment,
                            clock,
                            placement.transform,
                        )
                    })
                    .transpose()?
                    .flatten();
                self.attachments.insert((key, id), transform);
            }
        }
        Ok(())
    }
}

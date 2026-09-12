//! Transactional packet updates for retained object ownership.

use super::{
    PresentationResolver, ResolvedObjects, UiBackdropStatePlan, UiPresentationPlan,
    UiRegionGeometryPlan, UiRuntimeObjectPlan,
};

impl UiPresentationPlan {
    /// Replaces changed packet membership while retaining every unrelated resolved owner.
    /// Reindexing visits visible packet members, never the complete live object arena.
    pub(crate) fn rebuild_objects(
        &mut self,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        backdrops: &UiBackdropStatePlan,
        indices: &[usize],
    ) -> bool {
        if indices.iter().any(|index| {
            self.disabled_texture_parents
                .binary_search_by_key(index, |(owner, _)| *owner)
                .is_ok_and(|slot| {
                    self.disabled_texture_parents[slot].1 != live.objects()[*index].parent
                })
        }) {
            return false;
        }
        let resolver = PresentationResolver {
            live,
            geometry,
            backdrops,
            disabled_texture_owners: &self.disabled_texture_owners,
        };
        let mut output = ResolvedObjects::default();
        for &index in indices {
            resolver.append(index, &mut output);
        }
        for member in &self.members {
            self.member_indices_by_object[member.object_index].clear();
        }
        self.members
            .retain(|member| indices.binary_search(&member.object_index).is_err());
        self.members
            .extend(output.keyed.into_iter().map(|(_, member)| member));
        self.members
            .sort_by_key(|member| (member.key, member.object_index));
        self.packets.clear();
        for (index, member) in self.members.iter().enumerate() {
            self.member_indices_by_object[member.object_index].push(index);
            if self
                .packets
                .last()
                .is_none_or(|packet| packet.key != member.key)
            {
                self.packets.push(super::UiPresentationPacket {
                    key: member.key,
                    first_member: index,
                    member_count: 0,
                });
            }
            if let Some(packet) = self.packets.last_mut() {
                packet.member_count += 1;
            }
        }
        for model in &self.models {
            self.model_index_by_object[model.object_index] = None;
        }
        self.models
            .retain(|model| indices.binary_search(&model.object_index).is_err());
        self.models.extend(output.models);
        self.models
            .sort_by_key(|model| (model.strata, model.frame_level, model.object_index));
        for (index, model) in self.models.iter().enumerate() {
            self.model_index_by_object[model.object_index] = Some(index);
        }
        self.cleared_models
            .retain(|(index, _)| indices.binary_search(index).is_err());
        self.cleared_models.extend(output.cleared_models);
        self.cleared_models
            .sort_unstable_by_key(|(index, _)| *index);
        true
    }

    /// Resolves named owners without sorting or copying unrelated presentation packets.
    /// Source/material changes may reuse slots; a changed packet key or membership
    /// leaves publication to the topology owner before any retained member is changed.
    pub(crate) fn refresh_objects(
        &mut self,
        live: &UiRuntimeObjectPlan,
        geometry: &UiRegionGeometryPlan,
        backdrops: &UiBackdropStatePlan,
        indices: &[usize],
    ) -> bool {
        // Reparenting a native disabled texture changes its old and new owner's
        // role policy. That topology transaction belongs to the complete publisher.
        if indices.iter().any(|index| {
            self.disabled_texture_parents
                .binary_search_by_key(index, |(owner, _)| *owner)
                .is_ok_and(|slot| {
                    self.disabled_texture_parents[slot].1 != live.objects()[*index].parent
                })
        }) {
            return false;
        }
        let resolver = PresentationResolver {
            live,
            geometry,
            backdrops,
            disabled_texture_owners: &self.disabled_texture_owners,
        };
        let mut output = ResolvedObjects::default();
        for &index in indices {
            resolver.append(index, &mut output);
        }
        output
            .keyed
            .sort_by_key(|(key, member)| (member.object_index, *key));
        for &index in indices {
            let first = output
                .keyed
                .partition_point(|(_, member)| member.object_index < index);
            let end = output
                .keyed
                .partition_point(|(_, member)| member.object_index <= index);
            let existing = &self.member_indices_by_object[index];
            if existing.len() != end - first
                || existing
                    .iter()
                    .zip(&output.keyed[first..end])
                    .any(|(&slot, (key, _))| self.members[slot].key != *key)
            {
                return false;
            }
            let incoming = output
                .models
                .iter()
                .find(|model| model.object_index == index);
            match (self.model_index_by_object[index], incoming) {
                (None, None) => {}
                (Some(slot), Some(model))
                    if self.models[slot].strata == model.strata
                        && self.models[slot].frame_level == model.frame_level => {}
                _ => return false,
            }
            if self.cleared_models.iter().any(|(owner, _)| *owner == index)
                != output
                    .cleared_models
                    .iter()
                    .any(|(owner, _)| *owner == index)
            {
                return false;
            }
        }
        for &index in indices {
            let first = output
                .keyed
                .partition_point(|(_, member)| member.object_index < index);
            for (&slot, (_, member)) in self.member_indices_by_object[index]
                .iter()
                .zip(output.keyed[first..].iter())
            {
                self.members[slot] = member.clone();
            }
        }
        for model in output.models {
            if let Some(slot) = self.model_index_by_object[model.object_index] {
                self.models[slot] = model;
            }
        }
        for (index, shown) in output.cleared_models {
            if let Some((_, current)) = self
                .cleared_models
                .iter_mut()
                .find(|(owner, _)| *owner == index)
            {
                *current = shown;
            }
        }
        true
    }
}

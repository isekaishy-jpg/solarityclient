//! 799B70 admission and first-accepted fog for attached WMO models.

use glam::Vec3;

#[cfg(test)]
#[path = "../../../../tests/application/world_model_doodad_admission.rs"]
mod tests;

use super::{
    M2GpuPlacement, M2GpuPlacementOwner, M2GpuSource, distance::SceneryDistance,
    placement_bounding_sphere, visibility::M2PlacementVisibility,
};
use crate::application::terrain_coordinator::{
    RuntimeTerrainCoordinator, RuntimeWorldModelMovementOwner, m2_residency::ResidentM2Owner,
};

pub(super) fn owner_key(
    owner: M2GpuPlacementOwner,
) -> Option<(RuntimeWorldModelMovementOwner, usize)> {
    match owner {
        M2GpuPlacementOwner::Static(ResidentM2Owner::WorldModelDoodad {
            world_model_unique_id,
            doodad_index,
        }) => Some((
            RuntimeWorldModelMovementOwner::Static {
                unique_id: world_model_unique_id,
            },
            doodad_index,
        )),
        M2GpuPlacementOwner::GameObjectWorldModelDoodad {
            identity,
            doodad_index,
            ..
        } => Some((
            RuntimeWorldModelMovementOwner::GameObject { identity },
            doodad_index,
        )),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct M2DoodadScene {
    spheres: Vec<Option<(Vec3, f32, SceneryDistance)>>,
    fog_banks: Vec<Option<bool>>,
    opacities: Vec<f32>,
    outdoor_bins: Vec<Vec<usize>>,
    queued: Vec<bool>,
    retained_fog: Vec<bool>,
    prepared: Vec<bool>,
    accepted: Vec<usize>,
}

impl M2DoodadScene {
    /// Reuses compact per-placement state; only group MODR members are queried.
    pub(super) fn prepare(
        &mut self,
        terrain: &RuntimeTerrainCoordinator,
        visibility: &M2PlacementVisibility,
        placements: &mut [M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
        camera: Vec3,
        detail: f32,
    ) -> Result<(), crate::application::terrain_coordinator::RuntimeMovementRegistrationError> {
        let mut profile =
            crate::application::frame_profile::RuntimeFrameProfile::new("M2 doodad admission");
        // Large model inputs are initialized lazily by prepare_model. The
        // compact per-frame flags guard every consumer of retained entries.
        self.spheres.resize(placements.len(), None);
        self.fog_banks.clear();
        self.fog_banks.resize(placements.len(), None);
        self.opacities.resize(placements.len(), 1.0);
        self.queued.clear();
        self.queued.resize(placements.len(), false);
        self.retained_fog.resize(placements.len(), false);
        self.prepared.clear();
        self.prepared.resize(placements.len(), false);
        self.accepted.clear();
        self.outdoor_bins.resize_with(64, Vec::new);
        for bin in &mut self.outdoor_bins {
            bin.clear();
        }
        profile.mark("scratch reset");
        // Hidden light owners still advance and publish with their distance
        // opacity. Other models need admission inputs only when a group visits them.
        for &index in visibility.world_model_doodad_light_indices() {
            self.prepare_model(index, visibility, placements, sources, camera, detail);
        }
        profile.mark("light owners");
        let mut drained = 0;
        let mut outdoor_clip = None;
        for (depth, clip, group_bin, owner, references) in terrain.world_model_outdoor_doodads() {
            while drained < usize::from(group_bin) {
                self.submit_outdoor_bin(drained, clip, detail);
                drained += 1;
            }
            outdoor_clip = Some(clip);
            for &reference in references {
                let Some(&index) = visibility
                    .world_model_doodads()
                    .get(&(owner, usize::from(reference)))
                else {
                    continue;
                };
                if self.queued[index] || self.fog_banks[index].is_some() {
                    continue;
                }
                self.prepare_model(index, visibility, placements, sources, camera, detail);
                let Some((center, radius, _)) = self.spheres[index] else {
                    continue;
                };
                if let Some(bin) = depth.doodad_depth_bin(center, radius, group_bin)? {
                    self.outdoor_bins[usize::from(bin)].push(index);
                    self.queued[index] = true;
                }
            }
        }
        if let Some(clip) = outdoor_clip {
            while drained < 64 {
                self.submit_outdoor_bin(drained, clip, detail);
                drained += 1;
            }
        }
        profile.mark("outdoor groups");
        terrain.visit_world_model_doodads(|owner, references, clips, depth, indoor_fog| {
            for &reference in references {
                let Some(&index) = visibility
                    .world_model_doodads()
                    .get(&(owner, usize::from(reference)))
                else {
                    continue;
                };
                self.prepare_model(index, visibility, placements, sources, camera, detail);
                self.admit(index, clips, depth, detail, indoor_fog);
            }
        });
        profile.mark("indoor groups");
        for &index in &self.accepted {
            if let Some(bank) = self.fog_banks[index] {
                placements[index].scene_indoor_fog = bank;
            }
        }
        profile.mark("fog publication");
        Ok(())
    }

    /// Resolves each visited model once, after current moving-parent transforms.
    fn prepare_model(
        &mut self,
        index: usize,
        visibility: &M2PlacementVisibility,
        placements: &[M2GpuPlacement],
        sources: &[Option<M2GpuSource>],
        camera: Vec3,
        detail: f32,
    ) {
        if self.prepared[index] {
            return;
        }
        self.prepared[index] = true;
        self.spheres[index] = None;
        self.opacities[index] = 1.0;
        let placement = &placements[index];
        self.retained_fog[index] = placement.scene_indoor_fog;
        if !placement.placement_valid {
            return;
        }
        let Some(source) = sources[placement.source_index].as_ref() else {
            return;
        };
        let (center, radius) = visibility.bounds()[index]
            .unwrap_or_else(|| placement_bounding_sphere(&source.model, placement.transform));
        let scenery = visibility.scenery(index).unwrap_or_else(|| {
            let bounds = source.model.bounds();
            SceneryDistance::new(bounds.minimum(), bounds.maximum(), placement.transform)
        });
        self.spheres[index] = Some((center, radius, scenery));
        self.opacities[index] = scenery.opacity(camera, detail);
    }

    /// 7987A0 tests the current outdoor clip and preserves the model's fog bit.
    fn submit_outdoor_bin(
        &mut self,
        bin: usize,
        clip: solarity_systems::WorldSceneFrustum,
        detail: f32,
    ) {
        let class_depth = bin as f32 * f32::from_bits(0x4205_5555);
        for &index in &self.outdoor_bins[bin] {
            self.queued[index] = false;
            let Some((center, radius, scenery)) = self.spheres[index] else {
                continue;
            };
            if scenery.admits_group(class_depth, detail) && clip.intersects_sphere(center, radius) {
                self.fog_banks[index] = Some(self.retained_fog[index]);
                self.accepted.push(index);
            }
        }
    }

    fn admit(
        &mut self,
        index: usize,
        clips: &[solarity_systems::WorldSceneFrustum],
        depth: f32,
        detail: f32,
        indoor_fog: bool,
    ) {
        if self.fog_banks[index].is_some() {
            return;
        }
        let Some((center, radius, scenery)) = self.spheres[index] else {
            return;
        };
        if scenery.admits_group(depth, detail)
            && clips
                .iter()
                .any(|clip| clip.intersects_sphere(center, radius))
        {
            // 799B70 commits the bank before 791CB0 can fade to zero.
            self.fog_banks[index] = Some(indoor_fog);
            self.accepted.push(index);
        }
    }

    pub(super) fn fog_bank(&self, index: usize) -> Option<bool> {
        self.fog_banks.get(index).copied().flatten()
    }

    pub(super) fn opacity(&self, index: usize) -> f32 {
        if self.prepared.get(index).copied().unwrap_or(false) {
            self.opacities[index]
        } else {
            1.0
        }
    }
}

//! Ordered scene ownership and worker-ready receiver evaluation.

mod work;
use work::LightingBatch;

use glam::Vec3;
use solarity_rendering::{M2DirectionalLight, M2PointLight, M2SceneUniform, ScenePointLights};

use super::RuntimeTerrainFrameError;
use std::rc::{Rc, Weak};

struct RetainedDirectional {
    owner: Weak<()>,
    slot: usize,
    light: M2DirectionalLight,
    active: bool,
}

#[derive(Default)]
pub(super) struct SceneLighting {
    batch: LightingBatch,
    pub points: ScenePointLights,
    pub scenes: Vec<M2SceneUniform>,
    pub sample_directional: Vec<(usize, M2DirectionalLight)>,
    pub sample_points: Vec<M2PointLight>,
    directional: Vec<M2DirectionalLight>,
    retained_directionals: Vec<RetainedDirectional>,
    centers: Vec<Vec3>,
    placement_centers: Vec<Option<Vec3>>,
    receiver_lights: Vec<Option<M2DirectionalLight>>,
    receiver_fog: Vec<Option<Vec3>>,
    placement_lights: Vec<Option<M2DirectionalLight>>,
    placement_parents: Vec<Option<usize>>,
    receiver_placements: Vec<usize>,
    /// Current frame's indexed extent, independent of retained storage capacity.
    placement_end: usize,
}

impl SceneLighting {
    pub fn directionals(&self) -> &[M2DirectionalLight] {
        &self.directional[..self.directional.len().saturating_sub(1)]
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.scenes.clear();
        self.directional.clear();
        for light in &mut self.retained_directionals {
            light.active = false;
        }
        self.centers.clear();
        // Placement indices include distant resident terrain. Reset only the
        // receivers actually published last frame; preserving vector lengths
        // avoids filling every intervening scenery slot at the next receiver.
        // Clear all three facts together so removed or remapped parents cannot
        // lend a stale center, light, or ancestry edge to the new frame.
        for &placement in &self.receiver_placements {
            self.placement_centers[placement] = None;
            self.placement_lights[placement] = None;
            self.placement_parents[placement] = None;
        }
        self.receiver_lights.clear();
        self.receiver_fog.clear();
        self.receiver_placements.clear();
        self.placement_end = 0;
    }

    pub fn publish(
        &mut self,
        owner: &Rc<()>,
        opacity: f32,
    ) -> Result<(), RuntimeTerrainFrameError> {
        for &(slot, light) in &self.sample_directional {
            let light = M2DirectionalLight::new(
                light.direction(),
                light.ambient() * opacity,
                light.diffuse() * opacity,
            );
            if let Some(retained) = self
                .retained_directionals
                .iter_mut()
                .find(|entry| entry.owner.as_ptr() == Rc::as_ptr(owner) && entry.slot == slot)
            {
                retained.light = light;
                retained.active = true;
            } else {
                self.retained_directionals.push(RetainedDirectional {
                    owner: Rc::downgrade(owner),
                    slot,
                    light,
                    active: true,
                });
            }
        }
        for point in &self.sample_points {
            self.points.publish(M2PointLight::new(
                point.position(),
                point.ambient() * opacity,
                point.diffuse() * opacity,
            ))?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn receiver(
        &mut self,
        placement_index: usize,
        parent: Option<usize>,
        center: Vec3,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        self.receiver_with_light(placement_index, parent, center, None, None)
    }

    pub fn receiver_with_light(
        &mut self,
        placement_index: usize,
        parent: Option<usize>,
        center: Vec3,
        light: Option<M2DirectionalLight>,
        fog_color: Option<Vec3>,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        let size = self.placement_centers.len().max(placement_index + 1);
        self.placement_end = self.placement_end.max(placement_index + 1);
        self.placement_centers.resize(size, None);
        self.placement_centers[placement_index] = Some(center);
        self.placement_lights.resize(size, None);
        self.placement_lights[placement_index] = light;
        self.placement_parents.resize(size, None);
        self.placement_parents[placement_index] = parent;
        self.receiver_placements.push(placement_index);
        self.receiver_lights.push(light);
        self.receiver_fog.push(fog_color);
        let index = u32::try_from(self.centers.len())
            .map_err(|_| solarity_rendering::VulkanError::WorldFrameCapacity)?;
        self.centers.push(center);
        Ok(index)
    }

    /// Preserves linked-list source order on the existing Rc owner thread.
    fn prepare_directionals(&mut self, exterior: M2DirectionalLight) {
        // Scene linked lists precede the entity callback's exterior contribution.
        self.retained_directionals.retain(|light| light.active);
        self.directional.extend(
            self.retained_directionals
                .iter()
                .rev()
                .map(|entry| entry.light),
        );
        self.directional.push(exterior);
    }
}

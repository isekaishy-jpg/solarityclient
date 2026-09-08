//! Current scene publication, then receiver queries after every source update.

use glam::Vec3;
use solarity_rendering::{
    M2DirectionalLight, M2LocalLightState, M2PointLight, M2SceneUniform, ScenePointLights,
    merge_wotlk_directional_lights,
};

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
    pub points: ScenePointLights,
    pub scenes: Vec<M2SceneUniform>,
    pub sample_directional: Vec<(usize, M2DirectionalLight)>,
    pub sample_points: Vec<M2PointLight>,
    directional: Vec<M2DirectionalLight>,
    retained_directionals: Vec<RetainedDirectional>,
    centers: Vec<Vec3>,
    placement_centers: Vec<Option<Vec3>>,
    receiver_lights: Vec<Option<M2DirectionalLight>>,
    placement_lights: Vec<Option<M2DirectionalLight>>,
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
        self.placement_centers.clear();
        self.receiver_lights.clear();
        self.placement_lights.clear();
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
        self.receiver_with_light(placement_index, parent, center, None)
    }

    pub fn receiver_with_light(
        &mut self,
        placement_index: usize,
        parent: Option<usize>,
        center: Vec3,
        light: Option<M2DirectionalLight>,
    ) -> Result<u32, RuntimeTerrainFrameError> {
        let center = parent
            .and_then(|index| self.placement_centers.get(index).copied().flatten())
            .unwrap_or(center);
        self.placement_centers.resize(placement_index + 1, None);
        self.placement_centers[placement_index] = Some(center);
        let light = parent
            .and_then(|index| self.placement_lights.get(index).copied().flatten())
            .or(light);
        self.placement_lights.resize(placement_index + 1, None);
        self.placement_lights[placement_index] = light;
        self.receiver_lights.push(light);
        let index = u32::try_from(self.centers.len())
            .map_err(|_| solarity_rendering::VulkanError::WorldFrameCapacity)?;
        self.centers.push(center);
        Ok(index)
    }

    pub fn finish(
        &mut self,
        base: M2SceneUniform,
        exterior: M2DirectionalLight,
    ) -> Result<(), RuntimeTerrainFrameError> {
        // Scene linked lists precede the entity callback's exterior contribution.
        self.retained_directionals.retain(|light| light.active);
        self.directional.extend(
            self.retained_directionals
                .iter()
                .rev()
                .map(|entry| entry.light),
        );
        self.directional.push(exterior);
        for (center, light) in self.centers.iter().zip(&self.receiver_lights) {
            if let Some(last) = self.directional.last_mut() {
                *last = light.unwrap_or(exterior);
            }
            let sunlight = merge_wotlk_directional_lights(&self.directional);
            let mut lights = [M2LocalLightState::disabled(); 4];
            if let Some(sunlight) = sunlight {
                lights[0] = sunlight.local_light_state();
            }
            for (slot, index) in self
                .points
                .query(*center, 0.0)?
                .indices()
                .into_iter()
                .flatten()
                .take(3)
                .enumerate()
            {
                lights[slot + 1] = self.points.points()[index].local_light_state();
            }
            self.scenes.push(base.with_local_lights(lights));
        }
        Ok(())
    }
}

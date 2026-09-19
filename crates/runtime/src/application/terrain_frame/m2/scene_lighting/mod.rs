//! Ordered scene ownership and worker-ready receiver evaluation.

mod batch;
mod receivers;
mod sources;
mod work;
use batch::LightingBatch;
use receivers::ReceiverInputs;
pub(in crate::application::terrain_frame) use sources::SceneLightInputs;
use sources::SceneLightSources;

use glam::Vec3;
use solarity_rendering::{M2DirectionalLight, M2PointLight, M2SceneUniform, ScenePointLights};

use super::RuntimeTerrainFrameError;
use std::rc::{Rc, Weak};
use std::sync::Arc;

struct RetainedDirectional {
    owner: Weak<()>,
    slot: usize,
    light: M2DirectionalLight,
    active: bool,
}

#[derive(Default)]
pub(super) struct SceneLighting {
    batch: LightingBatch,
    sources: Arc<SceneLightSources>,
    pub scenes: Vec<M2SceneUniform>,
    pub sample_directional: Vec<(usize, M2DirectionalLight)>,
    pub sample_points: Vec<M2PointLight>,
    retained_directionals: Vec<RetainedDirectional>,
    receivers: Arc<ReceiverInputs>,
}

impl SceneLighting {
    pub fn points(&self) -> &ScenePointLights {
        &self.sources.points
    }

    pub fn directionals(&self) -> &[M2DirectionalLight] {
        &self.sources.directionals
    }

    /// Publication has ended before runtime exposes these inputs to main consumers.
    pub(in crate::application::terrain_frame::m2) fn inputs(&self) -> SceneLightInputs<'_> {
        SceneLightInputs {
            trace: self.sources.trace,
            points: self.points(),
            directionals: self.directionals(),
        }
    }

    pub fn clear(&mut self) {
        let sources = SceneLightSources::exclusive(&mut self.sources);
        sources.points.clear();
        sources.directionals.clear();
        self.scenes.clear();
        for light in &mut self.retained_directionals {
            light.active = false;
        }
        ReceiverInputs::exclusive(&mut self.receivers).clear();
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
        let sources = SceneLightSources::exclusive(&mut self.sources);
        for point in &self.sample_points {
            sources.points.publish(M2PointLight::new(
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
        ReceiverInputs::exclusive(&mut self.receivers).push(
            placement_index,
            parent,
            center,
            light,
            fog_color,
        )
    }

    /// Preserves linked-list source order on the existing Rc owner thread.
    fn prepare_directionals(&mut self) {
        // Scene linked lists precede the entity callback's exterior contribution.
        self.retained_directionals.retain(|light| light.active);
        let sources = SceneLightSources::exclusive(&mut self.sources);
        sources.trace = solarity_profiling::TraceContext::capture().fork("m2.light_sources");
        sources.directionals.extend(
            self.retained_directionals
                .iter()
                .rev()
                .map(|entry| entry.light),
        );
    }
}

//! Unit effect state keeps live callbacks separate from asynchronous source ownership.

mod events;
mod sources;
mod world;

use super::unit_water::UnitWaterSample;
use glam::Vec3;
use solarity_asset::{ArchiveCatalog, DecodedM2Model, ResourceWeak};
use solarity_ecs::WorldObjectIdentity;
use solarity_systems::UnitBreathState;
use sources::Sources;
use std::{collections::HashMap, rc::Rc, sync::Arc};

/// Live environmental state follows a unit lifetime rather than a decoded model cache entry.
#[derive(Default)]
struct UnitState {
    lifetime: Rc<()>,
    model: ResourceWeak<DecodedM2Model>,
    sample: Option<UnitWaterSample>,
    position: Vec3,
    surface: Option<f32>,
    breath: UnitBreathState,
    tint: solarity_systems::UnitModelTint,
}

/// Main owns gameplay callbacks; a bounded CPU operation owns source preparation.
pub(super) struct RuntimeUnitEffects {
    sources: Option<Sources>,
    environmental: Arc<solarity_asset::EnvironmentalDamageCatalog>,
    world: Option<WorldObjectIdentity>,
    units: HashMap<WorldObjectIdentity, UnitState>,
}

impl RuntimeUnitEffects {
    /// Defer process-retained effect sources until the executor can admit their work.
    pub(super) fn new(
        catalog: ArchiveCatalog,
        environmental: Arc<solarity_asset::EnvironmentalDamageCatalog>,
    ) -> Self {
        Self {
            sources: Some(Sources::Deferred(catalog)),
            environmental,
            world: None,
            units: HashMap::new(),
        }
    }
}

#[cfg(test)]
pub(in crate::application) use sources::prepare_sources_for_test;

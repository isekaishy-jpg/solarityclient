//! Weak derived plans follow exact decoded WMO generations.
use super::super::RuntimeTerrainError;
use solarity_asset::{
    AssetPath, AssetStore, DecodedWorldModel, ResourceLease, ResourceWeak, WmoModelCache,
};
use solarity_rendering::WorldModelMeshPlan;
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

/// Worker-owned decoded generations and weak references to their prepared mesh.
/// Resident CPU/GPU sources share the plan without extending retired lifetimes.
#[derive(Default)]
pub(in crate::application) struct ResidentWorldModelCache {
    models: WmoModelCache,
    plans: HashMap<AssetPath, (ResourceWeak<DecodedWorldModel>, Weak<WorldModelMeshPlan>)>,
}

impl ResidentWorldModelCache {
    pub(in crate::application) fn new() -> Self {
        Self::default()
    }

    pub(super) fn load(
        &mut self,
        store: &mut AssetStore,
        path: &AssetPath,
    ) -> Result<(ResourceLease<DecodedWorldModel>, Arc<WorldModelMeshPlan>), RuntimeTerrainError>
    {
        let model = self.models.load(store, path)?;
        self.prepare_model(model)
    }

    /// Derived geometry follows the shared decoded generation without taking another source lease.
    pub(super) fn prepare_model(
        &mut self,
        model: ResourceLease<DecodedWorldModel>,
    ) -> Result<(ResourceLease<DecodedWorldModel>, Arc<WorldModelMeshPlan>), RuntimeTerrainError>
    {
        let path = model.path();
        let generation = ResourceLease::downgrade(&model);
        let retained = self.plans.get(path).and_then(|(previous, plan)| {
            previous
                .ptr_eq(&generation)
                .then(|| plan.upgrade())
                .flatten()
        });
        let plan = if let Some(plan) = retained {
            plan
        } else {
            let plan = Arc::new(WorldModelMeshPlan::prepare(&model)?);
            self.plans
                .insert(path.clone(), (generation, Arc::downgrade(&plan)));
            plan
        };
        Ok((model, plan))
    }

    pub(in crate::application) fn collect_unused(&mut self) -> usize {
        let removed = self.models.collect_unused();
        self.plans
            .retain(|_, (model, plan)| model.is_alive() && plan.strong_count() > 0);
        removed
    }
}

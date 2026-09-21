//! Ordered effect requests join shared model readiness before consumer preparation.

use super::{PreparedUnitEffects, ResidentUnitEffect, UnitEffectResource, WATER_EFFECTS};
use crate::application::RuntimeTerrainError;
use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::application::terrain_coordinator::m2_residency::ResidentM2Source;
use solarity_asset::{
    AssetError, AssetPath, AssetStore, BlpTextureCache, DecodedM2Model, EnvironmentalDamageCatalog,
    M2LoadDependency, M2LoadError, ResourceLease, SpellVisualEffectCatalog,
    SpellVisualEffectDefinition,
};
use solarity_cpu::{
    CpuBuffer, CpuStorageBudget, CpuStorageClass, CpuStorageKind, CpuTaskDependency,
};
use std::{collections::BTreeSet, ops::ControlFlow};

/// One admitted operation owns every partial input until the complete bank publishes.
pub(in crate::application) struct UnitEffectPreparation {
    definitions: CpuBuffer<(UnitEffectResource, SpellVisualEffectDefinition)>,
    next: usize,
    pending: Option<M2LoadDependency>,
    model: Option<ResourceLease<DecodedM2Model>>,
    textures: BlpTextureCache,
    effects: PreparedUnitEffects,
}

impl UnitEffectPreparation {
    /// Preserve named-water order followed by the sorted unique environmental IDs.
    /// Model-path validation still occurs when that definition's turn is reached.
    pub(in crate::application) fn begin(
        store: &mut AssetStore,
        environmental: &EnvironmentalDamageCatalog,
        budget: &CpuStorageBudget,
    ) -> Result<Self, RuntimeTerrainError> {
        let catalog = SpellVisualEffectCatalog::load(store)?;
        let visuals = environmental
            .visual_kits()
            .flat_map(|kit| kit.effects().map(|(_, id)| id))
            .collect::<BTreeSet<_>>();
        let count = WATER_EFFECTS
            .len()
            .checked_add(visuals.len())
            .ok_or(solarity_cpu::CpuError::StorageSizeOverflow)?;
        let mut definitions = CpuBuffer::default();
        definitions.reserve(
            budget,
            CpuStorageClass::Required,
            CpuStorageKind::Metadata,
            count,
        )?;
        for kind in WATER_EFFECTS
            .into_iter()
            .map(UnitEffectResource::Water)
            .chain(visuals.into_iter().map(UnitEffectResource::Visual))
        {
            let definition = match kind {
                UnitEffectResource::Water(water) => catalog.named(water.name()),
                UnitEffectResource::Visual(id) => catalog.definition(id),
            };
            if let Some(definition) = definition {
                definitions.push((kind, definition.clone()))?;
            }
        }
        let mut effects = PreparedUnitEffects::default();
        effects.reserve(
            budget,
            CpuStorageClass::Required,
            CpuStorageKind::Result,
            definitions.len(),
        )?;
        Ok(Self {
            definitions,
            next: 0,
            pending: None,
            model: None,
            textures: BlpTextureCache::new(),
            effects,
        })
    }

    /// A turn resolves one model or prepares one consumer's textures/programs.
    /// Another producer suspends this operation without retaining its worker.
    pub(in crate::application) fn step(
        &mut self,
        store: &mut AssetStore,
        shared: &SharedTerrainSources,
    ) -> Result<ControlFlow<PreparedUnitEffects, Option<CpuTaskDependency>>, RuntimeTerrainError>
    {
        let Some((kind, definition)) = self.definitions.get(self.next) else {
            return Ok(ControlFlow::Break(std::mem::take(&mut self.effects)));
        };
        let Some(path) = definition.model_path()? else {
            self.next += 1;
            return Ok(ControlFlow::Continue(None));
        };
        if let Some(model) = self.model.take() {
            match ResidentM2Source::from_model_with_lights(
                model,
                &mut self.textures,
                store,
                solarity_rendering::M2LocalLightCount::Four,
            ) {
                Ok(source) => self.effects.push(Some(ResidentUnitEffect {
                    kind: *kind,
                    definition: definition.clone(),
                    source,
                }))?,
                Err(error) if !pipeline_failure(&error) => failed(definition, &path, &error),
                Err(error) => return Err(error),
            }
            self.next += 1;
            return Ok(ControlFlow::Continue(None));
        }
        match shared.model(&path, &mut self.pending, store) {
            Ok(ControlFlow::Break(model)) => self.model = Some(model),
            Ok(ControlFlow::Continue(edge)) => return Ok(ControlFlow::Continue(Some(edge))),
            Err(
                error @ (RuntimeTerrainError::Asset(_)
                | RuntimeTerrainError::SharedModel(M2LoadError::Asset(_))),
            ) if !pipeline_failure(&error) => {
                // Retain the existing per-effect source-failure policy. Executor
                // and producer-lifetime failures are pipeline errors, not missing models.
                failed(definition, &path, &error);
                self.next += 1;
            }
            Err(error) => return Err(error),
        }
        Ok(ControlFlow::Continue(None))
    }
}

/// Admission and producer ownership failures cannot be treated as missing authored data.
fn pipeline_failure(error: &RuntimeTerrainError) -> bool {
    let asset = match error {
        RuntimeTerrainError::Asset(error) => error,
        RuntimeTerrainError::SharedModel(M2LoadError::Asset(error)) => error.as_ref(),
        RuntimeTerrainError::Cpu(_) | RuntimeTerrainError::SharedModel(_) => return true,
        _ => return false,
    };
    matches!(
        asset,
        AssetError::ReadAdmission { .. }
            | AssetError::SourceStorage(_)
            | AssetError::SourceStorageConfigured
    )
}

/// The layer that omits a failed authored effect owns its existing diagnostic.
fn failed(definition: &SpellVisualEffectDefinition, path: &AssetPath, error: &RuntimeTerrainError) {
    tracing::warn!(effect = definition.id(), %path, %error, "unit effect model request failed");
}

#[cfg(test)]
#[path = "../../../../../tests/application/unit_effect_loading.rs"]
mod tests;

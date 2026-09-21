//! Authored effect order and source failure policy stay independent of worker scheduling.

use crate::application::RuntimeTerrainError;
use crate::application::terrain_coordinator::SharedTerrainSources;
use crate::application::terrain_frame::m2::unit_effects::{
    PreparedUnitEffects, ResidentUnitEffect, UnitEffectResource, WATER_EFFECTS,
};
use solarity_asset::{
    AssetPath, AssetStore, BlpTextureCache, EnvironmentalDamageCatalog, M2LoadDependency,
    SpellVisualEffectCatalog, SpellVisualEffectDefinition,
};
use solarity_cpu::{
    CpuBuffer, CpuStorageBudget, CpuStorageClass, CpuStorageKind, CpuTaskDependency,
};
use std::{collections::BTreeSet, ops::ControlFlow};

/// Declaration lookup is side-effect free; path validation retains its original per-effect order.
struct Entry {
    kind: UnitEffectResource,
    definition: SpellVisualEffectDefinition,
}

/// Source pins and charged records survive worker suspension and ordered output transfer.
pub(super) struct Preparation {
    entries: CpuBuffer<Entry>,
    next: usize,
    path: Option<AssetPath>,
    pending: Option<M2LoadDependency>,
    textures: BlpTextureCache,
    effects: PreparedUnitEffects,
}

impl Preparation {
    /// Resolves the existing five water names followed by ascending unique environmental visual IDs.
    pub(super) fn new(
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
        let mut entries = CpuBuffer::default();
        entries.reserve(
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
                entries.push(Entry {
                    kind,
                    definition: definition.clone(),
                })?;
            }
        }
        let mut effects = PreparedUnitEffects::default();
        effects.reserve(
            budget,
            CpuStorageClass::Required,
            CpuStorageKind::Result,
            entries.len(),
        )?;
        Ok(Self {
            entries,
            next: 0,
            path: None,
            pending: None,
            textures: BlpTextureCache::new(),
            effects,
        })
    }

    /// A joined producer supplies readiness. Only one effect builds derived sources per turn.
    pub(super) fn step(
        &mut self,
        store: &mut AssetStore,
        shared: &SharedTerrainSources,
    ) -> Result<ControlFlow<PreparedUnitEffects, Option<CpuTaskDependency>>, RuntimeTerrainError>
    {
        let Some(entry) = self.entries.get(self.next) else {
            return Ok(ControlFlow::Break(std::mem::take(&mut self.effects)));
        };
        if self.path.is_none() {
            self.path = entry.definition.model_path()?;
            if self.path.is_none() {
                self.next += 1;
                return Ok(ControlFlow::Continue(None));
            }
        }
        let path = self
            .path
            .as_ref()
            .unwrap_or_else(|| unreachable!("effect path validated before source request"));
        let result = match shared.model(path, &mut self.pending, store) {
            Ok(ControlFlow::Continue(edge)) => return Ok(ControlFlow::Continue(Some(edge))),
            Ok(ControlFlow::Break(model)) => ResidentUnitEffect::from_model(
                entry.kind,
                entry.definition.clone(),
                model,
                &mut self.textures,
                store,
            ),
            Err(error) => Err(error),
        };
        match result {
            Ok(source) => self.effects.push(Some(source))?,
            Err(error) => {
                tracing::warn!(effect = entry.definition.id(), %path, %error, "unit effect model request failed")
            }
        }
        self.path = None;
        self.next += 1;
        Ok(ControlFlow::Continue(None))
    }
}

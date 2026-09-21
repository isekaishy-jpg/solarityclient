//! The frozen synchronous effect path is the reference for shared loading.

use super::*;
use solarity_asset::{M2ModelCache, SpellVisualEffectCatalog};

impl ResidentUnitEffect {
    /// Frozen synchronous reference for existing scene fixtures and cursor parity.
    pub(in crate::application) fn load(
        store: &mut AssetStore,
        environmental: &EnvironmentalDamageCatalog,
    ) -> Result<Vec<Self>, RuntimeTerrainError> {
        let catalog = SpellVisualEffectCatalog::load(store)?;
        let mut models = M2ModelCache::new();
        let mut textures = BlpTextureCache::new();
        let mut effects = Vec::with_capacity(WATER_EFFECTS.len());
        let visuals = environmental
            .visual_kits()
            .flat_map(|kit| kit.effects().map(|(_, id)| id))
            .collect::<BTreeSet<_>>();
        for kind in WATER_EFFECTS
            .into_iter()
            .map(UnitEffectResource::Water)
            .chain(visuals.into_iter().map(UnitEffectResource::Visual))
        {
            let Some(definition) = (match kind {
                UnitEffectResource::Water(water) => catalog.named(water.name()),
                UnitEffectResource::Visual(id) => catalog.definition(id),
            }) else {
                continue;
            };
            let Some(path) = definition.model_path()? else {
                continue;
            };
            match ResidentM2Source::load(&path, &mut models, &mut textures, store) {
                Ok(source) => effects.push(Self {
                    kind,
                    definition: definition.clone(),
                    source,
                }),
                Err(error) => {
                    tracing::warn!(effect = definition.id(), %path, %error, "unit effect model request failed");
                }
            }
        }
        Ok(effects)
    }
}

/// A joined effect source releases the only worker and never publishes a partial
/// bank. The resulting five owners reuse the producer's exact model generation.
#[test]
fn shared_effect_source_yields_then_publishes_the_ordered_complete_bank()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::test_support::unit_models;
    use solarity_asset::{ArchiveCatalog, AssetResourceKey, ClientDataRoot, Locale, M2Load};
    use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, CpuTaskStep};
    use std::{num::NonZeroUsize, sync::mpsc, time::Duration};

    let fixture = unit_models::fixture_with_water_effects(17)?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let path = AssetPath::new("World/WaterEffect.m2")?;
    let M2Load::Producer(producer) = catalog
        .model_cache_service()
        .request(&AssetResourceKey::new(catalog.namespace(), path))?
    else {
        return Err("shared effect producer".into());
    };
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut cursor = UnitEffectPreparation::begin(
        &mut store,
        &EnvironmentalDamageCatalog::default(),
        &shared.budget,
    )?;
    let (notice, observed) = mpsc::channel();
    let task =
        permit.submit_resumable_with_context(move |_| match cursor.step(&mut store, &shared) {
            Ok(ControlFlow::Continue(Some(edge))) => {
                assert_eq!(cursor.next, 0);
                assert!(cursor.effects.is_empty());
                assert!(cursor.model.is_none());
                let _ = notice.send(());
                CpuTaskStep::Wait(edge)
            }
            Ok(ControlFlow::Continue(None)) => CpuTaskStep::Continue,
            Ok(ControlFlow::Break(effects)) => CpuTaskStep::Complete(Ok(effects)),
            Err(error) => CpuTaskStep::Complete(Err(error)),
        });
    observed.recv_timeout(Duration::from_secs(5))?;
    assert_eq!(cpu.try_submit(|| 41)?.join()?, 41);
    let mut reader = AssetStore::mount(catalog.clone())?;
    let model = cpu
        .try_submit(move || producer.load(&mut reader))?
        .join()??;
    let effects = task.join()??;
    assert_eq!(effects.len(), WATER_EFFECTS.len());
    assert!(effects.iter().all(Option::is_some));
    for (effect, kind) in effects.iter().flatten().zip(WATER_EFFECTS) {
        assert_eq!(effect.kind, UnitEffectResource::Water(kind));
        assert!(ResourceLease::ptr_eq(effect.source.model(), &model));
    }
    let mut reader = AssetStore::mount(catalog)?;
    let serial = ResidentUnitEffect::load(&mut reader, &EnvironmentalDamageCatalog::default())?;
    for (effect, reference) in effects.iter().flatten().zip(&serial) {
        assert_eq!(effect.kind, reference.kind);
        assert_eq!(effect.definition.id(), reference.definition.id());
        assert_eq!(
            effect.source.model().path(),
            reference.source.model().path()
        );
        assert_eq!(
            effect.source.textures().len(),
            reference.source.textures().len()
        );
    }
    cpu.shutdown()?;
    Ok(())
}

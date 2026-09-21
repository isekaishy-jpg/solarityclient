//! Sky source suspension, bounded admission, and retained owner policy.

use super::*;
use crate::test_support::{ClientFixture, game_object_models};
use solarity_asset::{AssetResourceKey, AssetStore, ClientDataRoot, Locale, M2Load};
use solarity_cpu::{CpuExecutionPlan, CpuPoolConfig, CpuStoragePlan};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::mpsc,
    time::{Duration, Instant},
};

impl RuntimeSkyResources {
    /// Visual comparisons await source readiness without advancing animation/RNG.
    pub(in crate::application::sky_resources) fn settle_sources(
        &mut self,
        cpu: &CpuExecutor,
        input: Option<(SkyModelInput<'_>, u32)>,
    ) -> Result<(), Box<dyn Error>> {
        if let Some((input, time)) = input {
            self.resolve_slots(input, time)?;
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.service_textures(cpu)?;
            self.service_models(cpu)?;
            if matches!(self.celestial_request, CelestialRequest::Complete)
                && self.stars_request.complete
                && self.skyboxes.iter().all(|entry| entry.request.complete)
            {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err("sky source completion timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[test]
fn sky_source_suspends_single_worker_and_preserves_shared_identity() -> Result<(), Box<dyn Error>> {
    let model = game_object_models::model()?;
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[("Sky.m2", &model), ("Sky00.skin", &skin)])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let key = AssetResourceKey::new(catalog.namespace(), AssetPath::new("Sky.m2")?);
    let M2Load::Producer(producer) = catalog.model_cache_service().request(&key)? else {
        return Err("expected producer".into());
    };
    let cpu = CpuExecutor::new(CpuPoolConfig::new(
        CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(4).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 0),
    ))?;
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let (suspended, wait) = mpsc::channel();
    let mut operation = model_steps(catalog.clone(), AssetPath::new("sky.mdx")?, true, shared);
    let task = permit.submit_resumable_with_context(move |context| {
        let result = operation(context);
        if matches!(result, CpuTaskStep::Wait(_)) {
            let _ = suspended.send(());
        }
        result
    });
    wait.recv_timeout(Duration::from_secs(5))?;
    assert!(!task.is_finished());
    assert_eq!(cpu.try_reserve()?.submit(|| 42).join()?, 42);
    task.cancel();
    assert!(matches!(
        task.join()?,
        Err(RuntimeTerrainError::Cpu(CpuError::JobCancelled))
    ));
    let permit = cpu.try_reserve()?;
    let shared = SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let task = permit.submit_resumable_with_context(model_steps(
        catalog.clone(),
        AssetPath::new("sky.mdx")?,
        true,
        shared,
    ));
    let expected = producer.load(&mut AssetStore::mount(catalog.clone())?)?;
    let (resident, lights) = task.join()??;
    assert!(solarity_asset::ResourceLease::ptr_eq(
        resident.model(),
        &expected
    ));
    assert_eq!(lights, M2LocalLightCount::Zero);
    let mut request = ModelRequest::new(AssetPath::new("SKY.M2")?, 1234, true);
    let mut permits = Vec::new();
    while let Ok(permit) = cpu.try_reserve() {
        permits.push(permit);
    }
    assert!(request.service(&cpu, &catalog)?.is_none());
    assert!(request.task.is_none());
    assert!(!request.complete);
    drop(permits);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if request.service(&cpu, &catalog)?.is_some() {
            break;
        }
        if Instant::now() > deadline {
            return Err("deferred sky did not resume".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(request.complete);
    assert_eq!(request.created_tick, 1234);

    let mut store = AssetStore::mount(catalog.clone())?;
    let lights = LightCatalog::load(&mut store)?;
    let animations = Arc::new(solarity_asset::AnimationDataCatalog::load(&mut store)?);
    let mut sky = RuntimeSkyResources::load(catalog, &lights, animations)?;
    let first = sky.resolve_name("Sky.m2", 3, 4321)?.ok_or("sky owner")?;
    assert_eq!(sky.resolve_name("SKY.M2", 1, 9999)?, Some(first));
    let owner = &sky.skyboxes[first];
    assert_eq!(owner.phase.flags, 3);
    assert_eq!(owner.request.created_tick, 4321);
    assert!(owner.request.task.is_none());
    assert!(
        owner.model.is_none(),
        "resolving an owner never prepares sources on main"
    );
    assert!(skybox::default_sky(
        &[skybox::SkyboxSlot {
            model: Some(first),
            weight: 1.,
            flags: 0
        }; 4],
        |index| sky.skyboxes[index].model.is_some()
    ));
    Ok(())
}

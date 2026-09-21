//! Real ground-detail sources suspend without publishing partial tile providers.

use super::{
    GroundDetailAssetCache, GroundDetailPreparation, ResidentGroundDetailTile, SharedTerrainSources,
};
use crate::test_support::{ClientFixture, bootstrap_texture_blp, game_object_models};
use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetResourceKey, AssetStore, BlpTextureCache, ClientDataRoot,
    GroundEffectCatalog, Locale, M2Load, M2ModelCache,
};
use solarity_cpu::{CpuExecutionPlan, CpuExecutor, CpuPoolConfig, CpuStoragePlan, CpuTaskStep};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{Arc, mpsc},
};

/// The private cursor consumes the ordinary GroundEffect schemas and source decoder.
#[test]
fn detail_dependency_yields_worker_then_publishes_complete_texture_and_mesh()
-> Result<(), Box<dyn Error>> {
    let mut model = game_object_models::model_with_animations(&[0])?;
    let texture = u32::from_le_bytes(model[0x54..0x58].try_into()?) as usize;
    let path = b"Ground/Grass.blp\0";
    let offset = model.len() as u32;
    model[texture + 8..texture + 12].copy_from_slice(&(path.len() as u32).to_le_bytes());
    model[texture + 12..texture + 16].copy_from_slice(&offset.to_le_bytes());
    model.extend_from_slice(path);
    let mut doodads = b"WDBC".to_vec();
    for word in [1u32, 3, 12, 10, 1, 1, 0] {
        doodads.extend_from_slice(&word.to_le_bytes());
    }
    doodads.extend_from_slice(b"\0grass.m2\0");
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient/GroundEffectDoodad.dbc", &doodads),
        ("World/NoDXT/Detail/grass.m2", &model),
        (
            "World/NoDXT/Detail/grass00.skin",
            &game_object_models::skin()?,
        ),
        ("Ground/Grass.blp", &bootstrap_texture_blp()),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let effects = Arc::new(GroundEffectCatalog::load(&mut store)?);
    let definition = effects.doodad(1).ok_or("ground model definition")?;
    let M2Load::Producer(producer) =
        catalog
            .model_cache_service()
            .request(&AssetResourceKey::new(
                catalog.namespace(),
                definition.path().clone(),
            ))?
    else {
        return Err("source producer".into());
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
    let mut preparation = Some(GroundDetailPreparation {
        ids: vec![1],
        next: 0,
        pending: None,
        resident: ResidentGroundDetailTile {
            catalog: Some(effects),
            ..Default::default()
        },
    });
    let mut cache = GroundDetailAssetCache::default();
    let mut models = M2ModelCache::new();
    let mut textures = BlpTextureCache::new();
    let (notice, observed) = mpsc::channel();
    let task = permit.submit_resumable_with_context(move |_| {
        let current = preparation
            .as_mut()
            .unwrap_or_else(|| unreachable!("unfinished cursor"));
        let mut edge = None;
        match current.advance(
            &mut cache,
            &mut models,
            &mut textures,
            &mut store,
            Some(&shared),
            &mut edge,
        ) {
            Err(error) => CpuTaskStep::Complete(Err(error)),
            Ok(true) => CpuTaskStep::Complete(Ok(preparation
                .take()
                .unwrap_or_else(|| unreachable!("completed cursor"))
                .finish())),
            Ok(false) => {
                assert_eq!(current.next, 0);
                assert!(current.resident.models.is_empty());
                assert!(current.resident.textures.is_empty());
                let _ = notice.send(());
                edge.map_or(CpuTaskStep::Continue, CpuTaskStep::Wait)
            }
        }
    });
    observed.recv_timeout(std::time::Duration::from_secs(5))?;
    assert_eq!(cpu.try_submit(|| 41)?.join()?, 41);
    let mut reader = AssetStore::mount(catalog)?;
    let source = cpu
        .try_submit(move || producer.load(&mut reader))?
        .join()??;
    let resident = task.join()??;
    assert!(resident.models.contains_key(&1));
    assert!(
        resident
            .textures
            .contains_key(&AssetPath::new("Ground/Grass.blp")?)
    );
    assert_eq!(resident.models.len(), 1);
    drop(source);
    cpu.shutdown()?;
    Ok(())
}

//! Terrain service yields without publishing incomplete scenes or losing its cache bank.

use super::{TerrainWorkerCompletion, TerrainWorkerSource, terrain_steps};
use crate::{
    application::terrain_coordinator::TerrainRequest,
    test_support::{ClientFixture, bootstrap_texture_blp, game_object_models},
};
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, MapDefinition, TerrainTileIndex,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan, CpuTaskStep};
use std::{error::Error, num::NonZeroUsize, sync::mpsc};

/// A queued independent service runs before an admitted terrain generation completes.
#[test]
fn terrain_yields_service_before_whole_generation_publication() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, definition) = fixture()?;
    let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let permit = cpu.try_reserve()?;
    let shared = super::super::tile_preparation::SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut steps = terrain_steps(
        TerrainWorkerSource::Catalog(catalog),
        definition,
        request(21)?,
        true,
        shared,
    );
    let (events, received) = mpsc::channel();
    let terminal = events.clone();
    let terrain = permit.submit_resumable_with_context(move |context| {
        let result = steps(context);
        if matches!(result, CpuTaskStep::Complete(_)) {
            let _sent = terminal.send("terrain");
        }
        result
    });
    let marker = cpu.try_submit(move || {
        let _sent = events.send("service");
    })?;
    release.send(())?;
    blocker.join()??;
    marker.join()?;
    let complete = terrain.join()?;
    cpu.shutdown()?;
    let resident = complete.result?.ok_or("required terrain withdrew")?;
    assert_eq!(
        received.into_iter().collect::<Vec<_>>(),
        ["service", "terrain"]
    );
    assert!(complete.worker.is_some());
    assert!(resident.global_world_model.is_none());
    assert_eq!(
        resident.tile.ok_or("missing complete tile")?.index(),
        request(21)?.tile
    );
    Ok(())
}

/// A late asset failure returns the mounted bank, which services a subsequent tile.
#[test]
fn failed_tile_returns_archive_bank_for_the_next_generation() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, definition) = fixture()?;
    let failed = finish(
        TerrainWorkerSource::Catalog(catalog),
        definition.clone(),
        request(22)?,
        true,
    )?;
    assert!(matches!(
        failed.result,
        Err(super::RuntimeTerrainError::SharedTexture(
            solarity_asset::BlpLoadError::Asset(_)
        ))
    ));
    let worker = failed
        .worker
        .ok_or("asset failure lost mounted archive bank")?;
    let identity = worker.assets.identity();
    let complete = finish(
        TerrainWorkerSource::Ready(worker),
        definition,
        request(21)?,
        true,
    )?;
    assert_eq!(
        complete
            .worker
            .ok_or("success lost mounted archive bank")?
            .assets
            .identity(),
        identity
    );
    assert_eq!(
        complete
            .result?
            .ok_or("required terrain withdrew")?
            .tile
            .ok_or("missing tile")?
            .index(),
        request(21)?.tile
    );
    Ok(())
}

/// A finite ceiling catches a continuation that retries failed work indefinitely.
fn finish(
    source: TerrainWorkerSource,
    definition: MapDefinition,
    request: TerrainRequest,
    specular: bool,
) -> Result<TerrainWorkerCompletion, Box<dyn Error>> {
    let mut cpu = test_cpu()?;
    let permit = cpu.try_reserve()?;
    let shared = super::super::tile_preparation::SharedTerrainSources {
        budget: cpu.storage().clone(),
        service: permit.service_control(),
    };
    let mut steps = terrain_steps(source, definition, request, specular, shared);
    let mut turns = 0;
    let task = permit.submit_resumable_with_context(move |context| {
        turns += 1;
        assert!(
            turns <= 128,
            "terrain did not terminate within the fixture operations"
        );
        steps(context)
    });
    let result = task.join()?;
    cpu.shutdown()?;
    Ok(result)
}

/// One service worker exposes exact step boundaries to controlled fixture channels.
fn test_cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        solarity_cpu::CpuExecutionPlan::new(0, 1, 1, 1)?,
        NonZeroUsize::new(3).ok_or("capacity")?,
        CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

/// Withdraw before execution and at each early map/tile boundary. A reused bank
/// returns unchanged in identity and the missing later texture is never required.
#[test]
fn withdrawn_terrain_returns_its_bank_without_publishing_or_decoding_later_assets()
-> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, definition) = fixture()?;
    let complete = finish(
        TerrainWorkerSource::Catalog(catalog),
        definition.clone(),
        request(21)?,
        true,
    )?;
    let mut bank = complete.worker.ok_or("initial mounted bank")?;
    let identity = bank.assets.identity();
    for boundary in 0..=4 {
        let mut cpu = test_cpu()?;
        let (release, wait) = mpsc::channel();
        let (entered, observed) = mpsc::channel();
        let blocker = if boundary == 0 {
            Some(cpu.try_submit(move || {
                let _sent = entered.send(());
                wait.recv()
            })?)
        } else {
            None
        };
        // Separate channels keep the initial queue gate independent from step gates.
        let (step_release, step_wait) = mpsc::channel();
        let (step_entered, step_observed) = mpsc::channel();
        let permit = cpu.try_reserve()?;
        let shared = super::super::tile_preparation::SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let mut steps = terrain_steps(
            TerrainWorkerSource::Ready(bank),
            definition.clone(),
            request(22)?,
            true,
            shared,
        );
        let mut turn = 0;
        let task = permit.submit_resumable_with_context(move |context| {
            let result = steps(context);
            turn += 1;
            if boundary != 0 && turn == boundary {
                assert!(
                    matches!(result, CpuTaskStep::Continue),
                    "withdrawal boundary must precede missing-texture decode"
                );
                let _sent = step_entered.send(());
                let _released = step_wait.recv();
            }
            result
        });
        let arrived = if boundary == 0 {
            observed.recv_timeout(std::time::Duration::from_secs(5))
        } else {
            step_observed.recv_timeout(std::time::Duration::from_secs(5))
        };
        task.cancel();
        let _released = release.send(());
        let _released = step_release.send(());
        if let Some(blocker) = blocker {
            blocker.join()??;
        }
        let cancelled = task.join()?;
        cpu.shutdown()?;
        arrived?;
        assert!(cancelled.result?.is_none());
        bank = cancelled.worker.ok_or("withdrawal lost the mounted bank")?;
        assert_eq!(bank.assets.identity(), identity);
    }
    let complete = finish(
        TerrainWorkerSource::Ready(bank),
        definition,
        request(21)?,
        true,
    )?;
    assert!(complete.result?.is_some());
    Ok(())
}

/// Both authored tiles exist; the second fails only after ADT decode at texture admission.
fn fixture() -> Result<(ClientFixture, ArchiveCatalog, MapDefinition), Box<dyn Error>> {
    let mut manifest = wow_wdt::WdtFile::new(wow_wdt::version::WowVersion::WotLK);
    manifest.mwmo = Some(wow_wdt::chunks::MwmoChunk::new());
    for x in [21, 22] {
        manifest
            .main
            .get_mut(x, 30)
            .ok_or("tile")?
            .set_has_adt(true);
    }
    let mut wdt = Vec::new();
    wow_wdt::WdtWriter::new(&mut wdt).write(&manifest)?;
    let model = game_object_models::model_with_animations(&[0])?;
    let skin = game_object_models::skin()?;
    let fixture = ClientFixture::with_common_files(&[
        ("World/Shared.m2", &model),
        ("World/Shared00.skin", &skin),
        ("DBFilesClient\\Map.dbc", &map_table()),
        ("World\\Maps\\Northrend\\Northrend.wdt", &wdt),
        (
            "World\\Maps\\Northrend\\Northrend_21_30.adt",
            &adt("tileset/fixture/grass.blp", 21)?,
        ),
        (
            "World\\Maps\\Northrend\\Northrend_22_30.adt",
            &adt("tileset/fixture/missing.blp", 22)?,
        ),
        ("tileset\\fixture\\grass.blp", &bootstrap_texture_blp()),
        ("tileset\\fixture\\grass_s.blp", &bootstrap_texture_blp()),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut store = AssetStore::mount(catalog.clone())?;
    let definition = MapCatalog::load(&mut store)?
        .map(571)
        .cloned()
        .ok_or("map")?;
    Ok((fixture, catalog, definition))
}

/// Gives all decoded chunks their real world address for collision validation.
fn adt(texture: &str, x: u32) -> Result<Vec<u8>, Box<dyn Error>> {
    let bytes = wow_adt::builder::AdtBuilder::new()
        .with_version(wow_adt::AdtVersion::WotLK)
        .add_texture(texture)
        .add_model("World/Shared.m2")
        .add_doodad_placement(wow_adt::DoodadPlacement {
            name_id: 0,
            unique_id: 9,
            position: [17066., 0., 17066.],
            rotation: [0.; 3],
            scale: 1024,
            flags: 0,
        })
        .build()?
        .to_bytes()?;
    let wow_adt::ParsedAdt::Root(mut root) = wow_adt::parse_adt(&mut std::io::Cursor::new(bytes))?
    else {
        return Err("not a root ADT".into());
    };
    root.texture_flags = Some(wow_adt::chunks::MtxfChunk { flags: vec![0] });
    for chunk in &mut root.mcnk_chunks {
        chunk.header.position = [
            17_066.666_f32 - (30 * 16 + chunk.header.index_y) as f32 * 33.333_332,
            17_066.666_f32 - (x * 16 + chunk.header.index_x) as f32 * 33.333_332,
            10.,
        ];
        chunk.heights.as_mut().ok_or("heights")?.heights.fill(0.);
    }
    let mut bytes = wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?;
    let chunk = bytes
        .windows(4)
        .rposition(|word| word == b"KNCM")
        .ok_or("MCNK")?;
    let size = u32::from_le_bytes(bytes[chunk + 4..chunk + 8].try_into()?) as usize;
    let end = chunk + 8 + size;
    put_u32(&mut bytes, chunk + 8 + 0x20, (8 + size) as u32);
    put_u32(&mut bytes, chunk + 8 + 0x10, 1);
    let mut references = b"FRCM".to_vec();
    references.extend_from_slice(&4_u32.to_le_bytes());
    references.extend_from_slice(&0_u32.to_le_bytes());
    let final_size = size + references.len();
    bytes.splice(end..end, references);
    put_u32(&mut bytes, chunk + 4, final_size as u32);
    let mcin = bytes
        .windows(4)
        .position(|word| word == b"NICM")
        .ok_or("MCIN")?;
    put_u32(&mut bytes, mcin + 8 + 255 * 16 + 4, final_size as u32);
    Ok(bytes)
}

/// Minimal strict Map.dbc row for the test namespace.
fn map_table() -> Vec<u8> {
    let strings = b"\0Northrend\0";
    let mut fields = [0_u32; 66];
    fields[0] = 571;
    fields[1] = 1;
    fields[5] = 1;
    fields[22] = 571;
    fields[59] = u32::MAX;
    fields[63] = 2;
    let mut bytes = b"WDBC".to_vec();
    for word in [1, 66, 264, strings.len() as u32].into_iter().chain(fields) {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

/// Tile addressing stays independent of any live player or desktop state.
fn request(x: u8) -> Result<TerrainRequest, Box<dyn Error>> {
    Ok(TerrainRequest {
        map_id: 571,
        tile: TerrainTileIndex::new(x, 30).ok_or("tile")?,
    })
}

/// Writes one exact little-endian ADT fixture field.
fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// A real referenced MDDF joins an already admitted source instead of decoding
/// it again. The only worker can run other work while terrain awaits the source.
#[test]
fn terrain_shared_model_gate_preserves_source_identity_failure_and_withdrawal()
-> Result<(), Box<dyn Error>> {
    for terminal in ["success", "abandoned", "cancelled"] {
        let (_fixture, catalog, definition) = fixture()?;
        let key = solarity_asset::AssetResourceKey::new(
            catalog.namespace(),
            solarity_asset::AssetPath::new("World/Shared.m2")?,
        );
        let solarity_asset::M2Load::Producer(producer) =
            catalog.model_cache_service().request(&key)?
        else {
            return Err("fixture source already produced".into());
        };
        let observer = producer.subscribe()?;
        let mut cpu = test_cpu()?;
        let permit = cpu.try_reserve()?;
        let shared = super::super::tile_preparation::SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let mut steps = terrain_steps(
            TerrainWorkerSource::Catalog(catalog.clone()),
            definition,
            request(21)?,
            true,
            shared,
        );
        let (suspended, observed) = mpsc::channel();
        let task = permit.submit_resumable_with_context(move |context| {
            let step = steps(context);
            if matches!(step, CpuTaskStep::Wait(_)) {
                let _sent = suspended.send(());
            }
            step
        });
        let parked = observed.recv_timeout(std::time::Duration::from_secs(15));
        // Always release/cancel before propagating an assertion or timeout.
        if parked.is_err() {
            task.cancel();
        }
        parked?;
        cpu.try_submit(|| ())?.join()?;
        assert!(!task.is_finished());
        let source = match terminal {
            "success" => Some(producer.load(&mut AssetStore::mount(catalog)?)?),
            "abandoned" => {
                drop(producer);
                None
            }
            "cancelled" => {
                task.cancel();
                assert!(observer.poll().is_none());
                // Keeping the shared producer alive must not strand this consumer.
                let done = task.join()?;
                assert!(done.worker.is_some());
                assert!(done.result?.is_none());
                assert!(observer.poll().is_none());
                drop(producer);
                cpu.shutdown()?;
                continue;
            }
            _ => unreachable!(),
        };
        let done = task.join()?;
        cpu.shutdown()?;
        assert!(done.worker.is_some());
        if let Some(source) = source {
            let tile = done
                .result?
                .ok_or("missing resident")?
                .tile
                .ok_or("missing tile")?;
            let delivered = tile
                .m2_scene
                .sources()
                .first()
                .ok_or("missing shared doodad")?
                .model();
            assert!(solarity_asset::ResourceLease::ptr_eq(delivered, &source));
        } else {
            assert!(matches!(
                done.result,
                Err(super::RuntimeTerrainError::SharedModel(
                    solarity_asset::M2LoadError::Abandoned
                ))
            ));
        }
    }
    Ok(())
}

#[test]
fn terrain_shared_texture_gate_preserves_worker_bank_and_cancellation() -> Result<(), Box<dyn Error>>
{
    use solarity_asset::{AssetPath, AssetReadBudget, AssetResourceKey, BlpLoad, BlpLoadError};
    use solarity_cpu::CpuService;
    for terminal in ["success", "abandoned", "cancelled"] {
        let (_fixture, catalog, definition) = fixture()?;
        let key = AssetResourceKey::new(
            catalog.namespace(),
            AssetPath::new("tileset/fixture/grass_s.blp")?,
        );
        let BlpLoad::Producer(producer) = catalog
            .texture_cache_service()
            .request_for(&key, CpuService::Required)?
        else {
            return Err("producer".into());
        };
        let observer = producer.subscribe_for(CpuService::Required)?;
        let mut cpu = test_cpu()?;
        let permit = cpu.try_reserve()?;
        let shared = super::super::tile_preparation::SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let mut steps = terrain_steps(
            TerrainWorkerSource::Catalog(catalog.clone()),
            definition,
            request(21)?,
            true,
            shared,
        );
        let (suspended, observed) = mpsc::channel();
        let task = permit.submit_resumable_with_context(move |context| {
            let step = steps(context);
            if matches!(step, CpuTaskStep::Wait(_)) {
                let _ = suspended.send(());
            }
            step
        });
        let parked = observed.recv_timeout(std::time::Duration::from_secs(15));
        if parked.is_err() {
            task.cancel();
        }
        parked?;
        assert_eq!(cpu.try_submit(|| 17)?.join()?, 17);
        assert!(!task.is_finished());
        match terminal {
            "success" => {
                let source = producer.load(
                    &mut AssetStore::mount(catalog)?,
                    &AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required),
                )?;
                let done = task.join()?;
                assert!(done.worker.is_some());
                let tile = done.result?.ok_or("resident")?.tile.ok_or("tile")?;
                let delivered = tile.textures.first().ok_or("texture")?;
                assert_eq!(
                    delivered.decode_mip(0)?.rgba8(),
                    source.decode_mip(0)?.rgba8()
                );
                assert_eq!(delivered.namespace(), source.namespace());
                assert_eq!(delivered.path(), key.path());
            }
            "abandoned" => {
                drop(producer);
                let done = task.join()?;
                assert!(done.worker.is_some());
                assert!(matches!(
                    done.result,
                    Err(super::RuntimeTerrainError::SharedTexture(
                        BlpLoadError::Abandoned
                    ))
                ));
            }
            "cancelled" => {
                task.cancel();
                let done = task.join()?;
                assert!(done.worker.is_some());
                assert!(done.result?.is_none());
                assert!(observer.poll().is_none());
                drop(producer);
            }
            _ => unreachable!(),
        }
        cpu.shutdown()?;
    }
    Ok(())
}

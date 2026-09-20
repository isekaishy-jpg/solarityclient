//! Terrain service yields without publishing incomplete scenes or losing its cache bank.

use super::{TerrainWorkerCompletion, TerrainWorkerSource, terrain_steps};
use crate::{
    application::terrain_coordinator::TerrainRequest,
    test_support::{ClientFixture, bootstrap_texture_blp},
};
use solarity_asset::{
    ArchiveCatalog, AssetStore, ClientDataRoot, Locale, MapCatalog, MapDefinition, TerrainTileIndex,
};
use solarity_cpu::{CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use std::{error::Error, num::NonZeroUsize, ops::ControlFlow, sync::mpsc};

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
    let mut steps = terrain_steps(
        TerrainWorkerSource::Catalog(catalog),
        definition,
        request(21)?,
        true,
    );
    let (events, received) = mpsc::channel();
    let terminal = events.clone();
    let terrain = cpu.try_reserve()?.submit_steps(move || {
        let result = steps();
        if result.is_break() {
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
    let resident = complete.result?;
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
    let failed = finish(terrain_steps(
        TerrainWorkerSource::Catalog(catalog),
        definition.clone(),
        request(22)?,
        true,
    ))?;
    assert!(matches!(
        failed.result,
        Err(super::RuntimeTerrainError::Asset(_))
    ));
    let worker = failed
        .worker
        .ok_or("asset failure lost mounted archive bank")?;
    let identity = worker.assets.identity();
    let complete = finish(terrain_steps(
        TerrainWorkerSource::Ready(worker),
        definition,
        request(21)?,
        true,
    ))?;
    assert_eq!(
        complete
            .worker
            .ok_or("success lost mounted archive bank")?
            .assets
            .identity(),
        identity
    );
    assert_eq!(
        complete.result?.tile.ok_or("missing tile")?.index(),
        request(21)?.tile
    );
    Ok(())
}

/// A finite ceiling catches a continuation that retries failed work indefinitely.
fn finish(
    mut steps: impl FnMut() -> ControlFlow<TerrainWorkerCompletion>,
) -> Result<TerrainWorkerCompletion, Box<dyn Error>> {
    for _ in 0..128 {
        if let ControlFlow::Break(result) = steps() {
            return Ok(result);
        }
    }
    Err("terrain did not terminate within the finite fixture's operations".into())
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
    let fixture = ClientFixture::with_common_files(&[
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
    Ok(wow_adt::builder::BuiltAdt::from_root_adt(*root, None).to_bytes()?)
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

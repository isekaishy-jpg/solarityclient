//! Regression tests for worker-owned backdrop residency and admission.

use solarity_asset::{M2LoadError, ResourceLease};
use std::error::Error;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::{Arc, mpsc};

use solarity_asset::{ArchiveCatalog, AssetError, AssetPath, ClientDataRoot, Locale};
use solarity_cpu::{CpuExecutor, CpuPoolConfig};
use wow_m2::chunks::texture::{M2Texture as RawTexture, M2TextureFlags, M2TextureType};
use wow_m2::common::{FixedString, M2Array, M2ArrayString};
use wow_m2::header::M2Header;
use wow_m2::skin::OldSkinHeader;
use wow_m2::{M2Model, M2Version, OldSkin};

use super::{GlueBackdropLoader, GlueM2Texture, RuntimeGlueModelError};
use crate::test_support::{self as support, ClientFixture};

fn loader(fixture: &ClientFixture) -> Result<GlueBackdropLoader, Box<dyn Error>> {
    Ok(GlueBackdropLoader::new(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?))
}

fn cpu() -> Result<CpuExecutor, Box<dyn Error>> {
    Ok(CpuExecutor::new(CpuPoolConfig::new(
        {
            let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
            solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
        },
        NonZeroUsize::MIN,
        solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
    ))?)
}

#[test]
fn full_pool_defers_archive_mount_and_preserves_request() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let mut loader = loader(&fixture)?;
    let cpu = cpu()?;
    let path = AssetPath::new("Interface/Glues/Missing.m2")?;
    let (release, wait) = mpsc::channel();
    let blocker = cpu.try_submit(move || wait.recv())?;
    let deferred = loader.poll(&path, &cpu);
    // Release before asserting so a failing regression cannot strand a worker.
    release.send(())?;
    blocker.join()??;
    assert!(matches!(deferred, Ok(None)));
    assert!(!loader.has_pending());
    assert!(
        loader
            .assets
            .as_ref()
            .ok_or("missing owner")?
            .state
            .is_none()
    );
    assert!(matches!(loader.load_blocking(&path, &cpu), Err(source)
        if matches!(source.as_ref(), RuntimeGlueModelError::SharedModel(M2LoadError::Asset(error)) if matches!(error.as_ref(), AssetError::AssetNotFound { path: missing } if *missing == path))));
    Ok(())
}

#[test]
fn changed_selection_retains_original_exact_failure_without_retry() -> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::new()?;
    let mut loader = loader(&fixture)?;
    let cpu = cpu()?;
    let first = AssetPath::new("Interface/Glues/First.m2")?;
    let second = AssetPath::new("Interface/Glues/Second.m2")?;
    assert!(matches!(loader.poll(&first, &cpu), Ok(None)));
    assert!(loader.load_blocking(&second, &cpu).is_err());
    let original = match loader.load_blocking(&first, &cpu) {
        Err(error) => error,
        Ok(_) => return Err("missing first model unexpectedly loaded".into()),
    };
    assert!(
        matches!(original.as_ref(), RuntimeGlueModelError::SharedModel(M2LoadError::Asset(error)) if matches!(error.as_ref(), AssetError::AssetNotFound { path } if *path == first))
    );
    let repeated = match loader.poll(&first, &cpu) {
        Err(error) => error,
        Ok(_) => return Err("missing model failure was not retained".into()),
    };
    assert!(Arc::ptr_eq(&original, &repeated));
    assert!(!loader.has_pending());
    assert_eq!(loader.ready.len(), 2);
    Ok(())
}

#[test]
fn demand_reuses_worker_model_after_selection_changes() -> Result<(), Box<dyn Error>> {
    let mut model = M2Model {
        header: M2Header::new(M2Version::WotLK),
        name: Some("Backdrop".to_owned()),
        ..M2Model::default()
    };
    model.header.num_skin_profiles = Some(1);
    let texture_names: [&[u8]; 3] = [
        b"Interface\\Glues\\Backdrop.blp",
        b"Interface\\Glues\\Missing.blp",
        b"",
    ];
    model.textures = texture_names
        .iter()
        .map(|name| {
            Ok(RawTexture {
                texture_type: M2TextureType::Hardcoded,
                flags: M2TextureFlags::empty(),
                filename: M2ArrayString {
                    string: FixedString {
                        data: name.to_vec(),
                    },
                    array: M2Array::new(u32::try_from(name.len() + 1)?, 0),
                },
            })
        })
        .collect::<Result<Vec<_>, std::num::TryFromIntError>>()?;
    let mut model_bytes = Cursor::new(Vec::new());
    model.write(&mut model_bytes)?;
    // The fixture writer does not relocate string payloads in texture entries.
    let model_bytes = model_bytes.get_mut();
    let textures_offset = u32::from_le_bytes(model_bytes[0x54..0x58].try_into()?) as usize;
    for (index, name) in texture_names.iter().enumerate() {
        let offset = u32::try_from(model_bytes.len())?;
        let entry = textures_offset + index * 16;
        model_bytes[entry + 8..entry + 12]
            .copy_from_slice(&u32::try_from(name.len() + 1)?.to_le_bytes());
        model_bytes[entry + 12..entry + 16].copy_from_slice(&offset.to_le_bytes());
        model_bytes.extend_from_slice(name);
        model_bytes.push(0);
    }
    let skin = OldSkin {
        header: OldSkinHeader::new(),
        indices: Vec::new(),
        triangles: Vec::new(),
        bone_indices: Vec::new(),
        submeshes: Vec::new(),
        batches: Vec::new(),
    };
    let mut skin_bytes = Cursor::new(Vec::new());
    skin.write(&mut skin_bytes)?;
    let fixture = ClientFixture::with_common_files(&[
        ("Interface/Glues/Backdrop.m2", model_bytes),
        ("Interface/Glues/Backdrop00.skin", skin_bytes.get_ref()),
        (
            "Interface/Glues/Backdrop.blp",
            &support::bootstrap_texture_blp(),
        ),
    ])?;
    let mut loader = loader(&fixture)?;
    let cpu = cpu()?;
    let path = AssetPath::new("Interface/Glues/Backdrop.m2")?;
    let other = AssetPath::new("Interface/Glues/Other.m2")?;
    assert!(matches!(loader.poll(&path, &cpu), Ok(None)));
    assert!(loader.load_blocking(&other, &cpu).is_err());
    let loaded = loader.load_blocking(&path, &cpu)?;
    let reused = loader
        .poll(&path, &cpu)?
        .ok_or("ready model was deferred")?;
    assert!(Arc::ptr_eq(&loaded, &reused));
    assert!(ResourceLease::ptr_eq(&loaded.model, &reused.model));
    assert_eq!(loaded.model.path(), &path);
    assert!(matches!(
        &loaded.textures[..],
        [
            GlueM2Texture::Authored(_),
            GlueM2Texture::StockFailure,
            GlueM2Texture::StockWhite
        ]
    ));
    let (GlueM2Texture::Authored(original_texture), GlueM2Texture::Authored(reused_texture)) =
        (&loaded.textures[0], &reused.textures[0])
    else {
        return Err("authored texture was lost".into());
    };
    assert!(Arc::ptr_eq(original_texture, reused_texture));
    assert!(!loader.has_pending());
    Ok(())
}

/// Independent presentation loaders wait off-pool and publish one shared model generation.
#[test]
fn separate_backdrops_join_one_model_without_occupying_waiting_workers()
-> Result<(), Box<dyn Error>> {
    let fixture = ClientFixture::with_common_files(&[
        (
            "World/Shared.m2",
            &support::game_object_models::model_with_animations(&[0])?,
        ),
        ("World/Shared00.skin", &support::game_object_models::skin()?),
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let path = AssetPath::new("World/Shared.m2")?;
    let key = solarity_asset::AssetResourceKey::new(catalog.namespace(), path.clone());
    let solarity_asset::M2Load::Producer(producer) = catalog.model_cache_service().request(&key)?
    else {
        return Err("missing initial producer".into());
    };
    let mut first = GlueBackdropLoader::new(catalog.clone());
    let mut second = GlueBackdropLoader::new(catalog.clone());
    let cpu = cpu()?;
    let occupied = cpu.try_reserve()?;
    assert!(first.poll(&path, &cpu)?.is_none());
    assert!(second.poll(&path, &cpu)?.is_none());
    assert!(first.pending.is_none() && second.pending.is_none());
    assert!(first.waiting.is_some() && second.waiting.is_some());
    assert!(
        first
            .assets
            .as_ref()
            .ok_or("missing owner")?
            .state
            .is_none()
    );
    // The sole CPU slot remains available: neither consumer submitted a polling/waiting job.
    drop(occupied);
    let mut store = solarity_asset::AssetStore::mount(catalog)?;
    let produced = cpu
        .try_submit(move || producer.load(&mut store))?
        .join()??;
    let first = first.load_blocking(&path, &cpu)?;
    let second = second.load_blocking(&path, &cpu)?;
    assert!(ResourceLease::ptr_eq(&produced, &first.model));
    assert!(ResourceLease::ptr_eq(&produced, &second.model));
    Ok(())
}

/// Backdrop material waits release the sole worker and return the reader on every terminal path.
#[test]
fn backdrop_texture_steps_share_failure_and_cancel_without_stock_substitution()
-> Result<(), Box<dyn Error>> {
    use solarity_asset::{
        AssetReadBudget, AssetResourceKey, AssetStore, BlpLoad, BlpLoadError, M2Load,
    };
    use solarity_cpu::{CpuService, CpuTaskStep};
    use std::time::Duration;
    for outcome in 0..3 {
        let mut model = support::game_object_models::model_with_animations(&[0])?;
        let texture = u32::from_le_bytes(model[0x54..0x58].try_into()?) as usize;
        let name = b"Textures/Backdrop.blp\0";
        let offset = model.len() as u32;
        model[texture + 8..texture + 12].copy_from_slice(&(name.len() as u32).to_le_bytes());
        model[texture + 12..texture + 16].copy_from_slice(&offset.to_le_bytes());
        model.extend_from_slice(name);
        let fixture = ClientFixture::with_common_files(&[
            ("Backdrop.m2", &model),
            ("Backdrop00.skin", &support::game_object_models::skin()?),
            ("Textures/Backdrop.blp", &support::bootstrap_texture_blp()),
        ])?;
        let catalog =
            ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
        let BlpLoad::Producer(producer) = catalog.texture_cache_service().request_for(
            &AssetResourceKey::new(
                catalog.namespace(),
                AssetPath::new("Textures/Backdrop.blp")?,
            ),
            CpuService::Required,
        ) else {
            return Err("texture producer".into());
        };
        let mut producer = Some(producer);
        let M2Load::Producer(primary) =
            catalog
                .model_cache_service()
                .request(&AssetResourceKey::new(
                    catalog.namespace(),
                    AssetPath::new("Backdrop.m2")?,
                ))?
        else {
            return Err("primary producer".into());
        };
        let mut cpu = CpuExecutor::new(CpuPoolConfig::new(
            solarity_cpu::CpuExecutionPlan::new(0, 1, 1, 1)?,
            NonZeroUsize::new(3).ok_or("capacity")?,
            solarity_cpu::CpuStoragePlan::new(64 << 20, 64 << 20, 16 << 20),
        ))?;
        let permit = cpu.try_reserve()?;
        let shared = crate::application::terrain_coordinator::SharedTerrainSources {
            budget: cpu.storage().clone(),
            service: permit.service_control(),
        };
        let mut steps = super::BackdropArchiveOwner {
            catalog: catalog.clone(),
            state: None,
        }
        .steps(super::BackdropModel::Producer(primary), shared);
        let (notice, observed) = mpsc::channel();
        let task = permit.submit_resumable_with_context(move |context| {
            let step = steps(context);
            if matches!(step, CpuTaskStep::Wait(_)) {
                let _ = notice.send(());
            }
            step
        });
        observed.recv_timeout(Duration::from_secs(5))?;
        assert_eq!(cpu.try_submit(|| 19)?.join()?, 19);
        assert!(!task.is_finished());
        match outcome {
            0 => {
                let mut reader = AssetStore::mount(catalog)?;
                let policy =
                    AssetReadBudget::for_service(cpu.storage().clone(), CpuService::Required);
                let source = producer.take().ok_or("producer")?;
                cpu.try_submit(move || source.load(&mut reader, &policy))?
                    .join()??;
            }
            1 => drop(producer.take()),
            _ => task.cancel(),
        }
        let complete = task.join()?;
        assert!(matches!(complete.assets.state, Some(Ok(_))));
        match outcome {
            0 => assert!(matches!(
                complete.result?.textures.as_slice(),
                [GlueM2Texture::Authored(_)]
            )),
            1 => assert!(
                matches!(complete.result, Err(error) if matches!(error.as_ref(), RuntimeGlueModelError::Asset(AssetError::TextureRequest(BlpLoadError::Abandoned))))
            ),
            _ => assert!(
                matches!(complete.result, Err(error) if matches!(error.as_ref(), RuntimeGlueModelError::Cpu(solarity_cpu::CpuError::JobCancelled)))
            ),
        }
        drop(producer);
        cpu.shutdown()?;
    }
    Ok(())
}

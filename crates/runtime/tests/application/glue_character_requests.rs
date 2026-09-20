//! Live Glue selection shares primary sources without occupying a waiting worker.

use super::super::{
    RuntimePlayerCatalogs, RuntimePlayerError, RuntimePlayerItemCatalogs, RuntimePlayerPresentation,
};
use crate::test_support::{ClientFixture, unit_models};
use solarity_asset::{
    AnimationDataCatalog, ArchiveCatalog, AssetResourceKey, AssetStore, AssetStoreHandle,
    CharacterAppearanceCatalog, CharacterRaceCatalog, CharacterStartOutfitCatalog, ClientDataRoot,
    CreatureCatalog, CreatureFamilyCatalog, HelmetGeosetVisibilityCatalog, ItemDefinitionCatalog,
    ItemDisplayCatalog, ItemVisualCatalog, Locale, M2Load, M2LoadError, ParticleColorCatalog,
    ResourceLease,
};
use solarity_cpu::{CoordinatorNotifier, CpuExecutor, CpuPoolConfig, CpuStoragePlan};
use solarity_rendering::CharacterComponentTextureLevel;
use solarity_ui::{
    UiCharacterDirectory, UiCharacterEquipment, UiCharacterInfo, UiCharacterPetPreview,
    UiCharacterSelectionPreview,
};
use std::{
    error::Error,
    num::NonZeroUsize,
    sync::{Arc, mpsc},
    time::Duration,
};

/// Public UI projection supplies complete stock selection inputs without a window.
fn selection(guid: u64) -> UiCharacterSelectionPreview {
    UiCharacterDirectory::new(
        vec![UiCharacterInfo::new(
            guid,
            "Soap".into(),
            "Human".into(),
            1,
            "Human".into(),
            "Mage".into(),
            8,
            80,
            None,
            2,
            0,
            [0; 5],
            [UiCharacterEquipment::default(); 23],
            UiCharacterPetPreview::default(),
            0,
            0,
        )],
        "Human".into(),
    )
    .selection_preview()
    .unwrap_or_else(|| unreachable!("single fixture character is selected"))
}

/// Catalog clones preserve one namespace and source-request authority across owners.
fn presentation(catalog: &ArchiveCatalog) -> Result<RuntimePlayerPresentation, Box<dyn Error>> {
    let mut store = AssetStore::mount(catalog.clone())?;
    let catalogs = RuntimePlayerCatalogs::new(
        AnimationDataCatalog::load(&mut store)?,
        CreatureCatalog::load(&mut store)?,
        CreatureFamilyCatalog::load(&mut store)?,
        CharacterAppearanceCatalog::load(&mut store)?,
        CharacterRaceCatalog::load(&mut store)?,
        HelmetGeosetVisibilityCatalog::load(&mut store)?,
        CharacterStartOutfitCatalog::load(&mut store)?,
        RuntimePlayerItemCatalogs::new(
            ItemDefinitionCatalog::load(&mut store)?,
            ItemDisplayCatalog::load(&mut store)?,
            ItemVisualCatalog::load(&mut store)?,
        ),
        ParticleColorCatalog::load(&mut store)?,
    );
    Ok(
        RuntimePlayerPresentation::new(AssetStoreHandle::new(store), catalogs)
            .with_glue_worker_catalog(catalog.clone()),
    )
}

/// Real coordinator notifications provide bounded test waits without polling sleeps.
struct Notify(mpsc::Sender<()>);
impl CoordinatorNotifier for Notify {
    fn notify(&self) {
        let _sent = self.0.send(());
    }
}

fn executor(capacity: usize) -> Result<(CpuExecutor, mpsc::Receiver<()>), Box<dyn Error>> {
    let (send, receive) = mpsc::channel();
    let cpu = CpuExecutor::with_notifier(
        CpuPoolConfig::new(
            {
                let total: std::num::NonZeroUsize = NonZeroUsize::MIN;
                solarity_cpu::CpuExecutionPlan::new(total.get() - 1, 1, 1, 1)
                    .unwrap_or_else(|_| unreachable!("one flexible worker fits a nonzero total"))
            },
            NonZeroUsize::new(capacity).ok_or("capacity")?,
            CpuStoragePlan::new(64 << 20, 64 << 20, 0),
        ),
        Arc::new(Notify(send)),
    )?;
    Ok((cpu, receive))
}

fn catalog(fixture: &ClientFixture) -> Result<ArchiveCatalog, Box<dyn Error>> {
    Ok(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)
}

/// Called only after the test publishes the source or withdraws its dependent.
fn wait(
    presentation: &RuntimePlayerPresentation,
    notifications: &mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    while presentation
        .pending_glue_character
        .as_ref()
        .is_some_and(|pending| !pending.task.is_finished())
    {
        notifications.recv_timeout(Duration::from_secs(10))?;
    }
    Ok(())
}

fn source_key(
    presentation: &RuntimePlayerPresentation,
    catalog: &ArchiveCatalog,
) -> Result<AssetResourceKey, RuntimePlayerError> {
    Ok(AssetResourceKey::new(
        catalog.namespace(),
        presentation.glue_primary_path(&super::super::ResidentGlueCharacterKey::Selection(
            Box::new(selection(1)),
        ))?,
    ))
}

#[test]
fn glue_joins_pending_source_and_only_publishes_latest_selection() -> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture()?;
    let catalog = catalog(&fixture)?;
    let mut current = presentation(&catalog)?;
    let service = catalog.model_cache_service();
    let M2Load::Producer(producer) = service.request(&source_key(&current, &catalog)?)? else {
        return Err("new producer".into());
    };
    let observer = producer.subscribe();
    let (cpu, notifications) = executor(8)?;
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    assert!(current.glue_character.is_none());
    assert!(current.glue_worker_cache.is_none());
    assert_eq!(
        cpu.try_submit(|| 42)?.join()?,
        42,
        "source wait leaves the sole worker available"
    );
    // Main coalesces residency changes; the obsolete job never owns the latest key.
    current.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    current.synchronize_character_selection_async(Some(&selection(3)), &cpu)?;
    assert!(
        observer.poll().is_none(),
        "selection withdrawal cannot abandon the producer"
    );
    let model = producer.load(&mut AssetStore::mount(catalog.clone())?)?;
    for _ in 0..8 {
        wait(&current, &notifications)?;
        current.synchronize_character_selection_async(Some(&selection(3)), &cpu)?;
        if current.glue_character.is_some() {
            break;
        }
    }
    let resident = current
        .glue_character
        .as_ref()
        .ok_or("latest selected character")?;
    assert!(
        resident
            .key
            .same_residency(&super::super::ResidentGlueCharacterKey::Selection(
                Box::new(selection(3))
            ))
    );
    assert!(
        ResourceLease::ptr_eq(&resident.model, &model),
        "shared source is consumed directly"
    );
    let mut serial = presentation(&catalog)?;
    serial.synchronize_character_selection(Some(&selection(3)))?;
    let reference = serial.glue_character.as_ref().ok_or("serial reference")?;
    assert_eq!(resident.atlas.mips(), reference.atlas.mips());
    assert_eq!(resident.geosets, reference.geosets);
    assert_eq!(resident.facing_radians, reference.facing_radians);
    assert!(current.glue_worker_cache.is_some());
    assert!(!current.synchronize_character_selection_async(Some(&selection(3)), &cpu)?);
    assert!(current.pending_glue_character.is_none());
    Ok(())
}

#[test]
fn glue_withdrawal_does_not_cancel_another_source_consumer() -> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture()?;
    let catalog = catalog(&fixture)?;
    let mut first = presentation(&catalog)?;
    let mut second = presentation(&catalog)?;
    let M2Load::Producer(producer) = catalog
        .model_cache_service()
        .request(&source_key(&first, &catalog)?)?
    else {
        return Err("new producer".into());
    };
    let observer = producer.subscribe();
    let (cpu, notifications) = executor(8)?;
    first.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    second.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    first.synchronize_character_selection_async(None, &cpu)?;
    wait(&first, &notifications)?;
    first.synchronize_character_selection_async(None, &cpu)?;
    assert!(first.pending_glue_character.is_none());
    assert!(first.glue_worker_cache.is_some());
    assert!(observer.poll().is_none());
    let model = producer.load(&mut AssetStore::mount(catalog.clone())?)?;
    wait(&second, &notifications)?;
    second.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    assert!(ResourceLease::ptr_eq(
        &second
            .glue_character
            .as_ref()
            .ok_or("second character")?
            .model,
        &model
    ));
    assert!(first.glue_character.is_none());
    Ok(())
}

#[test]
fn glue_source_failure_returns_cache_and_keeps_current_failure_identity()
-> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture()?;
    let catalog = catalog(&fixture)?;
    let mut current = presentation(&catalog)?;
    let M2Load::Producer(producer) = catalog
        .model_cache_service()
        .request(&source_key(&current, &catalog)?)?
    else {
        return Err("new producer".into());
    };
    let (cpu, notifications) = executor(8)?;
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    drop(producer);
    wait(&current, &notifications)?;
    assert!(matches!(
        current.synchronize_character_selection_async(Some(&selection(1)), &cpu),
        Err(RuntimePlayerError::ModelRequest(M2LoadError::Abandoned))
    ));
    assert!(current.glue_worker_cache.is_some());
    assert!(current.glue_character.is_none());
    assert!(!current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?);
    assert!(
        current.pending_glue_character.is_none(),
        "same failed selection does not silently retry"
    );
    current.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    wait(&current, &notifications)?;
    current.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    assert!(current.glue_character.is_some());
    Ok(())
}

#[test]
fn glue_admission_refusal_preserves_cache_then_new_producer_is_shared() -> Result<(), Box<dyn Error>>
{
    let fixture = unit_models::fixture()?;
    let catalog = catalog(&fixture)?;
    let mut current = presentation(&catalog)?;
    let (cpu, notifications) = executor(1)?;
    let permit = cpu.try_reserve()?;
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    assert!(current.pending_glue_character.is_none());
    assert!(current.glue_worker_cache.is_some());
    drop(permit);
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    wait(&current, &notifications)?;
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    let M2Load::Ready(shared) = catalog
        .model_cache_service()
        .request(&source_key(&current, &catalog)?)?
    else {
        return Err("published shared source".into());
    };
    assert!(ResourceLease::ptr_eq(
        &current.glue_character.as_ref().ok_or("resident")?.model,
        &shared
    ));
    Ok(())
}

#[test]
fn glue_quality_change_withdraws_old_appearance_even_when_selection_is_unchanged()
-> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture()?;
    let catalog = catalog(&fixture)?;
    let mut current = presentation(&catalog)?;
    let M2Load::Producer(producer) = catalog
        .model_cache_service()
        .request(&source_key(&current, &catalog)?)?
    else {
        return Err("new producer".into());
    };
    let (cpu, notifications) = executor(8)?;
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    let level = CharacterComponentTextureLevel::new(8).ok_or("component level")?;
    assert!(current.set_component_texture_level(level));
    current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    producer.load(&mut AssetStore::mount(catalog.clone())?)?;
    for _ in 0..8 {
        wait(&current, &notifications)?;
        current.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
        if current.glue_character.is_some() {
            break;
        }
    }
    assert_eq!(
        current
            .glue_character
            .as_ref()
            .ok_or("new quality")?
            .atlas
            .mip(0)
            .ok_or("top mip")?
            .width(),
        level.atlas_size()
    );
    assert!(current.glue_worker_cache.is_some());
    Ok(())
}

#[test]
fn glue_ready_source_is_shared_and_withdrawal_removes_resident_immediately()
-> Result<(), Box<dyn Error>> {
    let fixture = unit_models::fixture()?;
    let catalog = catalog(&fixture)?;
    let mut first = presentation(&catalog)?;
    let mut second = presentation(&catalog)?;
    let (cpu, notifications) = executor(8)?;
    first.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    wait(&first, &notifications)?;
    first.synchronize_character_selection_async(Some(&selection(1)), &cpu)?;
    second.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    wait(&second, &notifications)?;
    second.synchronize_character_selection_async(Some(&selection(2)), &cpu)?;
    assert!(ResourceLease::ptr_eq(
        &first.glue_character.as_ref().ok_or("first resident")?.model,
        &second
            .glue_character
            .as_ref()
            .ok_or("second resident")?
            .model,
    ));
    // Match the synchronous None transition: hide now, even when the request changed.
    assert!(first.synchronize_character_selection_async(None, &cpu)?);
    assert!(first.glue_character.is_none());
    assert!(!first.synchronize_character_selection_async(None, &cpu)?);
    assert!(second.glue_character.is_some());
    Ok(())
}

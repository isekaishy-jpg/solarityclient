//! Exercises stock Glue screen transitions against an installed client.

use std::cell::RefCell;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;
use std::rc::Rc;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_cpu::BlizzardRand;
use solarity_ui::{
    AddonCatalog, GlueInitialScreen, GlueManager, UiCharacterDirectory, UiCharacterEquipment,
    UiCharacterExpansion, UiCharacterInfo, UiCharacterPetPreview, UiEventArgument, UiEventPayload,
    UiGlueNetworkStatus,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| argument_error("missing client Data directory"))?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error("missing ASCII locale such as enUS"))?
        .parse::<Locale>()?;
    let screen = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| argument_error("missing Glue screen: login, charselect, or charcreate"))?;
    if arguments.next().is_some() {
        return Err(argument_error("unexpected extra arguments").into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let assets = AssetStoreHandle::new(AssetStore::mount(catalog)?);
    let mut manager = GlueManager::start_shared_with_profile_and_random(
        assets,
        (1280, 720),
        false,
        GlueInitialScreen::Login,
        &[],
        &AddonCatalog::default(),
        Rc::new(RefCell::new(BlizzardRand::new(0))),
    )?;
    let transition_started = std::time::Instant::now();
    match screen.as_str() {
        "login" => {}
        "charselect" => activate_character_selection(&mut manager)?,
        "charcreate" => activate_character_creation(&mut manager)?,
        _ => {
            return Err(
                argument_error("Glue screen must be login, charselect, or charcreate").into(),
            );
        }
    }
    let transition_elapsed = transition_started.elapsed();
    if manager.current_screen() != screen {
        return Err(argument_error(&format!(
            "requested Glue screen {screen}, but {} remained active",
            manager.current_screen()
        ))
        .into());
    }
    if manager.presentation().models().len() != 1 {
        return Err(argument_error("active stock Glue screen must expose one ModelFFX").into());
    }

    println!(
        "validated Glue screen {} with {} objects, {} packets, and {} models in {:.3} ms",
        manager.current_screen(),
        manager.objects().len(),
        manager.presentation().packets().len(),
        manager.presentation().models().len(),
        transition_elapsed.as_secs_f64() * 1_000.0,
    );
    for model in manager.presentation().models() {
        let background = model.background_lights().live().iter().flatten().count();
        let character = model.character_lights().live().iter().flatten().count();
        let pet = model.pet_lights().live().iter().flatten().count();
        println!(
            "model {} camera={} sequence={} sequence_time={}:{} scale={} bounds=({}, {}, {}, {}) alpha={} glow={} lights={}/{}/{}",
            model.path(),
            model.camera(),
            model.sequence(),
            model.sequence_time_sequence(),
            model.sequence_time_ms(),
            model.model_scale(),
            model.bounds().left(),
            model.bounds().bottom(),
            model.bounds().right(),
            model.bounds().top(),
            model.alpha(),
            model.glow(),
            background,
            character,
            pet,
        );
        println!(
            "  background={:?}\n  character={:?}\n  pet={:?}",
            model.background_lights().live(),
            model.character_lights().live(),
            model.pet_lights().live(),
        );
    }
    Ok(())
}

/// Drives the same empty-then-populated event order as the runtime owner.
fn activate_character_selection(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    manager.set_network_status(UiGlueNetworkStatus::new(None, true));
    manager.set_character_directory(UiCharacterDirectory::default());
    let snapshots = manager.runtime_snapshot_count();
    let screen_started = std::time::Instant::now();
    let screen_dispatch = manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
    )?;
    println!(
        "SET_GLUE_SCREEN subscribers: {} in {:.3} ms ({} snapshots)",
        screen_dispatch.subscriber_count(),
        screen_started.elapsed().as_secs_f64() * 1_000.0,
        manager.runtime_snapshot_count() - snapshots,
    );
    advance_fades(manager)?;
    let character = UiCharacterInfo::new(
        1,
        "SolarityTester".to_owned(),
        "Human".to_owned(),
        1,
        "Human".to_owned(),
        "Warrior".to_owned(),
        1,
        80,
        Some("Stormwind City".to_owned()),
        2,
        0,
        [0; 5],
        [UiCharacterEquipment::default(); 23],
        UiCharacterPetPreview::default(),
        0,
        0,
    );
    manager.set_character_directory(UiCharacterDirectory::new(
        vec![character],
        "Human".to_owned(),
    ));
    let directory_started = std::time::Instant::now();
    let directory_dispatch = manager.dispatch_event(
        "CHARACTER_LIST_UPDATE",
        &UiEventPayload::new([UiEventArgument::Integer(1)])?,
    )?;
    println!(
        "CHARACTER_LIST_UPDATE subscribers: {} in {:.3} ms",
        directory_dispatch.subscriber_count(),
        directory_started.elapsed().as_secs_f64() * 1_000.0,
    );
    Ok(())
}

/// Activates DBC-backed creation after publishing the Wrath entitlement.
fn activate_character_creation(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    manager.set_network_status(UiGlueNetworkStatus::new(None, true));
    manager.set_character_creation_expansion(UiCharacterExpansion::WRATH_OF_THE_LICH_KING);
    let snapshots = manager.runtime_snapshot_count();
    let screen_started = std::time::Instant::now();
    let screen_dispatch = manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charcreate".to_owned())])?,
    )?;
    println!(
        "SET_GLUE_SCREEN subscribers: {} in {:.3} ms ({} snapshots)",
        screen_dispatch.subscriber_count(),
        screen_started.elapsed().as_secs_f64() * 1_000.0,
        manager.runtime_snapshot_count() - snapshots,
    );
    advance_fades(manager)?;
    Ok(())
}

/// Advances authored Glue fades far enough to enter the pending screen.
fn advance_fades(manager: &mut GlueManager) -> Result<(), Box<dyn Error>> {
    for _frame in 0..120 {
        manager.update(1.0 / 60.0)?;
    }
    Ok(())
}

/// Constructs one command-line error without introducing a parser dependency.
fn argument_error(message: &str) -> IoError {
    IoError::new(ErrorKind::InvalidInput, message)
}

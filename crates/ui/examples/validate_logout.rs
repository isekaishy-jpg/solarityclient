//! Validates logout and quit against the mounted stock FrameXML without a renderer.

use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FrameManager, UiFactionGroup, UiPlayerClassState, UiPlayerFactionState,
    UiPlayerIdentityState, UiPlayerLanguage, UiPlayerProgressionState, UiPlayerRaceState,
    UiPlayerState, UiPlayerVitalsState, UiRealmDate, UiRealmTime, UiSpellBookTab, UiUnitPowerType,
    UiZoneState,
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
        .ok_or_else(|| argument_error("missing locale"))?
        .parse::<Locale>()?;
    if arguments.next().is_some() {
        return Err(argument_error("unexpected extra argument").into());
    }

    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(data_root)?, locale)?;
    let mut store = AssetStore::mount(catalog)?;
    let addons = AddonCatalog::discover(&mut store)?;
    let assets = AssetStoreHandle::new(store);
    let environment = solarity_ui::UiScriptEnvironment::new(1920, 1080, false)?;
    let world = environment.world_state();
    world.enter_player(UiPlayerState::new(0));
    world.set_player_guid(1);
    world.set_player_identity(UiPlayerIdentityState::new("SolarityTester", 80));
    world.set_player_class(UiPlayerClassState::new("Warrior", "WARRIOR", 1));
    world.set_player_race(UiPlayerRaceState::new("Human", "Human", 1));
    world.set_player_progression(UiPlayerProgressionState::new(0, 1));
    world.set_player_vitals(UiPlayerVitalsState::new(
        100,
        100,
        100,
        100,
        UiUnitPowerType::Mana,
    ));
    world.set_player_faction(UiPlayerFactionState::new(
        UiFactionGroup::Alliance,
        "Alliance",
    ));
    world.set_player_default_language(UiPlayerLanguage::new(7, "Common"));
    world.set_zone(UiZoneState::new("", "", "", "", None, false, None));
    world.set_realm_date(UiRealmDate::new(3, 12, 8, 2009)?);
    world.set_realm_time(UiRealmTime::new(12, 0)?);
    environment.action_bar_state().set_slots([0; 144]);
    environment
        .spell_book_state()
        .set_tabs(vec![UiSpellBookTab::new(
            "General",
            "Interface\\Icons\\INV_Misc_QuestionMark",
            0,
            0,
        )]);

    let started = std::time::Instant::now();
    let mut manager = FrameManager::start_shared(assets, environment, &[], &addons)?;
    println!(
        "FrameXML construction: {:.3}s",
        started.elapsed().as_secs_f64()
    );
    let mut errors = Vec::new();
    for event in [
        "VARIABLES_LOADED",
        "UPDATE_CHAT_WINDOWS",
        "PLAYER_LOGIN",
        "UPDATE_BINDINGS",
        "PLAYER_ENTERING_WORLD",
    ] {
        if let Err(error) = manager.dispatch_event(event, &solarity_ui::UiEventPayload::empty()) {
            errors.push(format!("{event}: {error}"));
        }
    }
    for update in 1..=3 {
        if let Err(error) = manager.update(1.0 / 60.0) {
            errors.push(format!("OnUpdate {update}: {error}"));
        }
    }
    if !errors.is_empty() {
        return Err(IoError::new(ErrorKind::InvalidData, errors.join("\n\n")).into());
    }
    check_logout_dialogs(&mut manager, &world)?;
    Ok(())
}

/// Clicks the authored menu and countdown buttons, without opening a network session.
fn check_logout_dialogs(
    manager: &mut FrameManager,
    world: &solarity_ui::UiWorldState,
) -> Result<(), Box<dyn Error>> {
    let state = world.logout();
    for (menu, event, cancel_button) in [
        (
            "GameMenuButtonLogout",
            "PLAYER_CAMPING",
            "StaticPopup1Button1",
        ),
        (
            "GameMenuButtonQuit",
            "PLAYER_QUITING",
            "StaticPopup1Button2",
        ),
    ] {
        manager.invoke_binding("TOGGLEGAMEMENU", true)?;
        click_stock_button(manager, menu)?;
        if state.pending_action() != Some(solarity_ui::UiLogoutAction::Request)
            || manager.take_process_action().is_some()
        {
            return Err(IoError::other("stock menu did not queue a server-owned logout").into());
        }
        state.accept_action();
        if state.response(0, false) != Some(event) {
            return Err(IoError::other("incorrect stock camping destination").into());
        }
        manager.dispatch_event(event, &solarity_ui::UiEventPayload::empty())?;
        manager.update(1.0)?;
        if manager.region_is_shown("StaticPopup1") != Some(true) {
            return Err(IoError::other("stock countdown popup was not shown").into());
        }
        click_stock_button(manager, cancel_button)?;
        if manager.region_is_shown("StaticPopup1") != Some(false)
            || state.pending_action() != Some(solarity_ui::UiLogoutAction::Cancel)
        {
            return Err(IoError::other(
                "stock cancellation did not hide its countdown and queue CancelLogout",
            )
            .into());
        }
        while let Some(action) = state.pending_action() {
            if action != solarity_ui::UiLogoutAction::Cancel {
                return Err(IoError::other("unexpected countdown cancellation action").into());
            }
            state.accept_action();
        }
        if state.cancel_acknowledged().is_some() {
            return Err(IoError::other("local cancellation retained native pending state").into());
        }
    }
    manager.invoke_binding("TOGGLEGAMEMENU", true)?;
    click_stock_button(manager, "GameMenuButtonQuit")?;
    state.accept_action();
    manager.dispatch_event("PLAYER_QUITING", &solarity_ui::UiEventPayload::empty())?;
    click_stock_button(manager, "StaticPopup1Button1")?;
    if manager.take_process_action() != Some(solarity_ui::UiProcessAction::Quit) {
        return Err(IoError::other("stock Quit Now did not emit ForceQuit").into());
    }
    for event in ["PLAYER_LOGOUT", "PLAYER_LEAVING_WORLD"] {
        manager.dispatch_event(event, &solarity_ui::UiEventPayload::empty())?;
    }
    if let Some(error) = manager.take_callback_failure() {
        return Err(IoError::other(error).into());
    }
    println!(
        "Stock logout and quit menu clicks opened countdowns, cancelled, and emitted Quit Now correctly"
    );
    Ok(())
}

/// Exercises normal pointer hit-testing and authored OnClick, including OnHide effects.
fn click_stock_button(manager: &mut FrameManager, name: &str) -> Result<(), Box<dyn Error>> {
    let bounds = (0..manager.geometry().region_count())
        .find(|&index| manager.object_name(index) == Some(name))
        .and_then(|index| manager.geometry().region(index))
        .map(solarity_ui::UiRegionGeometry::presentation_bounds)
        .ok_or_else(|| IoError::other(format!("missing stock button {name}")))?;
    let position = (
        (bounds.left() + bounds.right()) * 0.5,
        (bounds.bottom() + bounds.top()) * 0.5,
    );
    manager.pointer_motion(position)?;
    manager.pointer_button(position, solarity_ui::UiPointerButton::Left, true)?;
    manager.pointer_button(position, solarity_ui::UiPointerButton::Left, false)?;
    Ok(())
}

fn argument_error(message: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!("{message}; usage: validate_logout <Data> <locale>"),
    )
}

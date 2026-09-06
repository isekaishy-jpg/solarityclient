//! Executes the stock FrameXML startup and first update without a renderer.

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
    for expected in [true, false, true, false] {
        manager.invoke_binding("TOGGLEGAMEMENU", true)?;
        if manager.region_is_shown("GameMenuFrame") != Some(expected) {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                "Escape did not toggle the actual stock game menu",
            )
            .into());
        }
    }
    manager.set_input_event_time(1234);
    for (binding, control) in [
        ("TURNORACTION", solarity_ui::UiMovementControl::TurnOrAction),
        (
            "CAMERAORSELECTORMOVE",
            solarity_ui::UiMovementControl::CameraOrSelectOrMove,
        ),
    ] {
        for pressed in [true, false] {
            manager.invoke_binding(binding, pressed)?;
            let expected = solarity_ui::UiMovementCommand {
                action: if control == solarity_ui::UiMovementControl::CameraOrSelectOrMove
                    && !pressed
                {
                    solarity_ui::UiMovementAction::CameraOrbitStop {
                        sticky_camera: false,
                    }
                } else {
                    solarity_ui::UiMovementAction::Hold { control, pressed }
                },
                timestamp_ms: 1234,
            };
            if manager.take_movement_command() != Some(expected) {
                return Err(IoError::new(
                    ErrorKind::InvalidData,
                    "stock mouse binding lost its native command",
                )
                .into());
            }
        }
    }
    manager.set_modifier_keys(solarity_ui::UiModifierKeys::new(
        false, false, true, false, false, false,
    ));
    manager.invoke_binding("CAMERAORSELECTORMOVE", true)?;
    let _press = manager.take_movement_command();
    manager.invoke_binding("CAMERAORSELECTORMOVE", false)?;
    if !matches!(
        manager.take_movement_command(),
        Some(solarity_ui::UiMovementCommand {
            action: solarity_ui::UiMovementAction::CameraOrbitStop {
                sticky_camera: true
            },
            ..
        })
    ) {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            "Ctrl did not reach the stock sticky-camera release",
        )
        .into());
    }
    println!(
        "FrameXML startup, first updates, Escape menu toggles, and mouse bindings completed without errors"
    );
    Ok(())
}

fn argument_error(message: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!("{message}; usage: validate_frame_lifecycle <Data> <locale>"),
    )
}

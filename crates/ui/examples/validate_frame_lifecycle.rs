//! Checks stock FrameXML and optionally measures steady updates without a renderer.

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
    let benchmark_frames = arguments
        .next()
        .map(|value| value.to_string_lossy().parse::<std::num::NonZeroUsize>())
        .transpose()?;
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
    manager.dispatch_event(
        "CHAT_MSG_SYSTEM",
        &solarity_ui::UiEventPayload::new([
            solarity_ui::UiEventArgument::String("World chat initialized".to_owned()),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::Integer(0),
            solarity_ui::UiEventArgument::Integer(0),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::Integer(0),
            solarity_ui::UiEventArgument::Integer(1),
            solarity_ui::UiEventArgument::String(String::new()),
            solarity_ui::UiEventArgument::Integer(0),
        ]),
    )?;
    // Exercise the actual stock handlers with a resolved fixture combat event,
    // not just the empty history used during bootstrap.
    manager.append_combat_log(solarity_ui::UiCombatLogEntry::new(
        1.0,
        "SWING_DAMAGE",
        solarity_ui::UiCombatLogObject {
            guid: 1,
            name: Some("SolarityTester".to_owned()),
            flags: 0x511,
        },
        solarity_ui::UiCombatLogObject {
            guid: 0xf130_0000_0000_0007,
            name: Some("Training Dummy".to_owned()),
            flags: 0xa48,
        },
        None,
        solarity_ui::UiEventPayload::new([
            solarity_ui::UiEventArgument::Integer(87),
            solarity_ui::UiEventArgument::Integer(-1),
            solarity_ui::UiEventArgument::Integer(1),
            solarity_ui::UiEventArgument::Nil,
            solarity_ui::UiEventArgument::Nil,
            solarity_ui::UiEventArgument::Nil,
            solarity_ui::UiEventArgument::Nil,
            solarity_ui::UiEventArgument::Nil,
            solarity_ui::UiEventArgument::Nil,
        ]),
    )?)?;
    for (name, fraction) in [("PlayerFrameHealthBar", 1.0), ("PlayerFrameManaBar", 1.0)] {
        check_player_bar(&manager, name, fraction)?;
    }
    world.set_player_vitals(UiPlayerVitalsState::new(
        50,
        100,
        25,
        100,
        UiUnitPowerType::Mana,
    ));
    for event in ["UNIT_HEALTH", "UNIT_MANA"] {
        manager.dispatch_event(
            event,
            &solarity_ui::UiEventPayload::new([solarity_ui::UiEventArgument::String(
                "player".to_owned(),
            )]),
        )?;
    }
    check_player_bar(&manager, "PlayerFrameHealthBar", 0.5)?;
    check_player_bar(&manager, "PlayerFrameManaBar", 0.25)?;
    println!("Stock player health and mana textures rendered full and partial values");
    let chat_background = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .find(|texture| manager.object_name(texture.object_index()) == Some("ChatFrame1Background"))
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "stock chat background missing"))?;
    if (chat_background.bounds().width() - 434.0).abs() > 0.01
        || (chat_background.bounds().height() - 129.0).abs() > 0.01
        || chat_background
            .vertex_colors()
            .iter()
            .any(|color| color[..3] != [0.0, 0.0, 0.0])
    {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            "stock chat background dimensions or tint were not initialized",
        )
        .into());
    }
    println!("Stock chat background retained its 430x120 content size and black tint");
    check_chat_tabs(&manager)?;
    println!("Stock General and Combat Log tabs fit their labels without overlap");
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
    for (binding, inward) in [("CAMERAZOOMIN", true), ("CAMERAZOOMOUT", false)] {
        manager.invoke_binding(binding, true)?;
        if manager.take_movement_command()
            != Some(solarity_ui::UiMovementCommand {
                action: solarity_ui::UiMovementAction::CameraZoom { inward, amount: 1. },
                timestamp_ms: 1234,
            })
        {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                "stock wheel binding lost its zoom request",
            )
            .into());
        }
    }
    println!("Stock wheel zoom bindings emitted both timed distance requests");
    check_bag_hover_recovery(&mut manager)?;
    if let Some(count) = benchmark_frames {
        manager.set_modifier_keys(solarity_ui::UiModifierKeys::new(
            false, false, false, false, false, false,
        ));
        let mut durations = Vec::with_capacity(count.get());
        let mut dirty_frames = 0;
        let mut previous = std::time::Instant::now();
        for index in 0..count.get() + 32 {
            let started = std::time::Instant::now();
            let elapsed = started.duration_since(previous).as_secs_f64();
            previous = started;
            let changed = manager.update(elapsed)?;
            let duration = started.elapsed();
            if let Some(error) = manager.take_callback_failure() {
                return Err(IoError::other(error).into());
            }
            if index >= 32 {
                durations.push(duration);
                dirty_frames += usize::from(changed);
            }
        }
        let total: std::time::Duration = durations.iter().sum();
        durations.sort_unstable();
        let percentile =
            |percent: usize| durations[(durations.len() - 1) * percent / 100].as_secs_f64() * 1000.;
        println!(
            "FrameXML update samples={} dirty_frames={dirty_frames} mean_ms={:.6} p50_ms={:.6} p95_ms={:.6} p99_ms={:.6} max_ms={:.6}; fixture player, no GPU upload or rendering",
            count.get(),
            total.as_secs_f64() * 1000. / count.get() as f64,
            percentile(50),
            percentile(95),
            percentile(99),
            percentile(100)
        );
    }
    Ok(())
}

fn argument_error(message: &str) -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        format!("{message}; usage: validate_frame_lifecycle <Data> <locale> [benchmark frames]"),
    )
}

/// Replays the real bag tooltip boundary that previously trapped all later input.
fn check_bag_hover_recovery(manager: &mut FrameManager) -> Result<(), Box<dyn Error>> {
    let index = (0..manager.geometry().region_count())
        .find(|&index| manager.object_name(index) == Some("CharacterBag0Slot"))
        .ok_or_else(|| IoError::other("missing stock bag button"))?;
    let bounds = manager
        .geometry()
        .region(index)
        .ok_or_else(|| IoError::other("missing bag geometry"))?
        .presentation_bounds();
    let hit = manager.pointer_motion((
        (bounds.left() + bounds.right()) * 0.5,
        (bounds.bottom() + bounds.top()) * 0.5,
    ))?;
    if hit != Some(index) {
        return Err(IoError::other("stock bag did not receive pointer enter").into());
    }
    manager.pointer_motion((-10., -10.))?;
    let mut failures = 0;
    while let Some(error) = manager.take_callback_failure() {
        if !error.contains("SetInventoryItem") && !error.contains("ResetCursor") {
            return Err(IoError::other(error).into());
        }
        failures += 1;
    }
    for _ in 0..100 {
        manager.pointer_motion((-10., -10.))?;
        if let Some(error) = manager.take_callback_failure() {
            return Err(IoError::other(error).into());
        }
    }
    manager.invoke_binding("CAMERAORSELECTORMOVE", true)?;
    if manager.take_movement_command().is_none() {
        return Err(IoError::other("camera binding lost after bag hover").into());
    }
    manager.invoke_binding("CAMERAORSELECTORMOVE", false)?;
    if manager.take_movement_command().is_none() {
        return Err(IoError::other("camera release lost after bag hover").into());
    }
    println!(
        "Stock bag hover completed with {failures} contained tooltip callbacks; 100 later pointer events and camera press/release succeeded"
    );
    Ok(())
}

fn check_player_bar(
    manager: &FrameManager,
    name: &str,
    fraction: f64,
) -> Result<(), Box<dyn Error>> {
    let fill = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .find(|member| manager.object_name(member.owner_index()) == Some(name))
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, format!("missing {name} fill")))?;
    let owner = manager
        .geometry()
        .region(fill.owner_index())
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, format!("missing {name} geometry")))?;
    let bounds = owner.presentation_bounds();
    if fill.opacity() <= 0.0
        || bounds.width() <= 0.0
        || bounds.height() <= 0.0
        || (fill.bounds().width() - bounds.width() * fraction).abs() > 0.01
        || (fill.bounds().height() - bounds.height()).abs() > 0.01
    {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "incorrect {name} fill: {:?}, owner {bounds:?}, opacity {}",
                fill.bounds(),
                fill.opacity()
            ),
        )
        .into());
    }
    Ok(())
}

fn check_chat_tabs(manager: &FrameManager) -> Result<(), Box<dyn Error>> {
    let bounds = |name: &str| {
        (0..manager.geometry().region_count())
            .find(|&index| manager.object_name(index) == Some(name))
            .and_then(|index| manager.geometry().region(index))
            .map(solarity_ui::UiRegionGeometry::presentation_bounds)
            .ok_or_else(|| IoError::new(ErrorKind::InvalidData, format!("missing {name} geometry")))
    };
    let mut previous_right = None;
    for name in ["ChatFrame1Tab", "ChatFrame2Tab"] {
        let tab = bounds(name)?;
        let text = bounds(&format!("{name}Text"))?;
        if text.width() <= 1.0
            || text.left() < tab.left() - 0.01
            || text.right() > tab.right() + 0.01
            || previous_right.is_some_and(|right| tab.left() < right - 0.01)
        {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                format!("{name} does not fit its label: tab={tab:?}, text={text:?}"),
            )
            .into());
        }
        previous_right = Some(tab.right());
    }
    Ok(())
}

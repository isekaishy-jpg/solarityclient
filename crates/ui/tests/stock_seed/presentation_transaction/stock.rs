//! Full FrameXML exercises the publication boundary with the locally owned corpus.

use std::error::Error;
use std::time::Instant;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FrameManager, UiEventArgument, UiEventError, UiEventPayload, UiPlayerClassState,
    UiPlayerIdentityState, UiPlayerRaceState, UiPlayerState, UiPlayerVitalsState,
    UiScriptEnvironment, UiSpellBookTab, UiUnitPowerType,
};

/// Compares final geometry and named visibility after the actual first-login handlers.
/// Timings are diagnostic output only; machine speed never decides correctness.
#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn stock_framexml_entry_and_action_events_match_sequential_publication()
-> Result<(), Box<dyn Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let catalog = ArchiveCatalog::discover(ClientDataRoot::new(root)?, Locale::EnUs)?;
    let mut managers = Vec::new();
    for batched in [false, true] {
        let environment = UiScriptEnvironment::new(2560, 1440, false)?;
        let world = environment.world_state();
        world.enter_player(UiPlayerState::new(0));
        world.set_player_guid(1);
        world.set_player_identity(UiPlayerIdentityState::new("Soap", 80));
        world.set_player_class(UiPlayerClassState::new("Mage", "MAGE", 8));
        world.set_player_race(UiPlayerRaceState::new("Troll", "Troll", 8));
        world.set_zone(solarity_ui::UiZoneState::new(
            "Orgrimmar",
            "Orgrimmar",
            "Valley of Strength",
            "Valley of Strength",
            None,
            false,
            None,
        ));
        world.set_player_progression(solarity_ui::UiPlayerProgressionState::new(0, 1000));
        world.set_player_faction(solarity_ui::UiPlayerFactionState::new(
            solarity_ui::UiFactionGroup::Horde,
            "Horde",
        ));
        world.set_player_default_language(solarity_ui::UiPlayerLanguage::new(1, "Orcish"));
        world.set_realm_time(solarity_ui::UiRealmTime::new(4, 22)?);
        world.set_realm_date(solarity_ui::UiRealmDate::new(2, 9, 14, 2026)?);
        world.set_player_vitals(UiPlayerVitalsState::new(
            100,
            100,
            100,
            100,
            UiUnitPowerType::Mana,
        ));
        environment
            .spell_book_state()
            .set_tabs(vec![UiSpellBookTab::new(
                "General",
                "Interface\\Icons\\INV_Misc_QuestionMark",
                0,
                0,
            )]);
        let started = Instant::now();
        let mut source_duration = std::time::Duration::ZERO;
        let mut manager = if batched {
            let worker_catalog = catalog.clone();
            let sources = std::thread::spawn(move || {
                solarity_ui::FrameUiSources::load(&mut AssetStore::mount(worker_catalog)?)
            })
            .join()
            .map_err(|_| "source worker panicked")??;
            source_duration = started.elapsed();
            FrameManager::start_with_sources(
                AssetStoreHandle::new(AssetStore::mount(catalog.clone())?),
                environment,
                &[],
                &AddonCatalog::default(),
                sources,
            )?
        } else {
            FrameManager::start_shared(
                AssetStoreHandle::new(AssetStore::mount(catalog.clone())?),
                environment,
                &[],
                &AddonCatalog::default(),
            )?
        };
        let load = started.elapsed();
        let started = Instant::now();
        manager.with_suppressed_sound_entries(|manager| {
            if batched {
                manager.with_deferred_presentation(entry_events)?
            } else {
                entry_events(manager)
            }
        })?;
        let entry = started.elapsed();
        let started = Instant::now();
        if batched {
            manager.with_deferred_presentation(action_events)??;
        } else {
            action_events(&mut manager)?;
        }
        println!(
            "FrameXML batched={batched} load_ms={:.3} source_worker_ms={:.3} main_construction_ms={:.3} entry_ms={:.3} action_ms={:.3} objects={}",
            load.as_secs_f64() * 1000.,
            source_duration.as_secs_f64() * 1000.,
            (load - source_duration).as_secs_f64() * 1000.,
            entry.as_secs_f64() * 1000.,
            started.elapsed().as_secs_f64() * 1000.,
            manager.geometry().region_count()
        );
        managers.push(manager);
    }
    let [sequential, batched] = managers.as_slice() else {
        return Err("two managers required".into());
    };
    assert_eq!(
        sequential.geometry().region_count(),
        batched.geometry().region_count()
    );
    for index in 0..sequential.geometry().region_count() {
        assert_eq!(sequential.object_name(index), batched.object_name(index));
        let left = sequential
            .geometry()
            .region(index)
            .ok_or("sequential region")?;
        let right = batched.geometry().region(index).ok_or("batched region")?;
        assert_eq!(left.effectively_shown(), right.effectively_shown());
        assert_eq!(left.effective_alpha(), right.effective_alpha());
        assert_eq!(left.effective_scale(), right.effective_scale());
        assert_eq!(left.animation_active(), right.animation_active());
        for (left, right) in [
            (left.logical_bounds(), right.logical_bounds()),
            (left.presentation_bounds(), right.presentation_bounds()),
        ] {
            // Repeated native dimension publication can round an f64 layout
            // value differently. This tolerance is far below the f32 draw ABI.
            for (left, right) in [
                (left.left(), right.left()),
                (left.bottom(), right.bottom()),
                (left.right(), right.right()),
                (left.top(), right.top()),
            ] {
                assert!(
                    (left - right).abs() < 1e-8,
                    "region {index} {:?}: {left} != {right}",
                    sequential.object_name(index)
                );
            }
        }
    }
    assert_eq!(
        sequential.presentation().members_in_draw_order(),
        batched.presentation().members_in_draw_order(),
    );
    Ok(())
}

/// Executes the same ordered initial notifications as RuntimeWorldUi.
fn entry_events(manager: &mut FrameManager) -> Result<(), UiEventError> {
    for event in [
        "VARIABLES_LOADED",
        "UPDATE_CHAT_WINDOWS",
        "PLAYER_LOGIN",
        "UPDATE_BINDINGS",
        "PLAYER_ENTERING_WORLD",
    ] {
        manager.dispatch_event(event, &UiEventPayload::empty())?;
    }
    Ok(())
}

/// Exercises all authored action-slot subscribers against the same empty server image.
fn action_events(manager: &mut FrameManager) -> Result<(), UiEventError> {
    for index in 1..=144 {
        manager.dispatch_event(
            "ACTIONBAR_SLOT_CHANGED",
            &UiEventPayload::new([UiEventArgument::Integer(index)]),
        )?;
    }
    Ok(())
}

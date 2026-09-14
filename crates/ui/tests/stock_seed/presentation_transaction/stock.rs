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
    let mut capture = std::env::var_os("SOLARITY_UI_STARTUP_PROFILE_ROOT").map(|root| {
        solarity_profiling::Capture::new(
            std::path::Path::new(&root),
            "fixture=FrameXML startup".to_owned(),
        )
    });
    if let Some(capture) = &mut capture {
        let (_, path) = capture.toggle()?;
        println!("FrameXML profile={}", path.display());
    }
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
        let mut max_slice = std::time::Duration::ZERO;
        let mut slice_count = 0;
        let mut manager = if batched {
            let worker_catalog = catalog.clone();
            let sources = std::thread::spawn(move || {
                solarity_ui::FrameUiSources::load(&mut AssetStore::mount(worker_catalog)?)
            })
            .join()
            .map_err(|_| "source worker panicked")??;
            source_duration = started.elapsed();
            let mut construction = FrameManager::begin_with_sources(
                AssetStoreHandle::new(AssetStore::mount(catalog.clone())?),
                environment,
                &[],
                &AddonCatalog::default(),
                sources,
            );
            loop {
                let _frame = capture.as_ref().map(|_| solarity_profiling::begin_frame());
                let slice_start = Instant::now();
                let result = construction.advance(std::time::Duration::from_millis(2));
                max_slice = max_slice.max(slice_start.elapsed());
                slice_count += 1;
                if let std::task::Poll::Ready(result) = result {
                    break result?;
                }
            }
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
        let mut entry_max_slice = std::time::Duration::ZERO;
        let mut entry_slices = 0;
        if batched {
            let mut events = ENTRY_EVENTS.into_iter();
            let mut publication = manager.begin_suppressed_publication(move |manager| {
                let Some(event) = events.next() else {
                    return std::ops::ControlFlow::Break(Ok(()));
                };
                match manager.dispatch_event(event, &UiEventPayload::empty()) {
                    Ok(_) => std::ops::ControlFlow::Continue(()),
                    Err(error) => std::ops::ControlFlow::Break(Err(error)),
                }
            });
            loop {
                let _frame = capture.as_ref().map(|_| solarity_profiling::begin_frame());
                let slice_start = Instant::now();
                let result = publication.advance(std::time::Duration::from_millis(2));
                entry_max_slice = entry_max_slice.max(slice_start.elapsed());
                entry_slices += 1;
                if let std::task::Poll::Ready(result) = result {
                    let (published, callbacks) = result?;
                    callbacks?;
                    manager = published;
                    break;
                }
            }
            assert!(entry_slices > 2);
        } else {
            manager.with_suppressed_sound_entries(entry_events)?;
        }
        let entry = started.elapsed();
        let started = Instant::now();
        if batched {
            manager.with_deferred_presentation(action_events)??;
        } else {
            action_events(&mut manager)?;
        }
        println!(
            "FrameXML batched={batched} load_ms={:.3} source_worker_ms={:.3} main_construction_ms={:.3} max_slice_ms={:.3} slices={} entry_ms={:.3} entry_max_slice_ms={:.3} entry_slices={} action_ms={:.3} objects={}",
            load.as_secs_f64() * 1000.,
            source_duration.as_secs_f64() * 1000.,
            (load - source_duration).as_secs_f64() * 1000.,
            max_slice.as_secs_f64() * 1000.,
            slice_count,
            entry.as_secs_f64() * 1000.,
            entry_max_slice.as_secs_f64() * 1000.,
            entry_slices,
            started.elapsed().as_secs_f64() * 1000.,
            manager.geometry().region_count()
        );
        managers.push(manager);
    }
    if let Some(capture) = &mut capture {
        capture.shutdown()?;
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

/// Stock initial notifications, shared by the sequential and staged paths.
const ENTRY_EVENTS: [&str; 5] = [
    "VARIABLES_LOADED",
    "UPDATE_CHAT_WINDOWS",
    "PLAYER_LOGIN",
    "UPDATE_BINDINGS",
    "PLAYER_ENTERING_WORLD",
];

/// Executes the same ordered initial notifications as RuntimeWorldUi.
fn entry_events(manager: &mut FrameManager) -> Result<(), UiEventError> {
    for event in ENTRY_EVENTS {
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

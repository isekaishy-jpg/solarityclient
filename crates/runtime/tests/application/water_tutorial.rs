//! Ordered timer/tutorial input and encrypted completion acknowledgements.

use solarity_asset::{
    ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale, SpellNameCatalog,
};
use solarity_ui::{
    AddonCatalog, FrameManager, UiGlueMediaAction, UiScriptEnvironment, UiTutorialAction,
};

use crate::application::gameplay_coordinator::player_ui::{
    RuntimePlayerUiNotification, RuntimePlayerUiState,
};
use crate::test_network::{TestError, WorldServer};

thread_local! { static STOCK_NOW: std::cell::Cell<u32> = const { std::cell::Cell::new(1000) }; }
fn stock_now() -> u32 {
    STOCK_NOW.get()
}

#[test]
fn water_tutorial_combat_lockdown_matches_original_callbacks()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = super::tests::fixture_with(&[
        ("Interface/FrameXML/FrameXML.toc", b"Combat.xml\n"),
        ("Interface/FrameXML/Combat.xml", br#"<Ui><Frame name="Observer"><Scripts>
<OnLoad>self:RegisterEvent('PLAYER_REGEN_DISABLED');self:RegisterEvent('PLAYER_REGEN_ENABLED');LOG=''</OnLoad>
<OnEvent>LOG=event..':'..tostring(InCombatLockdown())</OnEvent>
</Scripts></Frame></Ui>"#),
        ("Interface/FrameXML/Bindings.xml", br#"<Bindings>
<Binding name="QUERY">QUERY=tostring(InCombatLockdown())</Binding>
</Bindings>"#),
    ])?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?;
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    let mut active = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(0),
        1,
        "WaterTest",
        glam::Vec3::ZERO,
        0.0,
    ));
    let mut player_ui = RuntimePlayerUiState::default();
    let mut previous_log = String::new();
    for line in include_str!("../fixtures/player_combat_lockdown.txt").lines() {
        let row = line
            .split_ascii_whitespace()
            .map(str::parse::<u32>)
            .collect::<Result<Vec<_>, _>>()?;
        let entity = active.local_player();
        active
            .storage_mut()
            .add_component(entity, (solarity_ecs::UnitFlags::new(row[0], 0, 0),));
        player_ui.observe_combat(&active);
        if row[2] == u32::MAX {
            assert_eq!(player_ui.take_notification(), None);
        } else {
            let Some(RuntimePlayerUiNotification::Combat(in_combat)) =
                player_ui.take_notification()
            else {
                panic!("expected native combat event");
            };
            manager.player_combat_changed(in_combat)?;
            previous_log = format!(
                "{}:{}",
                if row[2] == 152 {
                    "PLAYER_REGEN_DISABLED"
                } else {
                    "PLAYER_REGEN_ENABLED"
                },
                if row[3] == 0 { "nil" } else { "1" }
            );
        }
        assert_eq!(
            manager.localized_text("LOG")?.unwrap_or_default(),
            previous_log
        );
        manager.invoke_binding("QUERY", true)?;
        assert_eq!(
            manager.localized_text("QUERY")?.as_deref(),
            Some(if row[4] == 0 { "nil" } else { "1" })
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn stock_water_tutorial_opens_completes_and_queues_related_prompts()
-> Result<(), Box<dyn std::error::Error>> {
    use solarity_ui::{
        UiFactionGroup, UiPlayerClassState, UiPlayerFactionState, UiPlayerIdentityState,
        UiPlayerLanguage, UiPlayerProgressionState, UiPlayerRaceState, UiPlayerState,
        UiPlayerVitalsState, UiRealmDate, UiRealmTime, UiSpellBookTab, UiUnitPowerType,
        UiZoneState,
    };
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let spell_names = SpellNameCatalog::load(&mut store)?;
    STOCK_NOW.set(1000);
    let environment = UiScriptEnvironment::new(1920, 1080, false)?
        .with_client_clock(solarity_ui::UiClientClock::from_source(stock_now));
    let world = environment.world_state();
    world.enter_player(UiPlayerState::new(0));
    world.set_player_guid(1);
    world.set_player_identity(UiPlayerIdentityState::new("WaterTutorialTest", 1));
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
    world.tutorials().replace_flags(&[0; 32]);
    environment.action_bar_state().set_slots([0; 144]);
    environment
        .spell_book_state()
        .set_tabs(vec![UiSpellBookTab::new(
            "General",
            "Interface/Icons/INV_Misc_QuestionMark",
            0,
            0,
        )]);
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    for event in [
        "VARIABLES_LOADED",
        "UPDATE_CHAT_WINDOWS",
        "PLAYER_LOGIN",
        "UPDATE_BINDINGS",
        "PLAYER_ENTERING_WORLD",
    ] {
        manager.dispatch_event(event, &solarity_ui::UiEventPayload::empty())?;
    }
    while manager.take_media_action().is_some() {}
    world.set_combat_lockdown(true);
    manager.trigger_tutorial(27)?;
    assert_eq!(manager.region_is_shown("TutorialFrame"), Some(false));
    assert_eq!(
        manager.region_is_shown("TutorialFrameAlertButton"),
        Some(true)
    );
    assert!(!world.tutorials().is_flagged(27));
    manager.player_combat_changed(false)?;
    manager.trigger_tutorial(26)?;
    assert_eq!(manager.region_is_shown("TutorialFrame"), Some(true));
    assert!(world.tutorials().is_flagged(27));
    assert_eq!(
        world.tutorials().pending_action(),
        Some(UiTutorialAction::Flag(27))
    );
    world.tutorials().accept_action();
    manager.trigger_tutorial(28)?;
    assert_eq!(
        manager.region_is_shown("TutorialFrameAlertButton"),
        Some(true)
    );
    assert_eq!(
        manager.region_is_shown("TutorialFrameAlertButtonBadge"),
        // Build 12340 deliberately disables the badge in TutorialFrame_CheckBadge.
        Some(false)
    );
    assert!(!world.tutorials().is_flagged(26));
    assert!(!world.tutorials().is_flagged(28));
    assert_eq!(world.tutorials().pending_action(), None);
    manager.update(1.0 / 60.0)?;
    let mut active = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(0),
        1,
        "WaterTutorialTest",
        glam::Vec3::ZERO,
        0.0,
    ));
    active.create_object(
        1,
        solarity_ecs::ObjectKind::Player,
        None,
        [(24, 100), (32, 100)],
    )?;
    let mut state = RuntimePlayerUiState::default();
    for (kind, amount, absorbed, resisted) in [
        (1, 20, 0, 0),
        (0, 20, 0, 0),
        (3, 5, 0, 0),
        (4, 5, 0, 0),
        (1, 0, 10, 0),
        (1, 0, 0, 10),
    ] {
        STOCK_NOW.set(1000);
        state.receive_environmental_damage(
            &mut active,
            solarity_network::WorldEnvironmentalDamage {
                guid: 1,
                kind,
                amount,
                absorbed,
                resisted,
            },
            Some("WaterTutorialTest".into()),
            None,
            None,
            1000,
        );
        while let Some(notification) = state.take_notification() {
            match notification {
                RuntimePlayerUiNotification::ResurrectionOffer { offer, name } => {
                    world.set_resurrection_offer(offer);
                    if let Some(name) = name {
                        manager.dispatch_event(
                            "RESURRECT_REQUEST",
                            &solarity_ui::UiEventPayload::new([
                                solarity_ui::UiEventArgument::String(name),
                            ]),
                        )?;
                    }
                }
                RuntimePlayerUiNotification::CorpseRecovery(corpse) => {
                    world.set_corpse_state(corpse);
                    if let Some(event) = corpse.range_event() {
                        manager.dispatch_event(event, &solarity_ui::UiEventPayload::empty())?;
                    }
                }
                RuntimePlayerUiNotification::UnitDeath(snapshot) => {
                    super::super::environmental_damage::dispatch_unit_death(
                        &mut manager,
                        &world,
                        snapshot,
                    )?
                }
                RuntimePlayerUiNotification::Life {
                    snapshot,
                    event,
                    release_timer,
                } => super::dispatch_life(&mut manager, &world, snapshot, event, release_timer)?,
                RuntimePlayerUiNotification::Health {
                    snapshot,
                    health_changed,
                    maximum_changed,
                } => super::dispatch_health(
                    &mut manager,
                    &world,
                    snapshot,
                    health_changed,
                    maximum_changed,
                )?,
                RuntimePlayerUiNotification::EnvironmentalDamage(impact) => {
                    super::super::environmental_damage::dispatch_environmental_damage(
                        &mut manager,
                        impact,
                    )?
                }
                _ => return Err("unexpected damage notification".into()),
            }
        }
        assert_eq!(manager.region_is_shown("PlayerHitIndicator"), Some(true));
        STOCK_NOW.set(1200);
        manager.update(0.2)?;
        assert_eq!(manager.region_is_shown("PlayerHitIndicator"), Some(true));
        assert!(manager.take_callback_failure().is_none());
        STOCK_NOW.set(2201);
        manager.update(1.001)?;
        assert_eq!(manager.region_is_shown("PlayerHitIndicator"), Some(false));
    }
    assert!(manager.take_callback_failure().is_none());
    // Fatal replicated health opens the original FrameXML death dialog.
    active.update_fields(1, [(24, 0), (1197, 8)])?;
    solarity_systems::project_object_fields(&mut active, 1, [(24, 0), (1197, 8)])?;
    state.receive_unit_field(
        &active,
        active.object_identity(1).ok_or("player identity")?,
        crate::application::gameplay_session::UnitFieldNotification::Health { previous: 100 },
        2201,
    );
    while let Some(notification) = state.take_notification() {
        match notification {
            RuntimePlayerUiNotification::UnitDeath(snapshot) => {
                super::super::environmental_damage::dispatch_unit_death(
                    &mut manager,
                    &world,
                    snapshot,
                )?
            }
            RuntimePlayerUiNotification::Life {
                snapshot,
                event,
                release_timer,
            } => super::dispatch_life(&mut manager, &world, snapshot, event, release_timer)?,
            RuntimePlayerUiNotification::Health {
                snapshot,
                health_changed,
                maximum_changed,
            } => super::dispatch_health(
                &mut manager,
                &world,
                snapshot,
                health_changed,
                maximum_changed,
            )?,
            _ => return Err("unexpected death notification".into()),
        }
    }
    assert_eq!(manager.take_callback_failure(), None);
    assert_eq!(manager.region_is_shown("StaticPopup1"), Some(true));
    assert_eq!(manager.region_is_shown("StaticPopup1Button2"), Some(false));
    let click_button =
        |manager: &mut FrameManager, name: &str| -> Result<(), Box<dyn std::error::Error>> {
            let index = (0..manager.geometry().region_count())
                .find(|&i| manager.object_name(i) == Some(name))
                .ok_or("release button")?;
            let bounds = manager
                .geometry()
                .region(index)
                .ok_or("release bounds")?
                .presentation_bounds();
            let point = (
                (bounds.left() + bounds.right()) / 2.,
                (bounds.bottom() + bounds.top()) / 2.,
            );
            manager.pointer_button(point, solarity_ui::UiPointerButton::Left, true)?;
            manager.pointer_button(point, solarity_ui::UiPointerButton::Left, false)?;
            Ok(())
        };
    world.set_falling(true);
    manager.update(0.1)?;
    click_button(&mut manager, "StaticPopup1Button1")?;
    assert!(world.pending_death_action().is_none());
    assert_eq!(manager.region_is_shown("StaticPopup1"), Some(true));
    world.set_falling(false);
    manager.update(0.1)?;
    click_button(&mut manager, "StaticPopup1Button1")?;
    assert_eq!(manager.take_callback_failure(), None);
    assert!(world.pending_death_action().is_some());
    assert_eq!(manager.region_is_shown("StaticPopup1"), Some(false));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, session) = WorldServer::connect().await?;
            let (_, mut writer) = session.split();
            let received = server.exchange_raw(vec![], 1).await?;
            writer.send_release_spirit(false).await?;
            world.accept_death_action();
            assert_eq!(received.await??, vec![(0x15a, vec![0])]);
            Ok::<_, TestError>(())
        })
        .map_err(|error| error.to_string())?;
    assert!(world.pending_death_action().is_none());
    active.update_fields(1, [(1199, 21169)])?;
    state.receive_death_notice(&active, 2201);
    while let Some(notification) = state.take_notification() {
        match notification {
            RuntimePlayerUiNotification::UnitDeath(snapshot) => {
                super::super::environmental_damage::dispatch_unit_death(
                    &mut manager,
                    &world,
                    snapshot,
                )?
            }
            RuntimePlayerUiNotification::Resurrection(snapshot) => {
                snapshot.publish(&world, &spell_names)
            }
            RuntimePlayerUiNotification::Life {
                snapshot,
                event,
                release_timer,
            } => super::dispatch_life(&mut manager, &world, snapshot, event, release_timer)?,
            _ => return Err("unexpected self-resurrection notification".into()),
        }
    }
    assert_eq!(manager.region_is_shown("StaticPopup1"), Some(true));
    assert_eq!(manager.region_is_shown("StaticPopup1Button2"), Some(true));
    assert_eq!(
        world.resurrection_state().self_resurrection_name.as_deref(),
        spell_names.name(21169)
    );
    manager.update(0.1)?;
    click_button(&mut manager, "StaticPopup1Button2")?;
    assert_eq!(
        world.pending_death_action(),
        Some(solarity_ui::UiPlayerDeathAction::SelfResurrect)
    );
    assert_eq!(manager.region_is_shown("StaticPopup1"), Some(false));
    assert_eq!(manager.take_callback_failure(), None);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let (server, session) = WorldServer::connect().await?;
            let (_, mut writer) = session.split();
            let received = server.exchange_raw(vec![], 1).await?;
            writer.send_self_resurrect().await?;
            world.accept_death_action();
            assert_eq!(received.await??, vec![(0x2b3, vec![])]);
            Ok::<_, TestError>(())
        })
        .map_err(|error| error.to_string())?;
    for (index, sickness, timer, delay) in [(0, 1, 1, 1000), (1, 0, 1, 1000), (2, 0, 0, 30000)] {
        STOCK_NOW.set(10_000);
        state.receive_resurrection(
            &active,
            solarity_network::WorldPlayerResurrection::RecoveryDelay(delay),
            STOCK_NOW.get(),
        );
        state.receive_resurrection(
            &active,
            solarity_network::WorldPlayerResurrection::Offer {
                guid: 0x1234567800000009 + index,
                name: "Water Healer".into(),
                sickness,
                timer,
            },
            STOCK_NOW.get(),
        );
        while let Some(notification) = state.take_notification() {
            match notification {
                RuntimePlayerUiNotification::CorpseRecovery(corpse) => {
                    world.set_corpse_state(corpse)
                }
                RuntimePlayerUiNotification::ResurrectionOffer {
                    offer,
                    name: Some(name),
                } => {
                    world.set_resurrection_offer(offer);
                    manager.dispatch_event(
                        "RESURRECT_REQUEST",
                        &solarity_ui::UiEventPayload::new([solarity_ui::UiEventArgument::String(
                            name,
                        )]),
                    )?;
                }
                _ => return Err("resurrection notification".into()),
            }
        }
        manager.update(0.1)?;
        assert_eq!(manager.region_is_shown("StaticPopup1"), Some(true));
        assert_eq!(manager.region_is_shown("StaticPopup1Button2"), Some(true));
        click_button(&mut manager, "StaticPopup1Button1")?;
        if sickness != 0 || timer != 0 {
            assert!(
                world.pending_death_action().is_none(),
                "stock accept button waits for recovery"
            );
            STOCK_NOW.set(12_000);
            manager.update(2.0)?;
            click_button(&mut manager, "StaticPopup1Button1")?;
        }
        assert_eq!(
            world.pending_death_action(),
            Some(solarity_ui::UiPlayerDeathAction::ResurrectionResponse {
                guid: 0x1234567800000009 + index,
                accept: true
            })
        );
        world.accept_death_action();
        assert_eq!(world.resurrection_offer().guid, 0);
        assert_eq!(manager.region_is_shown("StaticPopup1"), Some(false));
        assert_eq!(manager.take_callback_failure(), None);
    }
    world.set_resurrection_offer(solarity_ui::UiPlayerResurrectionOffer {
        guid: 14,
        sickness: 0,
        timer: 0,
    });
    manager.dispatch_event(
        "RESURRECT_REQUEST",
        &solarity_ui::UiEventPayload::new([solarity_ui::UiEventArgument::String(
            "Another Healer".into(),
        )]),
    )?;
    manager.update(0.1)?;
    click_button(&mut manager, "StaticPopup1Button2")?;
    assert_eq!(
        world.pending_death_action(),
        Some(solarity_ui::UiPlayerDeathAction::ResurrectionResponse {
            guid: 14,
            accept: false
        })
    );
    world.accept_death_action();
    assert_eq!(world.resurrection_offer().guid, 0);
    assert_eq!(
        manager.region_is_shown("StaticPopup2"),
        Some(true),
        "decline restores the original death dialog"
    );
    assert_eq!(manager.take_callback_failure(), None);
    Ok(())
}

#[test]
fn water_tutorials_preserve_server_order_native_callbacks_and_wire_acknowledgements()
-> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let fixture = super::tests::fixture_with(&[
            ("Interface/FrameXML/FrameXML.toc", b"Tutorial.xml\n"),
            ("Interface/FrameXML/Tutorial.xml", br#"<Ui><Frame name="Observer"><Scripts>
<OnLoad>self:RegisterEvent('MIRROR_TIMER_START'); self:RegisterEvent('TUTORIAL_TRIGGER'); LOG=''</OnLoad>
<OnEvent>
 if event == 'MIRROR_TIMER_START' then LOG=LOG..'mirror:'..arg1..';' end
 if event == 'TUTORIAL_TRIGGER' then
  LOG=LOG..'tutorial:'..arg1..':'..tostring(IsTutorialFlagged(arg1))..':'..tostring(CanResetTutorials())..';'
  FlagTutorial(arg1)
 end
</OnEvent></Scripts></Frame></Ui>"#),
            ("Interface/FrameXML/Bindings.xml", br#"<Bindings>
<Binding name="DISABLE">SetCVar('showTutorials',0)</Binding>
<Binding name="ENABLE">SetCVar('showTutorials',1)</Binding>
<Binding name="CLEAR">ClearTutorials()</Binding>
<Binding name="RESET">ResetTutorials()</Binding>
</Bindings>"#),
        ]).map_err(|error| error.to_string())?;
        let mut store=AssetStore::mount(ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?,Locale::EnUs)?)?;
        let names=SpellNameCatalog::load(&mut store)?;
        let environment=UiScriptEnvironment::new(800,600,false)?;
        let world=environment.world_state();
        let tutorials=world.tutorials();
        let mut manager=FrameManager::start_shared(AssetStoreHandle::new(store),environment,&[],&AddonCatalog::default())?;
        let start = |timer: u32| {
            let mut body=Vec::new();
            for word in [timer,60000,60000,u32::MAX] { body.extend_from_slice(&word.to_le_bytes()); }
            body.push(0);body.extend_from_slice(&0_u32.to_le_bytes());(0x1d9,body)
        };
        let (server,network)=WorldServer::connect().await?;
        let (mut reader,mut writer)=network.split();
        let sent=server.exchange(vec![start(1),(0xfd,vec![0;32]),start(1),start(1),(0xfd,vec![0;32]),start(0)],0).await?;
        let mut state=RuntimePlayerUiState::default();
        for _ in 0..6 {
            let packet=reader.receive_packet().await?;
            if let Some(flags)=packet.tutorial_flags() { state.receive_tutorial_flags(flags); }
            else { state.receive(packet.mirror_timer()?.ok_or("timer packet")?,1000); }
        }
        sent.await??;
        while let Some(notification)=state.take_notification() {
            match notification {
RuntimePlayerUiNotification::UnitDeath(snapshot) => super::super::environmental_damage::dispatch_unit_death(&mut manager, &world, snapshot)?,
                RuntimePlayerUiNotification::Resurrection(snapshot) => snapshot.publish(&world, &names),
                RuntimePlayerUiNotification::DeathAction(action) => world.queue_death_action(action),
                RuntimePlayerUiNotification::PlayerAuras => { manager.dispatch_event("UNIT_AURA", &solarity_ui::UiEventPayload::new([solarity_ui::UiEventArgument::String("player".into())]))?; }
                RuntimePlayerUiNotification::Life {snapshot,event,release_timer} => super::dispatch_life(&mut manager,&world,snapshot,event,release_timer)?,
                RuntimePlayerUiNotification::Combat(in_combat) => manager.player_combat_changed(in_combat)?,
                RuntimePlayerUiNotification::EnvironmentalDamage(impact) => super::super::environmental_damage::dispatch_environmental_damage(&mut manager,impact)?,
                RuntimePlayerUiNotification::Health {snapshot,health_changed,maximum_changed} => super::dispatch_health(&mut manager,&world,snapshot,health_changed,maximum_changed)?,
                RuntimePlayerUiNotification::TutorialFlags(flags) => tutorials.replace_flags(&flags),
                RuntimePlayerUiNotification::MirrorTimer(timer) => super::dispatch_notification(&mut manager,&world,&names,timer)?,
                RuntimePlayerUiNotification::Attack(started) => { manager.dispatch_event(if started { "PLAYER_ENTER_COMBAT" } else { "PLAYER_LEAVE_COMBAT" }, &solarity_ui::UiEventPayload::empty())?; }
                RuntimePlayerUiNotification::ResurrectionOffer { offer, name } => {
                    world.set_resurrection_offer(offer);
                    if let Some(name) = name { manager.dispatch_event("RESURRECT_REQUEST", &solarity_ui::UiEventPayload::new([solarity_ui::UiEventArgument::String(name)]))?; }
                }
                RuntimePlayerUiNotification::CorpseRecovery(corpse) => {
                    world.set_corpse_state(corpse);
                    if let Some(event) = corpse.range_event() { manager.dispatch_event(event, &solarity_ui::UiEventPayload::empty())?; }
                }
            }
        }
        assert_eq!(manager.localized_text("LOG")?.as_deref(),Some("mirror:BREATH;mirror:BREATH;tutorial:29:nil:nil;mirror:BREATH;mirror:EXHAUSTION;tutorial:27:nil:nil;"));
        assert_eq!(manager.take_media_action(),Some(UiGlueMediaAction::PlaySound("TutorialPopup".into())));
        assert_eq!(manager.take_media_action(),Some(UiGlueMediaAction::PlaySound("TutorialPopup".into())));
        assert!(manager.take_media_action().is_none());
        manager.invoke_binding("DISABLE",true)?;
        manager.trigger_tutorial(27)?;
        assert!(tutorials.is_flagged(27));
        assert!(manager.take_media_action().is_none());
        manager.invoke_binding("ENABLE",true)?;
        manager.trigger_tutorial(27)?;
        assert!(manager.take_media_action().is_none());
        manager.invoke_binding("CLEAR",true)?;
        manager.invoke_binding("RESET",true)?;
        let expected=vec![(0xfe,28_u32.to_le_bytes().to_vec()),(0xfe,26_u32.to_le_bytes().to_vec()),(0xfe,27_u32.to_le_bytes().to_vec()),(0xff,vec![]),(0x100,vec![])];
        let received=server.exchange_raw(vec![],expected.len()).await?;
        while let Some(action)=tutorials.pending_action() {
            match action {
                UiTutorialAction::Flag(index) => writer.send_tutorial_flag(index).await?,
                UiTutorialAction::Clear => writer.send_tutorial_clear().await?,
                UiTutorialAction::Reset => writer.send_tutorial_reset().await?,
            }
            tutorials.accept_action();
        }
        assert_eq!(received.await??,expected);
        assert!(manager.take_callback_failure().is_none());
        Ok(())
    })
}

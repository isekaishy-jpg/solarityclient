//! Native Lua health values and ordered replication/prediction presentation.

use crate::application::gameplay_coordinator::player_ui::{
    RuntimePlayerHealthSnapshot, RuntimePlayerUiNotification, RuntimePlayerUiState,
};

#[test]
fn player_health_prediction_matches_native_signed_clamps_and_immunity()
-> Result<(), Box<dyn std::error::Error>> {
    for line in include_str!("../fixtures/player_health_native.prediction.txt").lines() {
        let row: Vec<i64> = line
            .split_ascii_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        assert_eq!(
            solarity_systems::predict_unit_health(
                row[0] as i32,
                row[1] as i32,
                row[2] as u32,
                row[3] as i32
            ) as u32,
            row[4] as u32,
            "{line}"
        );
    }
    Ok(())
}
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FrameManager, UiPlayerState, UiPlayerVitalsState, UiScriptEnvironment,
    UiUnitPowerType,
};

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn stock_player_health_bar_polls_prediction_and_reconciles_replication()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut stock = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let paths = [
        "Interface/FrameXML/UnitFrame.lua",
        "Interface/FrameXML/TextStatusBar.lua",
        "Fonts/FRIZQT__.TTF",
        "Interface/TargetingFrame/UI-StatusBar.blp",
    ];
    let reads = paths
        .iter()
        .map(|path| stock.read(&solarity_asset::AssetPath::new(path)?))
        .collect::<Result<Vec<_>, solarity_asset::AssetError>>()?;
    let mut files = paths
        .iter()
        .zip(&reads)
        .map(|(path, read)| (*path, read.bytes()))
        .collect::<Vec<_>>();
    files.extend_from_slice(&[
        ("Interface/FrameXML/FrameXML.toc",b"TextStatusBar.lua\nUnitFrame.lua\nHealth.xml\n"),
        ("Interface/FrameXML/Health.xml",br#"<Ui>
<StatusBar name="HealthBar"><Size x="200" y="20"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer><FontString name="HealthText" font="Fonts/FRIZQT__.TTF"><FontHeight><AbsValue val="12"/></FontHeight></FontString></Layer></Layers>
<BarTexture file="Interface/TargetingFrame/UI-StatusBar"/><Scripts><OnLoad>
TextStatusBar_Initialize(self); self.forceShow=true; self.showNumeric=true;
UnitFrameHealthBar_Initialize('player',self,HealthText,true); UnitFrameHealthBar_Update(self,'player')
</OnLoad></Scripts></StatusBar></Ui>"#),
        ("Interface/FrameXML/Bindings.xml",br#"<Bindings>
<Binding name="QUERY">QUERY=HealthBar:GetValue()..'|'..HealthText:GetText()</Binding>
<Binding name="REPLICATE">SetCVar('predictedHealth',0);UnitFrameHealthBar_OnEvent(HealthBar,'VARIABLES_LOADED')</Binding>
</Bindings>"#),
    ]);
    let fixture = super::tests::fixture_with(&files)?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?;
    let world = environment.world_state();
    world.enter_player(UiPlayerState::new(0));
    world.set_player_guid(1);
    world.set_player_vitals(UiPlayerVitalsState::new(
        500,
        1000,
        70,
        100,
        UiUnitPowerType::Mana,
    ));
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    manager.dispatch_event("VARIABLES_LOADED", &solarity_ui::UiEventPayload::empty())?;
    for (health, maximum, predicted, health_changed, maximum_changed, expected) in [
        (500, 1000, 400, false, false, "400|400 / 1000"),
        (380, 1000, 380, true, false, "380|380 / 1000"),
        (380, 1200, 380, false, true, "380|380 / 1200"),
        (380, 1200, 1, false, false, "1|1 / 1200"),
        (0, 1200, 0, true, false, "0|0 / 1200"),
    ] {
        super::dispatch_health(
            &mut manager,
            &world,
            RuntimePlayerHealthSnapshot {
                health,
                maximum,
                predicted,
                ghost: false,
            },
            health_changed,
            maximum_changed,
        )?;
        manager.update(1.0 / 60.0)?;
        manager.invoke_binding("QUERY", true)?;
        assert_eq!(manager.localized_text("QUERY")?.as_deref(), Some(expected));
    }
    manager.invoke_binding("REPLICATE", true)?;
    super::dispatch_health(
        &mut manager,
        &world,
        RuntimePlayerHealthSnapshot {
            health: 750,
            maximum: 1200,
            predicted: 400,
            ghost: false,
        },
        true,
        false,
    )?;
    manager.update(1.0 / 60.0)?;
    manager.invoke_binding("QUERY", true)?;
    assert_eq!(
        manager.localized_text("QUERY")?.as_deref(),
        Some("750|750 / 1200")
    );
    assert!(manager.take_callback_failure().is_none());
    Ok(())
}

#[test]
fn player_health_lua_matches_native_signed_prediction_and_life_queries()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = super::tests::fixture_with(&[
        ("Interface/FrameXML/FrameXML.toc", b"Health.xml\n"),
        ("Interface/FrameXML/Health.xml", br#"<Ui><Frame name="HealthObserver"><Scripts>
<OnLoad>LOG='';self:RegisterEvent('UNIT_HEALTH');self:RegisterEvent('UNIT_MAXHEALTH')</OnLoad>
<OnEvent>LOG=LOG..event..':'..(...)..':'..UnitHealth('player')..':'..UnitHealthMax('player')..'|'</OnEvent>
</Scripts></Frame></Ui>"#),
        ("Interface/FrameXML/Bindings.xml", br#"<Bindings>
<Binding name="QUERY">QUERY=UnitHealth('player')..' '..UnitHealthMax('player')..' '..tostring(UnitIsDead('player'))..' '..tostring(UnitIsGhost('player'))</Binding>
<Binding name="PREDICT">SetCVar('predictedHealth',1)</Binding>
<Binding name="REPLICATE">SetCVar('predictedHealth',0)</Binding>
</Bindings>"#),
    ])?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?;
    let world = environment.world_state();
    world.enter_player(UiPlayerState::new(0));
    world.set_player_guid(1);
    world.set_player_vitals(UiPlayerVitalsState::new(
        500,
        1000,
        70,
        100,
        UiUnitPowerType::Mana,
    ));
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    for line in include_str!("../fixtures/player_health_native.txt").lines() {
        let row: Vec<_> = line.split_ascii_whitespace().collect();
        let snapshot = RuntimePlayerHealthSnapshot {
            health: row[0].parse()?,
            maximum: row[1].parse()?,
            predicted: row[2].parse()?,
            ghost: row[4] != "0",
        };
        snapshot.publish(&world);
        manager.invoke_binding(
            if row[3] == "0" {
                "REPLICATE"
            } else {
                "PREDICT"
            },
            true,
        )?;
        manager.invoke_binding("QUERY", true)?;
        assert_eq!(
            manager.localized_text("QUERY")?,
            Some(row[6..].join(" ")),
            "{line}"
        );
        assert_eq!(world.player_vitals().ok_or("vitals")?.power(), 70);
    }
    manager.invoke_binding("PREDICT", true)?;
    let snapshot = RuntimePlayerHealthSnapshot {
        health: 500,
        maximum: 1000,
        predicted: 400,
        ghost: false,
    };
    super::dispatch_health(&mut manager, &world, snapshot, false, false)?;
    assert_eq!(manager.localized_text("LOG")?.unwrap_or_default(), "");
    super::dispatch_health(
        &mut manager,
        &world,
        RuntimePlayerHealthSnapshot {
            health: 400,
            predicted: 400,
            ..snapshot
        },
        true,
        false,
    )?;
    super::dispatch_health(
        &mut manager,
        &world,
        RuntimePlayerHealthSnapshot {
            health: 400,
            predicted: 400,
            maximum: 1200,
            ..snapshot
        },
        false,
        true,
    )?;
    assert_eq!(
        manager.localized_text("LOG")?.as_deref(),
        Some("UNIT_HEALTH:player:400:1000|UNIT_MAXHEALTH:player:400:1200|")
    );
    Ok(())
}

#[test]
fn player_health_reconciles_only_changed_health_and_retains_notification_order()
-> Result<(), Box<dyn std::error::Error>> {
    let mut world = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
        solarity_ecs::WorldMapId::new(0),
        1,
        "WaterTest",
        glam::Vec3::ZERO,
        0.0,
    ));
    world.create_object(1, solarity_ecs::ObjectKind::Player, None, [])?;
    solarity_systems::project_object_fields(&mut world, 1, [(24, 500), (32, 1000)])?;
    let mut state = RuntimePlayerUiState::default();
    state.refresh_health(&world);
    state.discard_published_notifications();
    let entity = world.local_player();
    world
        .storage_mut()
        .add_component(entity, (solarity_ecs::UnitHealthPrediction::new(400),));
    state.refresh_health(&world);
    for fields in [vec![(25, 60)], vec![(24, 500)], vec![(32, 1200)]] {
        let maximum_changed = fields[0].0 == 32;
        solarity_systems::project_object_fields(&mut world, 1, fields)?;
        assert_eq!(
            world
                .unit_health_prediction(1)
                .ok_or("prediction")?
                .health(),
            400
        );
        if maximum_changed {
            state.receive_unit_field(
                &world,
                world.object_identity(1).ok_or("identity")?,
                crate::application::gameplay_session::UnitFieldNotification::MaximumHealth,
                0,
            );
        }
        state.refresh_health(&world);
    }
    solarity_systems::project_object_fields(&mut world, 1, [(24, 380)])?;
    assert_eq!(
        world
            .unit_health_prediction(1)
            .ok_or("prediction")?
            .health(),
        380
    );
    state.receive_unit_field(
        &world,
        world.object_identity(1).ok_or("identity")?,
        crate::application::gameplay_session::UnitFieldNotification::Health { previous: 500 },
        0,
    );
    state.refresh_health(&world);
    let mut result = Vec::new();
    while let Some(notification) = state.take_notification() {
        let RuntimePlayerUiNotification::Health {
            snapshot,
            health_changed,
            maximum_changed,
        } = notification
        else {
            panic!("health");
        };
        result.push((
            snapshot.health,
            snapshot.maximum,
            snapshot.predicted,
            health_changed,
            maximum_changed,
        ));
    }
    assert_eq!(
        result,
        [
            (500, 1000, 400, false, false),
            (500, 1200, 400, false, true),
            (380, 1200, 380, true, false)
        ]
    );
    world.update_fields(1, [(150, 0x10)])?;
    assert!(
        RuntimePlayerHealthSnapshot::from_world(&world)
            .ok_or("snapshot")?
            .ghost
    );
    Ok(())
}

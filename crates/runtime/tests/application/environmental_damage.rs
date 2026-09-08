//! Encrypted environmental packets, retained health and original Lua arguments.

use crate::application::gameplay_coordinator::{
    environmental_damage::RuntimeCombatLogClock,
    player_ui::{RuntimePlayerUiNotification, RuntimePlayerUiState},
};
use crate::test_network::{TestError, WorldServer};
use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    AddonCatalog, FrameManager, UiPlayerState, UiPlayerVitalsState, UiScriptEnvironment,
    UiUnitPowerType,
};

thread_local! { static FEEDBACK_NOW: std::cell::Cell<u32> = const { std::cell::Cell::new(1000) }; }
fn feedback_now() -> u32 {
    FEEDBACK_NOW.get()
}

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn stock_combat_feedback_handles_environmental_damage_colors_absorption_and_fade()
-> Result<(), Box<dyn std::error::Error>> {
    use solarity_asset::AssetPath;
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut stock = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let paths = [
        "Interface/FrameXML/GlobalStrings.lua",
        "Interface/FrameXML/CombatFeedback.lua",
        "Fonts/FRIZQT__.TTF",
    ];
    let reads = paths
        .iter()
        .map(|path| stock.read(&AssetPath::new(path)?))
        .collect::<Result<Vec<_>, solarity_asset::AssetError>>()?;
    let mut files = paths
        .iter()
        .zip(&reads)
        .map(|(path, read)| (*path, read.bytes()))
        .collect::<Vec<_>>();
    files.push((
        "Interface/FrameXML/FrameXML.toc",
        b"GlobalStrings.lua\nCombatFeedback.lua\nFeedback.xml\n",
    ));
    files.push(("Interface/FrameXML/Feedback.xml", br#"<Ui><Font name="FeedbackFont" font="Fonts/FRIZQT__.TTF" virtual="true"><FontHeight><AbsValue val="30"/></FontHeight></Font><Frame name="Feedback"><Size x="800" y="600"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer level="OVERLAY"><FontString name="Hit" inherits="FeedbackFont" hidden="true"><Size x="200" y="60"/><Anchors><Anchor point="CENTER"/></Anchors></FontString></Layer></Layers>
<Scripts><OnLoad>CombatFeedback_Initialize(self, Hit, 30);self:RegisterEvent('UNIT_COMBAT')</OnLoad>
<OnEvent>local unit,action,flags,amount,school=...;if unit=='player' then CombatFeedback_OnCombatEvent(self,action,flags,amount,school) end</OnEvent>
<OnUpdate function="CombatFeedback_OnUpdate"/></Scripts></Frame></Ui>"#));
    files.push(("Interface/FrameXML/Bindings.xml", br#"<Bindings><Binding name="CHECK">
Hit:SetFontObject(Hit:GetFontObject());TEXT=Hit:GetText();ALPHA=tostring(Hit:GetAlpha());local r,g,b=Hit:GetTextColor();COLOR=r..','..g..','..b;local _,height=Hit:GetFont();HEIGHT=tostring(height)
</Binding></Bindings>"#));
    let fixture = super::tests::fixture_with(&files)?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?
        .with_client_clock(solarity_ui::UiClientClock::from_source(feedback_now));
    let ui = environment.world_state();
    ui.enter_player(UiPlayerState::new(0));
    ui.set_player_guid(1);
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    for (kind, amount, absorbed, resisted, expected_text, color, height) in [
        (1, 100, 0, 0, "100", "1,1,1", 30.0),
        (3, 80, 0, 0, "80", "1,1,0", 30.0),
        (4, 12, 0, 0, "12", "1,1,0", 30.0),
        (1, 0, 35, 0, "Absorb", "1,1,1", 22.5),
        (1, 0, 0, 40, "Resist", "1,1,1", 22.5),
    ] {
        FEEDBACK_NOW.set(1000);
        let mut active = solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(
            solarity_ecs::WorldMapId::new(0),
            1,
            "WaterTest",
            glam::Vec3::ZERO,
            0.0,
        ));
        active.create_object(1, solarity_ecs::ObjectKind::Player, None, [])?;
        let mut state = RuntimePlayerUiState::default();
        state.receive_environmental_damage(
            &mut active,
            solarity_network::WorldEnvironmentalDamage {
                guid: 1,
                kind,
                amount,
                absorbed,
                resisted,
            },
            Some("WaterTest".into()),
            None,
            1000,
        );
        while let Some(notification) = state.take_notification() {
            if let RuntimePlayerUiNotification::EnvironmentalDamage(impact) = notification {
                super::super::environmental_damage::dispatch_environmental_damage(
                    &mut manager,
                    impact,
                )?;
            }
        }
        manager.invoke_binding("CHECK", true)?;
        assert_eq!(
            manager.localized_text("TEXT")?.as_deref(),
            Some(expected_text)
        );
        assert_eq!(manager.localized_text("COLOR")?.as_deref(), Some(color));
        assert_eq!(
            manager
                .localized_text("HEIGHT")?
                .ok_or("font height")?
                .parse::<f32>()?,
            height
        );
        assert_eq!(manager.region_is_shown("Hit"), Some(true));
        for (milliseconds, alpha) in [
            (1000, 0.0),
            (1100, 0.5),
            (1200, 1.0),
            (1800, 1.0),
            (2050, 0.5),
        ] {
            FEEDBACK_NOW.set(milliseconds);
            manager.update(0.01)?;
            manager.invoke_binding("CHECK", true)?;
            let actual = manager
                .localized_text("ALPHA")?
                .ok_or("alpha")?
                .parse::<f32>()?;
            assert!(
                (actual - alpha).abs() < 0.0001,
                "time={milliseconds} alpha={actual}"
            );
            if alpha > 0.0 {
                assert!(
                    !manager.render_plan().mesh().vertices().is_empty(),
                    "damage text must reach render geometry"
                );
            }
        }
        FEEDBACK_NOW.set(2201);
        manager.update(0.01)?;
        assert_eq!(manager.region_is_shown("Hit"), Some(false));
    }
    assert!(manager.take_callback_failure().is_none());
    Ok(())
}

#[test]
fn environmental_damage_packets_health_and_lua_events_match_original_receiver()
-> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
        let fixture=super::tests::fixture_with(&[
            ("Interface/FrameXML/FrameXML.toc",b"Damage.xml\n"),
            ("Interface/FrameXML/Damage.xml",br#"<Ui><Frame name="DamageObserver"><Scripts>
<OnLoad>
function Capture(...) local values={}; for i=1,select('#',...) do values[i]=tostring(select(i,...)) end return table.concat(values,',') end
LOG='';EVENTS='';HEALTH_AT_LOG='';
for _,e in ipairs({'COMBAT_LOG_EVENT','COMBAT_LOG_EVENT_UNFILTERED','COMBAT_TEXT_UPDATE','UNIT_COMBAT','UNIT_HEALTH','UNIT_MAXHEALTH'}) do self:RegisterEvent(e) end
</OnLoad><OnEvent>
local id=({COMBAT_LOG_EVENT=566,COMBAT_LOG_EVENT_UNFILTERED=567,COMBAT_TEXT_UPDATE=484,UNIT_COMBAT=143,UNIT_HEALTH=18,UNIT_MAXHEALTH=26})[event]
if EVENTS~='' then EVENTS=EVENTS..';' end; EVENTS=EVENTS..id..':'..Capture(...)
if event=='COMBAT_LOG_EVENT' then LOG=Capture(...);HEALTH_AT_LOG=tostring(UnitHealth('player')) end
</OnEvent></Scripts></Frame></Ui>"#),
            ("Interface/FrameXML/Bindings.xml",br#"<Bindings>
<Binding name="RESET">LOG='';EVENTS='';HEALTH_AT_LOG='';CombatLogClearEntries();CombatTextSetActiveUnit('player')</Binding>
<Binding name="INACTIVE">CombatTextSetActiveUnit('target')</Binding>
<Binding name="QUERY">HEALTH=tostring(UnitHealth('player'));COUNT=tostring(CombatLogGetNumEntries(true));CombatLogSetCurrentEntry(0,true);HISTORY=Capture(CombatLogGetCurrentEntry())</Binding>
</Bindings>"#),
        ]).map_err(|error|error.to_string())?;
        let store=AssetStore::mount(ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?,Locale::EnUs)?)?;
        let environment=UiScriptEnvironment::new(800,600,false)?;
        let ui=environment.world_state();ui.enter_player(UiPlayerState::new(0));ui.set_player_guid(1);
        let mut manager=FrameManager::start_shared(AssetStoreHandle::new(store),environment,&[],&AddonCatalog::default())?;
        let rows=include_str!("../fixtures/environmental_damage_native.txt").lines().map(|line|line.split('|').collect::<Vec<_>>()).collect::<Vec<_>>();
        let messages=rows.iter().map(|row|{
            let body=(0..row[0].len()).step_by(2).map(|index|u8::from_str_radix(&row[0][index..index+2],16)).collect::<Result<Vec<_>,_>>()?;
            Ok((0x1fc,body))
        }).collect::<Result<Vec<_>,std::num::ParseIntError>>()?;
        let (server,network)=WorldServer::connect().await?;
        let (mut reader,_writer)=network.split();
        let sent=server.exchange(messages,0).await?;
        for row in rows {
            manager.invoke_binding("RESET",true)?;
            if row[1]!="1" {manager.invoke_binding("INACTIVE",true)?;}
            ui.set_player_vitals(UiPlayerVitalsState::new(500,1000,70,100,UiUnitPowerType::Mana));
            let mut active=solarity_ecs::ActiveWorld::enter(solarity_ecs::WorldBootstrap::new(solarity_ecs::WorldMapId::new(0),1,"WaterTest",glam::Vec3::ZERO,0.0));
            active.create_object(1,solarity_ecs::ObjectKind::Player,None,[])?;
            solarity_systems::project_object_fields(&mut active,1,[(24,500),(32,1000)])?;
            let mut state=RuntimePlayerUiState::default().with_combat_clock(RuntimeCombatLogClock {unix_seconds:1_700_000_000,milliseconds:1000});
            state.observe_health(&active);state.discard_published_notifications();
            let packet=reader.receive_packet().await?;
            assert_eq!(packet.name(),Some("SMSG_ENVIRONMENTALDAMAGELOG"));
            let packet=packet.environmental_damage()?.ok_or("damage packet")?;
            state.receive_environmental_damage(&mut active,packet,Some("WaterTest".into()),None,2250);
            while let Some(notification)=state.take_notification() {
                match notification {
                    RuntimePlayerUiNotification::Health {snapshot,health_changed,maximum_changed} => super::dispatch_health(&mut manager,&ui,snapshot,health_changed,maximum_changed)?,
                    RuntimePlayerUiNotification::EnvironmentalDamage(impact) => super::super::environmental_damage::dispatch_environmental_damage(&mut manager,impact)?,
                    _ => return Err("unexpected player notification".into()),
                }
            }
            manager.invoke_binding("QUERY",true)?;
            assert_eq!(manager.localized_text("HEALTH")?.as_deref(),Some(row[2]));
            assert_eq!(manager.localized_text("LOG")?.unwrap_or_default(),row[4]);
            assert_eq!(manager.localized_text("HISTORY")?.unwrap_or_default(),row[4]);
            assert_eq!(manager.localized_text("EVENTS")?.unwrap_or_default(),row[5]);
            if !row[4].is_empty() {assert_eq!(manager.localized_text("HEALTH_AT_LOG")?.as_deref(),Some(row[2]));}
            assert_eq!(manager.localized_text("COUNT")?.as_deref(),Some(if row[4].is_empty(){"0"}else{"1"}));
            let impact=state.take_environmental_impact();
            assert_eq!(usize::from(impact.is_some()),row[3].parse::<usize>()?);
            if let Some(impact)=impact {assert_eq!(impact.identity,active.object_identity(1).ok_or("identity")?);}
            assert!(state.take_environmental_impact().is_none());
            assert_eq!(active.local_player_vitals().ok_or("vitals")?.health(),500);
        }
        sent.await??;
        assert!(manager.take_callback_failure().is_none());
        Ok(())
    })
}

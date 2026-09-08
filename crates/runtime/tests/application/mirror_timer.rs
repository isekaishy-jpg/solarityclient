//! Encrypted packets through the runtime owner and real FrameXML Lua queries.

use std::cell::Cell;

use solarity_asset::{
    ArchiveCatalog, AssetPath, AssetStore, AssetStoreHandle, ClientDataRoot, Locale,
    SpellNameCatalog,
};
use solarity_ui::{AddonCatalog, FrameManager, UiClientClock, UiScriptEnvironment};

use super::{dispatch_notification, project_timer};
use crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiState;
use crate::test_network::{TestError, WorldServer};
use crate::test_support::ClientFixture;

thread_local! { static NOW: Cell<u32> = const { Cell::new(0) }; }
fn now() -> u32 {
    NOW.get()
}

const DELTAS: [u32; 5] = [0, 1, 1234, 100000, 0x80000001];
const TOKENS: [&str; 4] = ["EXHAUSTION", "BREATH", "FEIGNDEATH", "UNKNOWN"];
const LABELS: [&str; 4] = ["", "Fatigue", "Breath", "Authored spell"];

#[test]
#[ignore = "requires SOLARITY_STOCK_DATA_ROOT with locally owned build-12340 archives"]
fn stock_mirror_timer_frames_show_count_refill_pause_and_hide()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("SOLARITY_STOCK_DATA_ROOT").ok_or("stock data root")?;
    let mut stock = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(root)?,
        Locale::EnUs,
    )?)?;
    let names = SpellNameCatalog::load(&mut stock)?;
    assert_eq!(names.name(5384), Some("Feign Death"));
    let paths = [
        "Interface/FrameXML/MirrorTimer.lua",
        "Interface/FrameXML/MirrorTimer.xml",
        "Fonts/FRIZQT__.TTF",
        "Interface/CastingBar/UI-CastingBar-Border.blp",
        "Interface/TargetingFrame/UI-StatusBar.blp",
    ];
    let reads = paths
        .iter()
        .map(|path| stock.read(&AssetPath::new(path)?))
        .collect::<Result<Vec<_>, solarity_asset::AssetError>>()?;
    let mut extra = paths
        .iter()
        .zip(&reads)
        .map(|(path, read)| (*path, read.bytes()))
        .collect::<Vec<_>>();
    extra.push((
        "Interface/FrameXML/FrameXML.toc",
        b"Timers.lua\nStockPrelude.xml\nMirrorTimer.xml\nTimers.xml\n",
    ));
    extra.push(("Interface/FrameXML/StockPrelude.xml", br#"<Ui>
<Font name="GameFontHighlight" font="Fonts/FRIZQT__.TTF" virtual="true"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<Script>STATICPOPUP_NUMDIALOGS=4; function LowerFrameLevel(frame) frame:SetFrameLevel(frame:GetFrameLevel()-1) end</Script>
<Frame name="UIParent"><Size x="800" y="600"/><Anchors><Anchor point="CENTER"/></Anchors><Scripts>
<OnLoad>self:RegisterEvent('MIRROR_TIMER_START')</OnLoad><OnEvent>MirrorTimer_Show(...)</OnEvent>
</Scripts></Frame></Ui>"#));
    extra.push(("Interface/FrameXML/Bindings.xml", br#"<Bindings><Binding name="CHECK">
 local values={}
 for i=1,3 do
  local frame=_G['MirrorTimer'..i]
  if frame:IsShown() then local bar=_G[frame:GetName()..'StatusBar']; local lo,hi=bar:GetMinMaxValues()
   values[#values+1]=frame.timer..','.._G[frame:GetName()..'Text']:GetText()..','..bar:GetValue()..','..hi
  end
 end
 VISIBLE=table.concat(values,'|')
</Binding></Bindings>"#));
    let fixture = fixture_with(&extra)?;
    let store = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let environment = UiScriptEnvironment::new(800, 600, false)?
        .with_client_clock(UiClientClock::from_source(now));
    let world = environment.world_state();
    let mut manager = FrameManager::start_shared(
        AssetStoreHandle::new(store),
        environment,
        &[],
        &AddonCatalog::default(),
    )?;
    let start = |timer, value, scale, paused| {
        crate::application::gameplay_coordinator::player_ui::TimedMirrorTimerUpdate {
            update: solarity_network::WorldMirrorTimerUpdate::Start {
                timer,
                value,
                maximum: 60000,
                scale,
                paused,
                spell_id: 0,
            },
            timestamp_ms: 1000,
        }
    };
    NOW.set(1000);
    dispatch_notification(&mut manager, &world, &names, start(1, 60000, -1, 0))?;
    manager.invoke_binding("CHECK", true)?;
    assert_eq!(
        manager.localized_text("VISIBLE")?.as_deref(),
        Some("BREATH,Breath,60,60")
    );
    NOW.set(1250);
    manager.update(0.25)?;
    manager.invoke_binding("CHECK", true)?;
    assert_eq!(
        manager.localized_text("VISIBLE")?.as_deref(),
        Some("BREATH,Breath,59.75,60")
    );
    dispatch_notification(&mut manager, &world, &names, start(0, 30000, -1, 0))?;
    manager.invoke_binding("CHECK", true)?;
    assert_eq!(
        manager.localized_text("VISIBLE")?.as_deref(),
        Some("BREATH,Breath,59.75,60|EXHAUSTION,Fatigue,30,60")
    );
    for timer in 1..=2 {
        let bar_name = format!("MirrorTimer{timer}StatusBar");
        let text_name = format!("MirrorTimer{timer}Text");
        let bar = manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .find(|member| manager.object_name(member.owner_index()) == Some(bar_name.as_str()))
            .ok_or("missing timer fill quad")?;
        let mesh = manager.render_plan().mesh();
        let bar_position = mesh
            .object_indices()
            .iter()
            .position(|index| *index == bar.object_index())
            .ok_or("missing timer fill in mesh")?;
        let text_position = mesh
            .object_indices()
            .iter()
            .position(|index| manager.object_name(*index) == Some(text_name.as_str()))
            .ok_or("missing timer label glyphs in mesh")?;
        assert!(
            bar_position < text_position,
            "{text_name} must render above its lowered status bar: fill={bar_position}, text={text_position}"
        );
    }
    dispatch_notification(&mut manager, &world, &names, start(1, 10000, 10, 0))?;
    manager.update(0.25)?;
    manager.invoke_binding("CHECK", true)?;
    assert_eq!(
        manager.localized_text("VISIBLE")?.as_deref(),
        Some("BREATH,Breath,12.5,60|EXHAUSTION,Fatigue,29.75,60")
    );
    dispatch_notification(&mut manager, &world, &names, start(1, 25000, -1, 1))?;
    NOW.set(2250);
    manager.update(1.0)?;
    manager.invoke_binding("CHECK", true)?;
    assert_eq!(
        manager.localized_text("VISIBLE")?.as_deref(),
        Some("BREATH,Breath,25,60|EXHAUSTION,Fatigue,28.75,60")
    );
    for timer in 0..3 {
        dispatch_notification(
            &mut manager,
            &world,
            &names,
            crate::application::gameplay_coordinator::player_ui::TimedMirrorTimerUpdate {
                update: solarity_network::WorldMirrorTimerUpdate::Stop { timer },
                timestamp_ms: 0,
            },
        )?;
    }
    manager.invoke_binding("CHECK", true)?;
    assert_eq!(manager.localized_text("VISIBLE")?, None);
    assert!(manager.take_callback_failure().is_none());
    Ok(())
}

#[test]
fn mirror_timers_match_original_receiver_events_and_lua_progress() -> Result<(), TestError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let fixture = fixture().map_err(|error| error.to_string())?;
            let archive =
                ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
            let mut store = AssetStore::mount(archive)?;
            let names = SpellNameCatalog::load(&mut store)?;
            assert_eq!(names.name(5384), Some("Authored spell"));
            assert_eq!(names.name(999), None);
            let environment = UiScriptEnvironment::new(800, 600, false)?
                .with_client_clock(UiClientClock::from_source(now));
            let world = environment.world_state();
            let mut manager = FrameManager::start_shared(
                AssetStoreHandle::new(store),
                environment,
                &[],
                &AddonCatalog::default(),
            )?;
            let captures = include_bytes!("../fixtures/mirror_timer.bin")
                .chunks_exact(296)
                .collect::<Vec<_>>();
            assert_eq!(captures.len(), 40);
            let packets = captures
                .iter()
                .map(|record| {
                    (
                        word(record, 4) as u16,
                        record[12..12 + word(record, 8) as usize].to_vec(),
                    )
                })
                .collect();
            let (server, mut network) = WorldServer::connect().await?;
            let sent = server.exchange(packets, 0).await?;
            let mut timers = RuntimePlayerUiState::default();
            for (case, record) in captures.iter().enumerate() {
                let timestamp = word(record, 0);
                NOW.set(timestamp);
                let update = network
                    .receive_packet()
                    .await?
                    .mirror_timer()?
                    .ok_or("timer packet")?;
                timers.receive(update, timestamp);
            let notification = timers.take_notification().ok_or("ordered timer event")?;
            let crate::application::gameplay_coordinator::player_ui::RuntimePlayerUiNotification::MirrorTimer(notification) = notification else { return Err("expected mirror timer".into()); };
                dispatch_notification(&mut manager, &world, &names, notification)?;
                assert_eq!(
                    manager.localized_text("EVENT")?.as_deref(),
                    Some(event_text(&record[120..148]).as_str()),
                    "event case {case}"
                );
                assert_eq!(
                    manager.localized_text("BEFORE")?.as_deref(),
                    Some(state_text(&record[212..296]).as_str()),
                    "event-before-store case {case}"
                );
                manager.invoke_binding("CHECK", true)?;
                assert_eq!(
                    manager.localized_text("STATE")?.as_deref(),
                    Some(state_text(&record[36..120]).as_str()),
                    "Info case {case}"
                );
                for (index, slot) in timers.slots().iter().enumerate() {
                    let expected = slot
                        .map(|value| project_timer(&names, value))
                        .unwrap_or_default();
                    assert_eq!(
                        world.mirror_timer(index),
                        Some(expected),
                        "runtime/UI state case {case}"
                    );
                }
                for (index, delta) in DELTAS.iter().enumerate() {
                    NOW.set(timestamp.wrapping_add(*delta));
                    manager.invoke_binding("CHECK", true)?;
                    let expected = (0..3)
                        .map(|slot| (word(record, 152 + index * 12 + slot * 4) as i32).to_string())
                        .collect::<Vec<_>>()
                        .join("|");
                    assert_eq!(
                        manager.localized_text("PROGRESS")?.as_deref(),
                        Some(expected.as_str()),
                        "progress case {case}, delta {delta}"
                    );
                }
            }
            sent.await??;
            assert!(timers.take_notification().is_none());
            manager.invoke_binding("ARGUMENTS", true)?;
            Ok(())
        })
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn state_text(bytes: &[u8]) -> String {
    bytes
        .as_chunks::<28>()
        .0
        .iter()
        .map(|slot| {
            let kind = word(slot, 0);
            let label = if word(slot, 20) == 5384 {
                "Authored spell"
            } else {
                match kind {
                    0 => "Fatigue",
                    1 => "Breath",
                    _ => "",
                }
            };
            format!(
                "{},{},{},{},{},{}",
                TOKENS[kind as usize],
                word(slot, 4) as i32,
                word(slot, 8) as i32,
                word(slot, 12) as i32,
                word(slot, 16),
                label
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn event_text(bytes: &[u8]) -> String {
    let token = TOKENS[word(bytes, 4) as usize];
    match word(bytes, 0) {
        0x160 => format!(
            "MIRROR_TIMER_START|{token}|{}|{}|{}|{}|{}",
            word(bytes, 8) as i32,
            word(bytes, 12) as i32,
            word(bytes, 16) as i32,
            word(bytes, 20),
            LABELS[word(bytes, 24) as usize]
        ),
        0x161 => format!("MIRROR_TIMER_PAUSE|{token}|{}", word(bytes, 8)),
        0x162 => format!("MIRROR_TIMER_STOP|{token}"),
        _ => panic!("native timer event"),
    }
}

fn dbc(words: &[u32], fields: u32, strings: &[u8]) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for word in [
        words.len() as u32 / fields,
        fields,
        fields * 4,
        strings.len() as u32,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(strings);
    bytes
}

fn fixture() -> Result<ClientFixture, Box<dyn std::error::Error>> {
    fixture_with(&[])
}

pub(super) fn fixture_with(
    extra: &[(&str, &[u8])],
) -> Result<ClientFixture, Box<dyn std::error::Error>> {
    let mut spell = [0; 234];
    spell[0] = 5384;
    spell[136] = 1;
    let spells = dbc(&spell, 234, b"\0Authored spell\0");
    let base = dbc(&[0; 11], 1, b"\0");
    let coefficients = dbc(&[0; 1100], 1, b"\0");
    let slots = dbc(&[], 3, b"\0");
    let mut files: Vec<(&str, &[u8])> = vec![
        ("DBFilesClient/Spell.dbc", &spells),
        ("DBFilesClient/gtChanceToMeleeCritBase.dbc", &base),
        ("DBFilesClient/gtChanceToSpellCritBase.dbc", &base),
        ("DBFilesClient/PaperDollItemFrame.dbc", &slots),
        ("Interface/FrameXML/FrameXML.toc", b"Timers.lua\nTimers.xml\n"),
        ("WTF/DefaultBindings.wtf", b""),
        ("Interface/FrameXML/Bindings.xml", br#"<Bindings>
<Binding name="CHECK">STATE=CaptureState(); PROGRESS=GetMirrorTimerProgress('exhaustion')..'|'..GetMirrorTimerProgress('bReAtH')..'|'..GetMirrorTimerProgress('FEIGNDEATH')</Binding>
<Binding name="ARGUMENTS">
assert(select('#', GetMirrorTimerInfo('2.9')) == 6)
assert(GetMirrorTimerInfo(1.9) == GetMirrorTimerInfo(1))
for _, value in ipairs({0, 4, -1, 'bad', true, {}}) do assert(not pcall(GetMirrorTimerInfo, value)) end
for _, value in ipairs({'UNKNOWN', 'DEATH', '', 1, true, {}}) do assert(not pcall(GetMirrorTimerProgress, value)) end
assert(not pcall(GetMirrorTimerInfo)); assert(not pcall(GetMirrorTimerProgress))
UNKNOWN_LABEL='Custom inactive label'; assert(select(6, GetMirrorTimerInfo(1)) == UNKNOWN_LABEL)
</Binding></Bindings>"#),
        ("Interface/FrameXML/Timers.lua", br#"BREATH_LABEL='Breath'; EXHAUSTION_LABEL='Fatigue'
function CaptureState()
 local result={}
 for i=1,3 do local k,v,m,s,p,l=GetMirrorTimerInfo(i); result[i]=k..','..v..','..m..','..s..','..p..','..l end
 return table.concat(result,'|')
end
function CaptureEvent(self, event, ...)
 local values={event,...}; for i=1,#values do values[i]=tostring(values[i]) end
 EVENT=table.concat(values,'|'); BEFORE=CaptureState()
end"#),
        ("Interface/FrameXML/Timers.xml", br#"<Ui><Frame name="TimerObserver"><Scripts><OnLoad>
self:RegisterEvent('MIRROR_TIMER_START'); self:RegisterEvent('MIRROR_TIMER_PAUSE'); self:RegisterEvent('MIRROR_TIMER_STOP')
</OnLoad><OnEvent function="CaptureEvent"/></Scripts></Frame></Ui>"#),
    ];
    for path in [
        "DBFilesClient/gtChanceToMeleeCrit.dbc",
        "DBFilesClient/gtChanceToSpellCrit.dbc",
        "DBFilesClient/gtOCTRegenHP.dbc",
        "DBFilesClient/gtRegenHPPerSpt.dbc",
        "DBFilesClient/gtRegenMPPerSpt.dbc",
    ] {
        files.push((path, &coefficients));
    }
    for (path, bytes) in extra {
        files.retain(|(existing, _)| existing != path);
        files.push((path, bytes));
    }
    ClientFixture::with_common_files(&files)
}

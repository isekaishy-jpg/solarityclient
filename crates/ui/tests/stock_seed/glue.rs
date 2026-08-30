//! External stock-compatibility tests for persistent GlueXML ownership.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_ui::{
    GlueError, GlueManager, UiEventArgument, UiEventError, UiEventPayload, UiLayoutError,
    UiObjectKind,
};

use crate::support::{Fixture, FixtureFile};

/// Startup publishes the two stock lifecycle events before the first snapshot.
#[test]
fn glue_manager_activates_the_stock_login_screen() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Lifecycle.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Lifecycle.xml",
            bytes: br#"<Ui><Frame name="GlueParent"><Scripts>
  <OnLoad>
    LIFECYCLE = ""
    self:RegisterEvent("FRAMES_LOADED")
    self:RegisterEvent("SET_GLUE_SCREEN")
  </OnLoad>
  <OnEvent>
    if event == "FRAMES_LOADED" then
      LIFECYCLE = LIFECYCLE .. "FRAMES_LOADED;"
    elseif event == "SET_GLUE_SCREEN" then
      SetCurrentScreen(arg1)
      PlayGlueMusic("Sound\\Music\\GlueScreenMusic\\WotLK_main_title.mp3")
      PlayGlueAmbience("Sound\\Ambience\\GlueScreen\\Dwarf.mp3", 4.0)
      LIFECYCLE = LIFECYCLE .. "SET_GLUE_SCREEN:" .. arg1 .. ";"
    end
  </OnEvent>
</Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let globals = manager.bundle().lua().globals();

    assert_eq!(
        globals.get::<String>("LIFECYCLE")?,
        "FRAMES_LOADED;SET_GLUE_SCREEN:login;"
    );
    let current_screen = globals.get::<mlua::Function>("GetCurrentScreen")?;
    assert_eq!(current_screen.call::<String>(())?, "login");
    let media = manager.media_intent();
    assert_eq!(
        media.music(),
        Some("Sound\\Music\\GlueScreenMusic\\WotLK_main_title.mp3")
    );
    assert_eq!(
        media.ambience(),
        Some("Sound\\Ambience\\GlueScreen\\Dwarf.mp3")
    );
    Ok(())
}

/// The manager retains executable state after temporary XML plans are gone.
#[test]
fn glue_manager_owns_executed_login_ui() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Bootstrap.xml\nAfter.lua\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Bootstrap.xml",
            bytes: br#"<Ui><Frame name="GlueBootstrap"><Frames>
  <Model name="$parentModel"/>
</Frames><Scripts><OnLoad>
  GlueBootstrapModel:SetModel("Solarity\\FixtureMarker.txt")
  self.loaded = true
</OnLoad></Scripts></Frame></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\After.lua",
            bytes: br#"assert(GlueBootstrap.loaded)
assert(GlueBootstrapModel:GetModel() == "SOLARITY\\FIXTUREMARKER.TXT")
GLUE_READY = true"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let report = manager.report();

    assert_eq!(report.resource_count(), 2);
    assert_eq!(report.action_count(), 2);
    assert_eq!(report.object_count(), 2);
    assert_eq!(report.named_object_count(), 2);
    assert_eq!(report.frame_count(), 2);
    assert_eq!(report.region_count(), 2);
    assert_eq!(report.executed_chunk_count(), 1);
    assert_eq!(report.executed_load_handler_count(), 1);
    assert_eq!(manager.objects()[0].kind(), UiObjectKind::Frame);
    assert_eq!(manager.children(0), Some(&[1][..]));
    assert_eq!(manager.objects()[1].parent(), Some(0));
    assert!(manager.bundle().lua().globals().get::<bool>("GLUE_READY")?);
    Ok(())
}

/// Screen rectangles use post-OnLoad dimensions, anchors, and dynamic frames.
#[test]
fn glue_manager_resolves_live_startup_geometry() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Geometry.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Geometry.xml",
            bytes: br#"<Ui>
<Frame name="DynamicTemplate" virtual="true" alpha="0.5" scale="0.5">
  <Size x="60" y="30"/>
  <Anchors><Anchor point="CENTER"/></Anchors>
</Frame>
<Frame name="Root"><Frames>
  <Frame name="$parentStretch">
    <Size x="7" y="9"/>
    <Anchors>
      <Anchor point="TOPLEFT" relativeTo="$parent" relativePoint="TOPLEFT">
        <Offset x="5" y="-6"/>
      </Anchor>
      <Anchor point="BOTTOMRIGHT" relativeTo="$parent" relativePoint="BOTTOMRIGHT">
        <Offset x="-7" y="8"/>
      </Anchor>
    </Anchors>
  </Frame>
</Frames><Scripts><OnLoad>
  self:SetSize(400, 200)
  self:SetPoint("CENTER", nil, "CENTER")
  self:SetAlpha(0.8)
  self:SetScale(0.5)
  local dynamic = CreateFrame("Frame", "Dynamic", self)
  dynamic:SetSize(120, 40)
  dynamic:SetPoint("TOPLEFT", self, "TOPLEFT", 10, -20)
  CreateFrame("Frame", "Templated", self, "DynamicTemplate")
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;

    assert_eq!(manager.report().object_count(), 4);
    assert_eq!(manager.report().region_count(), 4);
    assert_eq!(manager.objects()[2].name(), Some("Dynamic"));
    assert_eq!(manager.objects()[3].name(), Some("Templated"));
    assert_eq!(manager.children(0), Some(&[1, 2, 3][..]));

    let root = manager
        .geometry()
        .region(0)
        .ok_or("missing root geometry")?;
    let stretch = manager
        .geometry()
        .region(1)
        .ok_or("missing stretch geometry")?;
    let dynamic = manager
        .geometry()
        .region(2)
        .ok_or("missing dynamic geometry")?;
    let templated = manager
        .geometry()
        .region(3)
        .ok_or("missing templated geometry")?;
    assert_close(root.logical_bounds().width(), 400.0);
    assert_close(root.logical_bounds().height(), 200.0);
    assert_close(root.logical_bounds().left(), 482.666_666_666_7);
    assert_close(root.logical_bounds().bottom(), 284.0);
    assert_close(root.presentation_bounds().width(), 200.0);
    assert_close(root.effective_alpha(), 0.8);
    assert_close(root.effective_scale(), 0.5);
    assert_close(
        stretch.logical_bounds().left(),
        root.logical_bounds().left() + 5.0,
    );
    assert_close(
        stretch.logical_bounds().right(),
        root.logical_bounds().right() - 7.0,
    );
    assert_close(
        stretch.logical_bounds().top(),
        root.logical_bounds().top() - 6.0,
    );
    assert_close(
        stretch.logical_bounds().bottom(),
        root.logical_bounds().bottom() + 8.0,
    );
    assert_close(
        dynamic.logical_bounds().left(),
        root.logical_bounds().left() + 10.0,
    );
    assert_close(
        dynamic.logical_bounds().top(),
        root.logical_bounds().top() - 20.0,
    );
    assert_close(dynamic.logical_bounds().width(), 120.0);
    assert_close(dynamic.logical_bounds().height(), 40.0);
    assert_close(dynamic.presentation_bounds().width(), 60.0);
    assert_close(templated.logical_bounds().width(), 60.0);
    assert_close(templated.logical_bounds().height(), 30.0);
    assert_close(
        (templated.logical_bounds().left() + templated.logical_bounds().right()) * 0.5,
        (root.logical_bounds().left() + root.logical_bounds().right()) * 0.5,
    );
    assert_close(templated.presentation_bounds().width(), 15.0);
    assert_close(templated.effective_alpha(), 0.4);
    assert_close(templated.effective_scale(), 0.25);
    Ok(())
}

/// Mutually dependent live anchors fail instead of receiving guessed bounds.
#[test]
fn glue_manager_rejects_live_anchor_cycles() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Cycle.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Cycle.xml",
            bytes: br#"<Ui><Frame name="Root"><Frames>
  <Frame name="First"><Size x="10" y="10"/></Frame>
  <Frame name="Second"><Size x="10" y="10"/></Frame>
</Frames><Scripts><OnLoad>
  First:SetPoint("CENTER", Second, "CENTER")
  Second:SetPoint("CENTER", First, "CENTER")
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let result = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false);

    assert!(matches!(
        result,
        Err(GlueError::Layout(UiLayoutError::Resolution { .. }))
    ));
    Ok(())
}

/// Registered `OnEvent` handlers receive stock globals and creation ordering.
#[test]
fn glue_manager_dispatches_canonical_events() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Events.xml\nAfter.lua\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Events.xml",
            bytes: br#"<Ui><Frame name="Root"><Layers>
  <Layer><Texture name="Visual" file="Interface\Glues\Visual"/></Layer>
</Layers><Frames>
  <Frame name="Child"><Scripts>
    <OnLoad>self:RegisterEvent("set_glue_screen")</OnLoad>
    <OnEvent>
      local screen, sequence = ...
      assert(event == "SET_GLUE_SCREEN" and arg1 == screen and arg2 == sequence and arg3 == nil)
      if screen == "login" and sequence == nil then return end
      EVENT_ORDER = EVENT_ORDER .. self:GetName() .. ":" .. screen .. ":" .. sequence .. ";"
    </OnEvent>
  </Scripts></Frame>
</Frames><Scripts>
  <OnLoad>EVENT_ORDER = "" self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
  <OnEvent>
    local screen, sequence = ...
    assert(event == "SET_GLUE_SCREEN" and arg1 == screen and arg2 == sequence and arg3 == nil)
    if screen == "login" and sequence == nil then return end
    Visual:Hide()
    EVENT_ORDER = EVENT_ORDER .. self:GetName() .. ":" .. screen .. ":" .. sequence .. ";"
  </OnEvent>
</Scripts></Frame></Ui>"#,
        },
        FixtureFile {
            path: "Interface\\GlueXML\\After.lua",
            bytes: b"event = 'previous' arg1 = 'old' arg2 = 99",
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let payload = UiEventPayload::new([
        UiEventArgument::String("login".to_owned()),
        UiEventArgument::Integer(7),
    ])?;
    assert_eq!(manager.presentation().member_count(), 1);

    let dispatch = manager.dispatch_event("set_glue_screen", &payload)?;

    assert_eq!(dispatch.subscriber_count(), 2);
    assert_eq!(manager.presentation().member_count(), 0);
    let globals = manager.bundle().lua().globals();
    assert_eq!(
        globals.get::<String>("EVENT_ORDER")?,
        "Root:login:7;Child:login:7;"
    );
    assert_eq!(globals.get::<String>("event")?, "previous");
    assert_eq!(globals.get::<String>("arg1")?, "old");
    assert_eq!(globals.get::<i64>("arg2")?, 99);
    assert!(globals.get::<Option<String>>("arg3")?.is_none());
    assert!(matches!(
        manager.dispatch_event("NOT_A_GLUE_EVENT", &UiEventPayload::empty()),
        Err(UiEventError::Unknown { .. })
    ));
    assert!(matches!(
        UiEventPayload::new((0..10).map(UiEventArgument::Integer)),
        Err(UiEventError::PayloadTooLarge { .. })
    ));
    Ok(())
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.000_001,
        "{actual} != {expected}"
    );
}

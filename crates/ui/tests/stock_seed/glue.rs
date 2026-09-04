//! External stock-compatibility tests for persistent GlueXML ownership.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    GlueError, GlueInitialScreen, GlueManager, UiEventArgument, UiEventError, UiEventPayload,
    UiGlueMediaAction, UiGlueNetworkAction, UiGlueNetworkStatus, UiLayoutError, UiObjectKind,
    UiObjectRole, UiPointerButton, UiProcessAction, UiRealmCategory, UiRealmDirectory,
    UiRealmFlags, UiRealmInfo, UiRealmVersion,
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
      PlayGlueMusic("GS_LichKing")
      PlayGlueAmbience("GlueScreenIntro", 4.0)
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
    assert_eq!(media.music(), Some("GS_LichKing"));
    assert_eq!(media.ambience(), Some("GlueScreenIntro"));
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlayGlueMusic("GS_LichKing".to_owned()))
    );
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlayGlueAmbience {
            name: "GlueScreenIntro".to_owned(),
            fade_seconds: 4.0,
        })
    );
    assert_eq!(manager.take_media_action(), None);
    Ok(())
}

/// Event-driven visibility changes stay on the retained mutation path.
#[test]
fn glue_manager_reconciles_visual_events_without_full_snapshots() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"VisualEvent.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\VisualEvent.xml",
            bytes: br#"<Ui>
<Frame name="VisualEventOwner"><Scripts>
  <OnLoad>self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
  <OnEvent>
    if arg1 == "charcreate" then VisualEventTarget:Show() else VisualEventTarget:Hide() end
  </OnEvent>
</Scripts></Frame>
<Frame name="VisualEventTarget" hidden="true">
  <Size x="80" y="40"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Layers><Layer level="ARTWORK">
    <Texture name="$parentTexture" file="Interface\Glues\VisualEvent"/>
  </Layer></Layers>
</Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let texture = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("VisualEventTargetTexture"))
        .ok_or("missing event-owned texture")?;
    let presented = |manager: &GlueManager| {
        manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .any(|member| member.object_index() == texture && member.opacity() > 0.0)
    };
    let snapshots = manager.runtime_snapshot_count();

    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charcreate".to_owned())])?,
    )?;
    assert!(presented(&manager));
    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("login".to_owned())])?,
    )?;
    assert!(!presented(&manager));
    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    Ok(())
}

/// Typed texture and layout mutations copy only their owning Lua objects.
#[test]
fn glue_manager_reconciles_typed_events_without_full_snapshots() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"TypedEvent.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\TypedEvent.xml",
            bytes: br#"<Ui>
<Frame name="TypedEventOwner"><Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
<Layers><Layer level="ARTWORK">
<Texture name="TypedEventTexture" file="Interface\Glues\TypedEvent">
  <Size x="48" y="24"/><Anchors><Anchor point="CENTER"/></Anchors>
</Texture>
</Layer></Layers><Scripts>
  <OnLoad>self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
  <OnEvent>
    TypedEventTexture:ClearAllPoints()
    TypedEventTexture:SetPoint("CENTER", nil, "CENTER", 24, -12)
    if arg1 == "charcreate" then
      TypedEventTexture:SetWidth(96)
      TypedEventTexture:SetTexCoord(0.25, 0.75, 0, 1)
      TypedEventTexture:SetVertexColor(0.2, 0.4, 0.6, 0.8)
    end
  </OnEvent>
</Scripts></Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let texture = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("TypedEventTexture"))
        .ok_or("missing typed-event texture")?;
    let snapshots = manager.runtime_snapshot_count();

    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charcreate".to_owned())])?,
    )?;

    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    assert_eq!(
        manager
            .geometry()
            .region(texture)
            .ok_or("missing typed-event geometry")?
            .logical_bounds()
            .width(),
        96.0
    );
    let presented = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .find(|member| member.object_index() == texture)
        .ok_or("missing typed-event presentation")?;
    assert_eq!(
        presented.tex_coords(),
        [0.25, 0.0, 0.25, 1.0, 0.75, 0.0, 0.75, 1.0]
    );
    assert_eq!(presented.vertex_colors()[0], [0.2, 0.4, 0.6, 0.8]);
    Ok(())
}

/// Any unclassified event mutation preserves the complete-snapshot fallback.
#[test]
fn glue_manager_does_not_mask_fallback_mutations_with_typed_mutations() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"FallbackEvent.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\FallbackEvent.xml",
            bytes: br#"<Ui>
<Frame name="FallbackEventOwner"><Scripts>
  <OnLoad>self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
  <OnEvent>
    FallbackEventTexture:SetVertexColor(0.2, 0.4, 0.6, 1)
    FallbackEventFrame:SetScale(arg1 == "charcreate" and 0.75 or 1)
  </OnEvent>
</Scripts></Frame>
<Frame name="FallbackEventFrame"><Size x="80" y="40"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Layers><Layer level="ARTWORK"><Texture name="FallbackEventTexture" file="Interface\Glues\Fallback"/></Layer></Layers>
</Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let snapshots = manager.runtime_snapshot_count();

    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charcreate".to_owned())])?,
    )?;

    assert_eq!(manager.runtime_snapshot_count(), snapshots + 1);
    Ok(())
}

/// Stock audio globals preserve invocation order and the two boolean-returning
/// direct-file calls while transferring playback to the process media owner.
#[test]
fn glue_manager_queues_typed_stock_audio_actions() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Audio.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Audio.xml",
            bytes: br#"<Ui><Frame name="GlueParent"/></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let globals = manager.bundle().lua().globals();

    globals
        .get::<mlua::Function>("PlaySound")?
        .call::<()>("gsTitleOptions")?;
    assert!(
        globals
            .get::<mlua::Function>("PlaySoundFile")?
            .call::<bool>("Sound\\Interface\\Click.wav")?
    );
    assert!(
        globals
            .get::<mlua::Function>("PlayMusic")?
            .call::<bool>("Sound\\Music\\Credits.mp3")?
    );
    globals.get::<mlua::Function>("StopMusic")?.call::<()>(())?;
    globals
        .get::<mlua::Function>("StopAllSFX")?
        .call::<()>(1.0)?;

    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlaySound("gsTitleOptions".to_owned()))
    );
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlaySoundFile(
            "Sound\\Interface\\Click.wav".to_owned()
        ))
    );
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::PlayMusic(
            "Sound\\Music\\Credits.mp3".to_owned()
        ))
    );
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::StopMusic)
    );
    assert_eq!(
        manager.take_media_action(),
        Some(UiGlueMediaAction::StopAllSfx { fade_seconds: 1.0 })
    );
    assert_eq!(manager.take_media_action(), None);
    Ok(())
}

/// Native first-run selection shows MovieFrame, runs its authored OnShow, and
/// retains the exact locale-loose AVI request for the media owner.
#[test]
fn glue_manager_starts_first_run_movie() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Movie.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Movie.xml",
            bytes: br#"<Ui>
<Frame name="GlueParent"><Scripts><OnLoad>
  self:RegisterEvent("SET_GLUE_SCREEN")
</OnLoad><OnEvent>
  if event == "SET_GLUE_SCREEN" then
    MovieFrame:Show()
    SetCurrentScreen(arg1)
  end
</OnEvent></Scripts></Frame>
<MovieFrame name="MovieFrame" hidden="true"><Scripts><OnShow>
  self:EnableSubtitles(true)
  HideCursor()
  local suffix = GetMovieResolution() >= 1024 and "1024" or "800"
  local movies = { [3] = { [2] = "Interface\\Cinematics\\WOW_Intro_LK_" .. suffix } }
  local index = 1
  repeat index = index + 1 until movies[GetClientExpansionLevel()][index]
  assert(self:StartMovie(movies[GetClientExpansionLevel()][index], 250))
</OnShow><OnMovieFinished>
  self:StopMovie()
  ShowCursor()
  SetCurrentScreen("login")
</OnMovieFinished><OnKeyUp>
  if key == "SPACE" or key == "ENTER" then self:StopMovie() end
</OnKeyUp></Scripts></MovieFrame>
</Ui>"#,
        },
    ])?;
    fixture.write_loose_file(
        "Data/enUS/Interface/Cinematics/WOW_Intro_LK_1024.avi",
        b"RIFF fixture",
    )?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start_shared_with_initial_screen(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1920, 1080),
        false,
        GlueInitialScreen::Movie,
    )?;

    assert_eq!(manager.current_screen(), "movie");
    assert!(!manager.cursor_visible());
    let media = manager.media_intent();
    let movie = media.movie().ok_or("missing first-run movie request")?;
    assert!(
        movie
            .path()
            .ends_with("enUS/Interface/Cinematics/WOW_Intro_LK_1024.avi")
    );
    assert_eq!(movie.volume(), 250);
    assert!(movie.subtitles_enabled());
    let object_index = movie.object_index();
    manager.movie_key_up(object_index, "SPACE")?;
    assert!(manager.media_intent().movie().is_none());
    assert_eq!(manager.take_movie_stop_completion(), Some(object_index));
    manager.movie_finished(object_index)?;
    assert_eq!(manager.current_screen(), "login");
    assert!(manager.cursor_visible());
    assert!(manager.media_intent().movie().is_none());
    Ok(())
}

/// Fresh legal state shows the stock notice and authored acceptance mutates the
/// profile-backed CVar instead of bypassing the agreement sequence.
#[test]
fn glue_manager_retains_legal_agreement_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Agreement.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Agreement.xml",
            bytes: br#"<Ui><Frame name="Agreement"><Scripts><OnLoad>
  EULA_WAS_ACCEPTED = EULAAccepted()
  EULA_SHOWED_NOTICE = ShowEULANotice()
  TOS_WAS_ACCEPTED = TOSAccepted()
  SCAN_WAS_FINISHED = IsScanDLLFinished()
  SYSTEM_WAS_SUPPORTED = IsSystemSupported()
  TOKEN_WAS_USED = GetUsesToken()
  SetUsesToken(true)
  TOKEN_IS_USED = GetUsesToken()
  AcceptEULA()
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start_shared_with_initial_screen_and_cvars(
        AssetStoreHandle::new(AssetStore::mount(catalog)?),
        (1920, 1080),
        false,
        GlueInitialScreen::Login,
        &[("readTOS".to_owned(), "1".to_owned())],
    )?;
    let globals = manager.bundle().lua().globals();

    assert!(!globals.get::<bool>("EULA_WAS_ACCEPTED")?);
    assert!(globals.get::<bool>("EULA_SHOWED_NOTICE")?);
    assert!(globals.get::<bool>("TOS_WAS_ACCEPTED")?);
    assert!(globals.get::<bool>("SCAN_WAS_FINISHED")?);
    assert!(globals.get::<bool>("SYSTEM_WAS_SUPPORTED")?);
    assert!(!globals.get::<bool>("TOKEN_WAS_USED")?);
    assert!(globals.get::<bool>("TOKEN_IS_USED")?);
    assert_eq!(manager.cvar_value("readEULA").as_deref(), Some("1"));
    assert_eq!(
        manager.take_changed_cvars(),
        vec![("readEULA".to_owned(), "1".to_owned())]
    );
    assert!(manager.take_changed_cvars().is_empty());
    Ok(())
}

/// Wow.exe `FUN_004DCD60` maps expansion indices to the three locale-loose
/// credits documents consumed by the stock Credits Glue screen.
#[test]
fn glue_manager_loads_stock_expansion_credits_text() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Credits.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Credits.xml",
            bytes: br#"<Ui><Frame name="Credits"><Scripts><OnLoad>
  BASE_CREDITS = GetCreditsText(1)
  BC_CREDITS = GetCreditsText("2")
  LK_CREDITS = GetCreditsText(3)
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    fixture.write_loose_file("Data/enUS/Credits.html", b"base credits")?;
    fixture.write_loose_file("Data/enUS/Credits_BC.html", b"bc credits")?;
    fixture.write_loose_file("Data/enUS/Credits_LK.html", b"lk credits")?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let globals = manager.bundle().lua().globals();

    assert_eq!(globals.get::<String>("BASE_CREDITS")?, "base credits");
    assert_eq!(globals.get::<String>("BC_CREDITS")?, "bc credits");
    assert_eq!(globals.get::<String>("LK_CREDITS")?, "lk credits");
    Ok(())
}

/// Pointer capture selects the frontmost frame and activates the stock default
/// `LeftButtonUp` action only when release remains over the captured button.
#[test]
fn glue_manager_routes_captured_button_clicks() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Pointer.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Pointer.xml",
            bytes: br#"<Ui>
<Button name="LowButton" enableMouse="true" frameStrata="LOW" frameLevel="20">
  <Size x="200" y="80"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnClick>POINTER_LOG = POINTER_LOG .. "low;"</OnClick></Scripts>
</Button>
<Button name="HighButton" enableMouse="true" frameStrata="DIALOG" frameLevel="1">
  <Size x="160" y="60"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>
    POINTER_LOG = ""
    self:SetHitRectInsets(5, 5, 5, 5)
  </OnLoad><OnMouseDown>
    POINTER_LOG = POINTER_LOG .. "down:" .. arg1 .. ";"
  </OnMouseDown><OnMouseUp>
    POINTER_LOG = POINTER_LOG .. "up:" .. arg1 .. ";"
  </OnMouseUp><OnClick>
    POINTER_LOG = POINTER_LOG .. "click:" .. arg1 .. ";"
    AcceptEULA()
  </OnClick><OnDoubleClick>
    POINTER_LOG = POINTER_LOG .. "double:" .. arg1 .. ";"
  </OnDoubleClick></Scripts>
</Button>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let high_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("HighButton"))
        .ok_or("missing high pointer target")?;

    let down = manager.pointer_button(
        (1920.0 / 1080.0 * 384.0, 384.0),
        UiPointerButton::Left,
        true,
    )?;
    assert_eq!(down.object_index(), Some(high_index));
    assert!(!down.click_activated());
    let up = manager.pointer_button(
        (1920.0 / 1080.0 * 384.0, 384.0),
        UiPointerButton::Left,
        false,
    )?;
    assert_eq!(up.object_index(), Some(high_index));
    assert!(up.click_activated());

    manager.pointer_button_with_click_count(
        (1920.0 / 1080.0 * 384.0, 384.0),
        UiPointerButton::Left,
        true,
        2,
    )?;
    let double_up = manager.pointer_button_with_click_count(
        (1920.0 / 1080.0 * 384.0, 384.0),
        UiPointerButton::Left,
        false,
        2,
    )?;
    assert!(double_up.click_activated());

    let globals = manager.bundle().lua().globals();
    assert_eq!(
        globals.get::<String>("POINTER_LOG")?,
        "down:LeftButton;up:LeftButton;click:LeftButton;down:LeftButton;up:LeftButton;click:LeftButton;double:LeftButton;"
    );
    assert_eq!(manager.cvar_value("readEULA").as_deref(), Some("1"));
    Ok(())
}

/// A primary-button gesture captured by one Glue screen cannot release into
/// an overlapping button exposed by a screen transition during that gesture.
#[test]
fn glue_manager_does_not_click_through_a_held_primary_button() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"HeldPointer.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\HeldPointer.xml",
            bytes: br#"<Ui>
<Frame name="CharacterSelectScreen">
  <Size x="600" y="400"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>SetCurrentScreen("charselect")</OnLoad></Scripts>
  <Frames><Button name="CharacterSelectBackButton" enableMouse="true">
    <Size x="150" y="38"/><Anchors><Anchor point="BOTTOMRIGHT"/></Anchors>
    <Scripts><OnMouseDown>
      CharacterSelectScreen:Hide()
      AccountLoginScreen:Show()
      SetCurrentScreen("login")
    </OnMouseDown></Scripts>
  </Button></Frames>
</Frame>
<Frame name="AccountLoginScreen" hidden="true">
  <Size x="600" y="400"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Frames><Button name="AccountLoginExitButton" enableMouse="true">
    <Size x="150" y="38"/><Anchors><Anchor point="BOTTOMRIGHT"/></Anchors>
    <Scripts><OnClick>QuitGame()</OnClick></Scripts>
  </Button></Frames>
</Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let position = (900.0, 200.0);

    let down = manager.pointer_button(position, UiPointerButton::Left, true)?;
    assert!(down.object_index().is_some());
    assert_eq!(manager.current_screen(), "login");
    let up = manager.pointer_button(position, UiPointerButton::Left, false)?;

    assert!(!up.click_activated());
    assert_eq!(manager.take_process_action(), None);
    Ok(())
}

/// ScrollFrame's native wheel admission drives authored `OnMouseWheel` and
/// clamped `OnVerticalScroll` state without a runtime-invented scroll speed.
#[test]
fn glue_manager_routes_authored_scroll_frame_wheel() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Scroll.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Scroll.xml",
            bytes: br#"<Ui>
<ScrollFrame name="LegalScroll" frameStrata="DIALOG" frameLevel="3">
  <Size x="200" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Frames><Slider name="$parentScrollBar"><Size x="20" y="100"/><Scripts><OnLoad>
    self:SetMinMaxValues(0, 300)
  </OnLoad><OnValueChanged>
    self:GetParent():SetVerticalScroll(value)
  </OnValueChanged></Scripts></Slider></Frames>
  <Scripts><OnLoad>SCROLL_OFFSET = 0</OnLoad><OnMouseWheel>
    LegalScrollScrollBar:SetValue(LegalScrollScrollBar:GetValue() - delta * 50)
  </OnMouseWheel><OnVerticalScroll>
    SCROLL_OFFSET = offset
  </OnVerticalScroll></Scripts>
  <ScrollChild><Frame name="LegalText"><Size x="200" y="400"/></Frame></ScrollChild>
</ScrollFrame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let scroll_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("LegalScroll"))
        .ok_or("missing legal scroll frame")?;

    assert_eq!(
        manager
            .scroll_frames()
            .state(scroll_index)
            .ok_or("missing initial scroll state")?
            .range(),
        (0.0, 300.0)
    );
    let target = manager.pointer_wheel((1920.0 / 1080.0 * 384.0, 384.0), -1.0)?;
    assert_eq!(target, Some(scroll_index));
    assert_eq!(
        manager
            .scroll_frames()
            .state(scroll_index)
            .ok_or("missing updated scroll state")?
            .offset(),
        (0.0, 50.0)
    );
    assert_eq!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<f64>("SCROLL_OFFSET")?,
        50.0
    );
    Ok(())
}

/// Native hover boundaries drive authored enter/leave handlers, button
/// highlight presentation, and pointer handlers on non-button model frames.
#[test]
fn glue_manager_routes_hover_and_generic_frame_pointer_handlers() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Hover.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Hover.xml",
            bytes: br#"<Ui>
<Button name="HoverButton" enableMouse="true" frameStrata="DIALOG" frameLevel="2">
  <Size x="160" y="60"/><Anchors><Anchor point="CENTER"/></Anchors>
  <HighlightTexture name="$parentHighlight" file="Interface\Glues\Hover"/>
  <Scripts><OnLoad>POINTER_LOG = ""</OnLoad>
    <OnEnter>BUTTON_MOUSE_OVER = self:IsMouseOver(); POINTER_LOG = POINTER_LOG .. "button-enter;"</OnEnter>
    <OnLeave>BUTTON_MOUSE_LEFT = not self:IsMouseOver(); POINTER_LOG = POINTER_LOG .. "button-leave;"</OnLeave>
  </Scripts>
</Button>
<ModelFFX name="CharacterModel" enableMouse="true" frameStrata="DIALOG" frameLevel="1">
  <Size x="100" y="100"/><Anchors><Anchor point="BOTTOMLEFT"><Offset><AbsDimension x="50" y="50"/></Offset></Anchor></Anchors>
  <Scripts>
    <OnEnter>POINTER_LOG = POINTER_LOG .. "model-enter;"</OnEnter>
    <OnLeave>POINTER_LOG = POINTER_LOG .. "model-leave;"</OnLeave>
    <OnMouseDown>
      CURSOR_X, CURSOR_Y = GetCursorPosition()
      POINTER_LOG = POINTER_LOG .. "model-down:" .. arg1 .. ";"
    </OnMouseDown>
    <OnMouseUp>POINTER_LOG = POINTER_LOG .. "model-up:" .. arg1 .. ";"</OnMouseUp>
  </Scripts>
</ModelFFX>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let button_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("HoverButton"))
        .ok_or("missing hover button")?;
    let highlight_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("HoverButtonHighlight"))
        .ok_or("missing highlight texture")?;
    let model_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("CharacterModel"))
        .ok_or("missing model pointer target")?;
    let is_presented = |manager: &GlueManager, object_index| {
        manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .any(|member| member.object_index() == object_index && member.opacity() > 0.0)
    };

    assert!(!is_presented(&manager, highlight_index));
    let snapshots = manager.runtime_snapshot_count();
    let mesh_identity = manager.render_plan().mesh().geometry_identity();
    let vertex_bytes = manager.render_plan().mesh().vertex_bytes().to_vec();
    let index_bytes = manager.render_plan().mesh().index_bytes().to_vec();
    let button_bounds = manager
        .geometry()
        .region(button_index)
        .ok_or("missing button geometry")?
        .presentation_bounds();
    assert_eq!(
        manager.pointer_motion((
            (button_bounds.left() + button_bounds.right()) * 0.5,
            (button_bounds.bottom() + button_bounds.top()) * 0.5,
        ))?,
        Some(button_index)
    );
    assert!(is_presented(&manager, highlight_index));
    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    assert!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<bool>("BUTTON_MOUSE_OVER")?
    );
    assert_eq!(
        manager.render_plan().mesh().geometry_identity(),
        mesh_identity
    );
    assert_eq!(manager.render_plan().mesh().vertex_bytes(), vertex_bytes);
    assert_eq!(manager.render_plan().mesh().index_bytes(), index_bytes);

    let model_bounds = manager
        .geometry()
        .region(model_index)
        .ok_or("missing model geometry")?
        .presentation_bounds();
    let model_center = (
        (model_bounds.left() + model_bounds.right()) * 0.5,
        (model_bounds.bottom() + model_bounds.top()) * 0.5,
    );
    let down = manager.pointer_button(model_center, UiPointerButton::Left, true)?;
    assert_eq!(manager.pointer_motion(model_center)?, Some(model_index));
    let up = manager.pointer_button(model_center, UiPointerButton::Left, false)?;
    assert_eq!(down.object_index(), Some(model_index));
    assert_eq!(up.object_index(), Some(model_index));
    assert!(!is_presented(&manager, highlight_index));
    assert!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<bool>("BUTTON_MOUSE_LEFT")?
    );
    let globals = manager.bundle().lua().globals();
    let cursor_scale = 1080.0 / 768.0;
    assert!((globals.get::<f64>("CURSOR_X")? - model_center.0 * cursor_scale).abs() < 0.000_01);
    assert!((globals.get::<f64>("CURSOR_Y")? - model_center.1 * cursor_scale).abs() < 0.000_01);
    assert_eq!(
        globals.get::<String>("POINTER_LOG")?,
        "button-enter;button-leave;model-enter;model-down:LeftButton;model-up:LeftButton;"
    );
    Ok(())
}

/// Authored hover tooltips materialize once, then toggle through the visual journal.
#[test]
fn glue_manager_retains_authored_hover_visibility() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"HoverVisibility.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\HoverVisibility.xml",
            bytes: br#"<Ui>
<Button name="HoverVisibilityButton" enableMouse="true" frameStrata="DIALOG" frameLevel="2">
  <Size x="160" y="60"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts>
    <OnEnter>HoverVisibilityTip:Show()</OnEnter>
    <OnLeave>HoverVisibilityTip:Hide()</OnLeave>
  </Scripts>
</Button>
<Frame name="HoverVisibilityTip" hidden="true" frameStrata="TOOLTIP" frameLevel="4">
  <Size x="120" y="40"/><Anchors><Anchor point="TOP" relativeTo="HoverVisibilityButton" relativePoint="BOTTOM"/></Anchors>
  <Layers><Layer level="ARTWORK">
    <Texture name="$parentTexture" file="Interface\Glues\Hover"><Size x="120" y="40"/><Color r="0.8" g="0.2" b="0.1" a="1"/></Texture>
  </Layer></Layers>
</Frame>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let button = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("HoverVisibilityButton"))
        .ok_or("missing hover button")?;
    let texture = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("HoverVisibilityTipTexture"))
        .ok_or("missing tooltip texture")?;
    let shown = |manager: &GlueManager| {
        manager
            .presentation()
            .members_in_draw_order()
            .iter()
            .any(|member| member.object_index() == texture && member.opacity() > 0.0)
    };
    let bounds = manager
        .geometry()
        .region(button)
        .ok_or("missing button geometry")?
        .presentation_bounds();
    let center = (
        (bounds.left() + bounds.right()) * 0.5,
        (bounds.bottom() + bounds.top()) * 0.5,
    );
    let snapshots = manager.runtime_snapshot_count();

    assert!(!shown(&manager));
    assert_eq!(manager.pointer_motion(center)?, Some(button));
    assert!(shown(&manager));
    assert_eq!(manager.pointer_motion((-1.0, -1.0))?, Some(button));
    assert!(!shown(&manager));
    let retained_identity = manager.render_plan().mesh().geometry_identity();
    assert_eq!(manager.pointer_motion(center)?, Some(button));
    assert!(shown(&manager));
    assert_eq!(
        manager.render_plan().mesh().geometry_identity(),
        retained_identity
    );
    assert_eq!(manager.runtime_snapshot_count(), snapshots);
    Ok(())
}

/// CheckButton mutates its native checked state before `OnClick`, including
/// character creation's mutually exclusive race/class/gender groups.
#[test]
fn glue_manager_applies_native_check_button_click_state() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Checks.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Checks.xml",
            bytes: br#"<Ui><Frame name="Choices">
<CheckButton name="CharacterCreateRaceButton1" enableMouse="true">
  <Size x="80" y="40"/><Anchors><Anchor point="CENTER"><Offset><AbsDimension x="-60" y="0"/></Offset></Anchor></Anchors>
  <Scripts><OnLoad>self:SetChecked(1)</OnLoad></Scripts>
</CheckButton>
<CheckButton name="CharacterCreateRaceButton2" enableMouse="true">
  <Size x="80" y="40"/><Anchors><Anchor point="CENTER"><Offset><AbsDimension x="60" y="0"/></Offset></Anchor></Anchors>
  <Scripts><OnLoad>self:SetChecked(nil)</OnLoad><OnClick>
    CLICK_SAW_CHECKED = self:GetChecked()
    CLICK_SAW_FIRST = CharacterCreateRaceButton1:GetChecked()
  </OnClick></Scripts>
</CheckButton>
<CheckButton name="IndependentCheck" enableMouse="true">
  <Size x="80" y="40"/><Anchors><Anchor point="CENTER"><Offset><AbsDimension x="0" y="-80"/></Offset></Anchor></Anchors>
</CheckButton>
</Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let click = |manager: &mut GlueManager, name| -> Result<(), Box<dyn Error>> {
        let index = manager
            .objects()
            .iter()
            .position(|object| object.name() == Some(name))
            .ok_or_else(|| format!("missing check button {name}"))?;
        let bounds = manager
            .geometry()
            .region(index)
            .ok_or_else(|| format!("missing check button geometry {name}"))?
            .presentation_bounds();
        let center = (
            (bounds.left() + bounds.right()) * 0.5,
            (bounds.bottom() + bounds.top()) * 0.5,
        );
        let down = manager.pointer_button(center, UiPointerButton::Left, true)?;
        let up = manager.pointer_button(center, UiPointerButton::Left, false)?;
        if down.object_index() != Some(index) || up.object_index() != Some(index) {
            return Err(
                format!("check button {name} was not the pointer target: {down:?} {up:?}").into(),
            );
        }
        Ok(())
    };

    click(&mut manager, "CharacterCreateRaceButton2")?;
    let globals = manager.bundle().lua().globals();
    assert!(globals.get::<bool>("CLICK_SAW_CHECKED")?);
    assert!(!globals.get::<bool>("CLICK_SAW_FIRST")?);
    click(&mut manager, "CharacterCreateRaceButton2")?;
    assert!(globals.get::<bool>("CLICK_SAW_CHECKED")?);
    click(&mut manager, "IndependentCheck")?;
    let independent = globals.get::<mlua::Table>("IndependentCheck")?;
    let get_checked = independent.get::<mlua::Function>("GetChecked")?;
    assert!(get_checked.call::<bool>(independent.clone())?);
    click(&mut manager, "IndependentCheck")?;
    assert!(!get_checked.call::<bool>(independent)?);
    Ok(())
}

/// Slider is a native mouse target whose vertical thumb follows its value and
/// whose capture prevents clicks from falling through to lower Glue frames.
#[test]
fn glue_manager_routes_vertical_slider_click_and_drag() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Slider.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Slider.xml",
            bytes: br#"<Ui>
<Button name="UnderButton" enableMouse="true" frameStrata="DIALOG" frameLevel="1">
  <Size x="20" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>UNDER_CLICKS = 0</OnLoad><OnClick>UNDER_CLICKS = UNDER_CLICKS + 1</OnClick></Scripts>
</Button>
<Slider name="LegalSlider" frameStrata="DIALOG" frameLevel="2">
  <Size x="20" y="100"/><Anchors><Anchor point="CENTER"/></Anchors>
  <ThumbTexture><Size x="18" y="24"/></ThumbTexture>
  <Scripts><OnLoad>
    self:SetMinMaxValues(0, 300)
    self:SetValueStep(25)
  </OnLoad><OnValueChanged>SLIDER_VALUE = value</OnValueChanged></Scripts>
</Slider>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let slider_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("LegalSlider"))
        .ok_or("missing slider")?;
    let thumb_index = manager
        .objects()
        .iter()
        .position(|object| {
            object.parent() == Some(slider_index) && object.role() == UiObjectRole::ThumbTexture
        })
        .ok_or("missing slider thumb")?;
    let track = manager
        .geometry()
        .region(slider_index)
        .ok_or("missing slider geometry")?
        .presentation_bounds();
    let initial_thumb = manager
        .geometry()
        .region(thumb_index)
        .ok_or("missing initial thumb geometry")?
        .presentation_bounds();
    assert!((initial_thumb.top() - track.top()).abs() < 0.001);

    let top = ((track.left() + track.right()) * 0.5, track.top());
    let bottom = ((track.left() + track.right()) * 0.5, track.bottom());
    let down = manager.pointer_button(top, UiPointerButton::Left, true)?;
    assert_eq!(down.object_index(), Some(slider_index));
    assert_eq!(
        manager.pointer_motion_deferred_refresh(bottom)?,
        Some(slider_index)
    );
    assert_eq!(
        manager
            .geometry()
            .region(thumb_index)
            .ok_or("missing retained thumb geometry")?
            .presentation_bounds(),
        initial_thumb
    );
    assert!(manager.flush_deferred_refresh()?);
    assert!(!manager.flush_deferred_refresh()?);
    let up = manager.pointer_button(bottom, UiPointerButton::Left, false)?;
    assert_eq!(up.object_index(), Some(slider_index));
    assert!(!up.click_activated());

    let globals = manager.bundle().lua().globals();
    assert_eq!(globals.get::<f64>("SLIDER_VALUE")?, 300.0);
    assert_eq!(globals.get::<u32>("UNDER_CLICKS")?, 0);
    let final_thumb = manager
        .geometry()
        .region(thumb_index)
        .ok_or("missing final thumb geometry")?
        .presentation_bounds();
    assert!((final_thumb.bottom() - track.bottom()).abs() < 0.001);
    Ok(())
}

/// An inherited authored horizontal axis controls pointer values and thumb
/// placement instead of silently retaining `CSimpleSlider`'s vertical default.
#[test]
fn glue_manager_honors_inherited_horizontal_slider_orientation() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Slider.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Slider.xml",
            bytes: br#"<Ui>
<Slider name="HorizontalSliderTemplate" orientation="HORIZONTAL" virtual="true">
  <Size x="100" y="20"/><ThumbTexture><Size x="20" y="20"/></ThumbTexture>
</Slider>
<Slider name="HorizontalSlider" inherits="HorizontalSliderTemplate">
  <Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>self:SetMinMaxValues(0, 100)</OnLoad></Scripts>
</Slider>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let slider_index = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("HorizontalSlider"))
        .ok_or("missing horizontal slider")?;
    let thumb_index = manager
        .objects()
        .iter()
        .position(|object| {
            object.parent() == Some(slider_index) && object.role() == UiObjectRole::ThumbTexture
        })
        .ok_or("missing horizontal slider thumb")?;
    let track = manager
        .geometry()
        .region(slider_index)
        .ok_or("missing horizontal slider geometry")?
        .presentation_bounds();
    let middle_y = (track.bottom() + track.top()) * 0.5;
    let left = (track.left(), middle_y);
    let right = (track.right(), middle_y);
    assert_eq!(
        manager
            .pointer_button(left, UiPointerButton::Left, true)?
            .object_index(),
        Some(slider_index)
    );
    assert_eq!(manager.pointer_motion(right)?, Some(slider_index));
    assert_eq!(
        manager
            .pointer_button(right, UiPointerButton::Left, false)?
            .object_index(),
        Some(slider_index)
    );
    let slider = manager
        .bundle()
        .lua()
        .globals()
        .get::<mlua::Table>("HorizontalSlider")?;
    assert_eq!(
        slider
            .get::<mlua::Function>("GetOrientation")?
            .call::<String>(slider.clone())?,
        "HORIZONTAL"
    );
    assert_eq!(
        slider
            .get::<mlua::Function>("GetValue")?
            .call::<f64>(slider)?,
        100.0
    );
    let thumb = manager
        .geometry()
        .region(thumb_index)
        .ok_or("missing horizontal thumb geometry")?
        .presentation_bounds();
    assert!((thumb.right() - track.right()).abs() < 0.001);
    Ok(())
}

/// Stock login globals preserve action order and expose runtime-owned status.
#[test]
fn glue_manager_bridges_login_actions_without_exposing_passwords() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Network.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Network.xml",
            bytes: br#"<Ui><Frame name="Network"><Scripts><OnLoad>
  DefaultServerLogin("Account", "Secret")
  CancelLogin()
  StatusDialogClick()
  DisconnectFromServer()
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    let globals = manager.bundle().lua().globals();
    let server_name = globals.get::<mlua::Function>("GetServerName")?;
    let connected = globals.get::<mlua::Function>("IsConnectedToServer")?;

    assert_eq!(server_name.call::<Option<String>>(())?, None);
    assert!(!connected.call::<bool>(())?);
    let UiGlueNetworkAction::Login(request) = manager
        .take_network_action()
        .ok_or("missing login action")?
    else {
        return Err("first network action was not login".into());
    };
    assert_eq!(request.account_name(), "Account");
    assert_eq!(request.password_bytes(), b"Secret");
    let diagnostic = format!("{request:?}");
    assert!(diagnostic.contains("<redacted>"));
    assert!(!diagnostic.contains("Secret"));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::CancelLogin)
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::StatusDialogClick)
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::Disconnect)
    ));
    assert!(manager.take_network_action().is_none());

    manager.set_network_status(
        UiGlueNetworkStatus::new(Some("Local Realm".to_owned()), true)
            .with_realm_rules(true, false, true),
    );
    let server_values = server_name.call::<mlua::MultiValue>(())?;
    assert_eq!(server_values.len(), 4);
    assert_eq!(
        server_values[0]
            .as_string()
            .map(|value| value.to_string_lossy()),
        Some("Local Realm".to_owned())
    );
    assert_eq!(lua_numeric(&server_values[1]), Some(1.0));
    assert!(server_values[2].is_nil());
    assert_eq!(lua_numeric(&server_values[3]), Some(1.0));
    assert!(connected.call::<bool>(())?);
    Ok(())
}

/// Stock RealmList globals retain one-based category semantics and fourteen row values.
#[test]
fn glue_manager_bridges_stock_realm_list_globals() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Realm.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Realm.xml",
            bytes: br#"<Ui><Frame name="RealmBridge"/></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;
    manager.set_realm_directory(UiRealmDirectory::new(
        vec![
            UiRealmCategory::new(1, "Empty".to_owned(), Vec::new()),
            UiRealmCategory::new(
                2,
                "United States".to_owned(),
                vec![UiRealmInfo::new(
                    41,
                    "Azeroth".to_owned(),
                    5,
                    UiRealmFlags::new(false, false, false, true, false),
                    -3.0,
                    None,
                )],
            ),
            UiRealmCategory::new(
                3,
                "Oceanic".to_owned(),
                vec![UiRealmInfo::new(
                    42,
                    "Kalimdor".to_owned(),
                    0,
                    UiRealmFlags::new(true, true, true, true, true),
                    2.0,
                    Some(UiRealmVersion::new(3, 3, 5, 12_340, 1)),
                )],
            )
            .with_eligibility(true, true, true),
        ],
        Some(42),
    ));
    let globals = manager.bundle().lua().globals();

    let categories = globals.get::<mlua::Function>("GetRealmCategories")?;
    let categories = categories.call::<mlua::MultiValue>(())?;
    assert_eq!(categories.len(), 2);
    assert_eq!(
        categories[0]
            .as_string()
            .map(|value| value.to_string_lossy()),
        Some("United States".to_owned())
    );
    assert_eq!(
        categories[1]
            .as_string()
            .map(|value| value.to_string_lossy()),
        Some("Oceanic".to_owned())
    );
    let count = globals.get::<mlua::Function>("GetNumRealms")?;
    assert_eq!(count.call::<usize>(())?, 2);
    assert_eq!(count.call::<usize>(1_u32)?, 1);
    let selected = globals.get::<mlua::Function>("GetSelectedCategory")?;
    assert_eq!(selected.call::<u32>(())?, 2);

    let realm_info = globals.get::<mlua::Function>("GetRealmInfo")?;
    let values = realm_info.call::<mlua::MultiValue>((2_u32, 1_u32))?;
    assert_eq!(values.len(), 14);
    assert_eq!(
        values[0].as_string().map(|value| value.to_string_lossy()),
        Some("Kalimdor".to_owned())
    );
    assert_eq!(lua_numeric(&values[1]), Some(0.0));
    assert_eq!(lua_numeric(&values[2]), Some(1.0));
    assert_eq!(lua_numeric(&values[3]), Some(1.0));
    assert_eq!(lua_numeric(&values[4]), Some(1.0));
    assert_eq!(lua_numeric(&values[5]), Some(1.0));
    assert_eq!(lua_numeric(&values[6]), Some(1.0));
    assert_eq!(lua_numeric(&values[7]), Some(2.0));
    assert_eq!(lua_numeric(&values[8]), Some(1.0));
    assert_eq!(lua_numeric(&values[9]), Some(3.0));
    assert_eq!(lua_numeric(&values[12]), Some(12_340.0));
    assert_eq!(lua_numeric(&values[13]), Some(1.0));
    let invalid_locale = globals.get::<mlua::Function>("IsInvalidLocale")?;
    assert!(invalid_locale.call::<bool>(2_u32)?);

    globals.raw_set("REALM_LIST_IN_PROGRESS", "Retrieving realm list")?;
    globals
        .get::<mlua::Function>("RequestRealmList")?
        .call::<()>(true)?;
    globals
        .get::<mlua::Function>("CancelRealmListQuery")?
        .call::<()>(())?;
    globals
        .get::<mlua::Function>("ChangeRealm")?
        .call::<()>((1_u32, 1_u32))?;
    globals
        .get::<mlua::Function>("SetPreferredInfo")?
        .call::<()>((2_u32, true, true))?;
    globals
        .get::<mlua::Function>("SortRealms")?
        .call::<()>("name")?;
    globals
        .get::<mlua::Function>("SetCurrentScreen")?
        .call::<()>("login")?;
    globals
        .get::<mlua::Function>("RealmListDialogCancelled")?
        .call::<()>(())?;

    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::RequestRealmList {
            show_progress_dialog: true,
            status_message: Some(message),
        }) if message == "Retrieving realm list"
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::CancelRealmListQuery)
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::ChangeRealm { realm_id: 41 })
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::SetPreferredRealmInfo {
            category_index: 2,
            player_killing_allowed: true,
            roleplaying: true,
        })
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::SortRealms {
            sort: solarity_ui::UiRealmSort::Name,
        })
    ));
    assert!(matches!(
        manager.take_network_action(),
        Some(UiGlueNetworkAction::RealmListDialogCancelled {
            from_login_screen: true
        })
    ));
    Ok(())
}

/// Stock shared Lua runtime exports both quit spellings to one ordered process action.
#[test]
fn glue_manager_bridges_quit_and_quit_game_to_process_owner() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Process.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Process.xml",
            bytes: br#"<Ui><Frame name="Process"><Scripts><OnLoad>
  Quit()
  QuitGame()
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1920, 1080), false)?;

    assert_eq!(manager.take_process_action(), Some(UiProcessAction::Quit));
    assert_eq!(manager.take_process_action(), Some(UiProcessAction::Quit));
    assert_eq!(manager.take_process_action(), None);
    Ok(())
}

/// Realm sorting preserves stock's promoted four-key precedence and direction toggles.
#[test]
fn realm_directory_applies_stock_sort_generations() {
    let realm = |id, name: &str, characters, load, pvp, rp| {
        UiRealmInfo::new(
            id,
            name.to_owned(),
            characters,
            UiRealmFlags::new(false, false, false, pvp, rp),
            load,
            None,
        )
    };
    let mut directory = UiRealmDirectory::new(
        vec![UiRealmCategory::new(
            1,
            "Test".to_owned(),
            vec![
                realm(1, "A", 2, 1.0, false, false),
                realm(2, "b", 1, 2.0, true, false),
                realm(3, "C", 1, 0.0, false, true),
                realm(4, "d", 1, 0.0, false, false),
            ],
        )],
        None,
    );
    let names = |directory: &UiRealmDirectory| {
        directory.categories()[0]
            .realms()
            .iter()
            .map(|realm| realm.name().to_owned())
            .collect::<Vec<_>>()
    };

    directory.sort(solarity_ui::UiRealmSort::Name);
    assert_eq!(names(&directory), ["A", "b", "C", "d"]);
    directory.sort(solarity_ui::UiRealmSort::Name);
    assert_eq!(names(&directory), ["d", "C", "b", "A"]);
    directory.sort(solarity_ui::UiRealmSort::Characters);
    assert_eq!(names(&directory), ["d", "C", "b", "A"]);
    directory.sort(solarity_ui::UiRealmSort::Characters);
    assert_eq!(names(&directory), ["A", "d", "C", "b"]);
    directory.sort(solarity_ui::UiRealmSort::Load);
    assert_eq!(names(&directory), ["d", "C", "A", "b"]);
    directory.sort(solarity_ui::UiRealmSort::Mode);
    assert_eq!(names(&directory), ["d", "A", "C", "b"]);
    directory.sort(solarity_ui::UiRealmSort::Mode);
    assert_eq!(names(&directory), ["b", "C", "d", "A"]);
}

/// Reads either Lua numeric representation without changing value semantics.
fn lua_numeric(value: &mlua::Value) -> Option<f64> {
    match value {
        mlua::Value::Integer(value) => i32::try_from(*value).ok().map(f64::from),
        mlua::Value::Number(value) => Some(*value),
        _ => None,
    }
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
    if screen == "login" then Visual:Hide() else Visual:Show() end
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
    let mesh_identity = manager.render_plan().mesh().geometry_identity();
    let vertex_bytes = manager.render_plan().mesh().vertex_bytes().to_vec();
    let index_bytes = manager.render_plan().mesh().index_bytes().to_vec();

    let dispatch = manager.dispatch_event("set_glue_screen", &payload)?;

    assert_eq!(dispatch.subscriber_count(), 2);
    assert_eq!(manager.presentation().member_count(), 1);
    assert_eq!(
        manager.presentation().members_in_draw_order()[0].opacity(),
        0.0
    );
    assert_eq!(
        manager.render_plan().mesh().geometry_identity(),
        mesh_identity
    );
    assert_eq!(manager.render_plan().mesh().vertex_bytes(), vertex_bytes);
    assert_eq!(manager.render_plan().mesh().index_bytes(), index_bytes);
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

    let reveal_payload = UiEventPayload::new([
        UiEventArgument::String("charselect".to_owned()),
        UiEventArgument::Integer(8),
    ])?;
    manager.dispatch_event("SET_GLUE_SCREEN", &reveal_payload)?;
    assert_eq!(
        manager.presentation().members_in_draw_order()[0].opacity(),
        1.0
    );
    assert_eq!(
        manager.render_plan().mesh().geometry_identity(),
        mesh_identity
    );
    assert_eq!(manager.render_plan().mesh().vertex_bytes(), vertex_bytes);
    assert_eq!(manager.render_plan().mesh().index_bytes(), index_bytes);
    Ok(())
}

/// Only visible frames receive one elapsed interval per native update, and a
/// presentation rebuild occurs only when authored state actually changes.
#[test]
fn glue_manager_advances_visible_on_update_handlers_once() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"Update.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\Update.xml",
            bytes: br#"<Ui><Frame name="Animated"><Size x="100" y="50"/><Scripts>
  <OnLoad>UPDATE_CALLS = 0 self:RegisterEvent("SET_GLUE_SCREEN")</OnLoad>
  <OnEvent>if arg1 == "charselect" then self:Hide() end</OnEvent>
  <OnUpdate>UPDATE_CALLS = UPDATE_CALLS + 1 self:SetAlpha(self:GetAlpha() - elapsed)</OnUpdate>
</Scripts></Frame>
<Button name="StateButton"><Size x="80" y="30"/><Anchors><Anchor point="TOP"/></Anchors>
  <NormalTexture name="$parentNormal" file="Interface\Glues\Normal"/>
  <DisabledTexture name="$parentDisabled" file="Interface\Glues\Disabled"/>
  <Scripts><OnLoad>DISABLE_NEXT = false</OnLoad><OnUpdate>
    if DISABLE_NEXT then DISABLE_NEXT = false self:Disable() end
  </OnUpdate></Scripts>
</Button>
<Frame name="LayoutUpdate"><Size x="90" y="20"/><Scripts>
  <OnLoad>LAYOUT_NEXT = false</OnLoad>
  <OnUpdate>if LAYOUT_NEXT then LAYOUT_NEXT = false self:SetWidth(240) end</OnUpdate>
</Scripts></Frame>
<Frame name="DynamicUpdate"><Scripts><OnLoad>
  DYNAMIC_UPDATE_CALLS = 0
  self:SetScript("OnUpdate", function(frame)
    DYNAMIC_UPDATE_CALLS = DYNAMIC_UPDATE_CALLS + 1
    frame:SetScript("OnUpdate", nil)
  end)
</OnLoad></Scripts></Frame></Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let mut manager = GlueManager::start(AssetStore::mount(catalog)?, (1280, 720), false)?;
    let animated = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("Animated"))
        .ok_or("Animated fixture frame is absent")?;
    let normal = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("StateButtonNormal"))
        .ok_or("StateButton normal texture is absent")?;
    let disabled = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("StateButtonDisabled"))
        .ok_or("StateButton disabled texture is absent")?;
    let layout = manager
        .objects()
        .iter()
        .position(|object| object.name() == Some("LayoutUpdate"))
        .ok_or("LayoutUpdate fixture frame is absent")?;

    assert!(manager.update(0.25)?);
    assert_close(
        manager
            .geometry()
            .region(animated)
            .ok_or("Animated geometry is absent")?
            .effective_alpha(),
        0.75,
    );
    assert_eq!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<u32>("UPDATE_CALLS")?,
        1
    );
    assert_eq!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<u32>("DYNAMIC_UPDATE_CALLS")?,
        1
    );
    let snapshots_after_dynamic_update = manager.runtime_snapshot_count();
    manager.bundle().lua().globals().set("DISABLE_NEXT", true)?;
    assert!(manager.update(0.0)?);
    assert_eq!(
        manager.runtime_snapshot_count(),
        snapshots_after_dynamic_update
    );
    assert_eq!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<u32>("DYNAMIC_UPDATE_CALLS")?,
        1
    );
    manager
        .bundle()
        .lua()
        .load(
            r#"DynamicUpdate:SetScript("OnUpdate", function(frame)
          DYNAMIC_UPDATE_CALLS = DYNAMIC_UPDATE_CALLS + 1
          frame:SetScript("OnUpdate", nil)
        end)"#,
        )
        .exec()?;
    manager.update(0.0)?;
    assert_eq!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<u32>("DYNAMIC_UPDATE_CALLS")?,
        2
    );
    let presented = manager
        .presentation()
        .members_in_draw_order()
        .iter()
        .filter(|member| member.opacity() > 0.0)
        .map(|member| member.object_index())
        .collect::<Vec<_>>();
    assert!(!presented.contains(&normal));
    assert!(presented.contains(&disabled));

    let snapshots_before_layout = manager.runtime_snapshot_count();
    manager.bundle().lua().globals().set("LAYOUT_NEXT", true)?;
    assert!(manager.update(0.0)?);
    assert_eq!(manager.runtime_snapshot_count(), snapshots_before_layout);
    assert_close(
        manager
            .geometry()
            .region(layout)
            .ok_or("LayoutUpdate geometry is absent")?
            .logical_bounds()
            .width(),
        240.0,
    );

    manager.dispatch_event(
        "SET_GLUE_SCREEN",
        &UiEventPayload::new([UiEventArgument::String("charselect".to_owned())])?,
    )?;
    assert!(!manager.update(0.25)?);
    assert_eq!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<u32>("UPDATE_CALLS")?,
        4
    );
    assert!(matches!(
        manager.update(-0.01),
        Err(UiEventError::Script(_))
    ));
    Ok(())
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.000_001,
        "{actual} != {expected}"
    );
}

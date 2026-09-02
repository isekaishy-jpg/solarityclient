//! External stock-compatibility tests for persistent GlueXML ownership.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, AssetStoreHandle, ClientDataRoot, Locale};
use solarity_ui::{
    GlueError, GlueInitialScreen, GlueManager, UiEventArgument, UiEventError, UiEventPayload,
    UiGlueNetworkAction, UiGlueNetworkStatus, UiLayoutError, UiObjectKind, UiObjectRole,
    UiPointerButton, UiProcessAction, UiRealmCategory, UiRealmDirectory, UiRealmFlags, UiRealmInfo,
    UiRealmVersion,
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
  </OnClick></Scripts>
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

    let globals = manager.bundle().lua().globals();
    assert_eq!(
        globals.get::<String>("POINTER_LOG")?,
        "down:LeftButton;up:LeftButton;click:LeftButton;"
    );
    assert_eq!(manager.cvar_value("readEULA").as_deref(), Some("1"));
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
    assert_eq!(manager.pointer_motion(bottom)?, Some(slider_index));
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
</Scripts></Frame></Ui>"#,
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
        1
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

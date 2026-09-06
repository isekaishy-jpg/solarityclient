//! External stock-compatibility tests for physical binding transitions.

use std::error::Error;

use sdl3::keyboard::Scancode as SdlScanCode;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_runtime::{
    ButtonState, InputBindingPhase, InputBindingRouter, KeyModifiers, KeyStateEvent, MouseButton,
    MouseButtonEvent, MouseWheelDirection, MouseWheelEvent, PlatformEvent, ScanCode, WindowEvent,
    WindowId,
};
use solarity_ui::{UiBindingAssignments, UiBindingCatalog, UiBindingMode};

use crate::support::ClientFixture;

const PRIMARY_WINDOW: WindowId = WindowId::new(17);
const OTHER_WINDOW: WindowId = WindowId::new(18);
const LEFT_SHIFT: u16 = 0x0001;
const LEFT_CONTROL: u16 = 0x0040;
const LEFT_ALT: u16 = 0x0100;

const BINDINGS_XML: &[u8] = br#"<Bindings>
  <Binding name="MOVEFORWARD" runOnUp="true">
    if keystate == "down" then MoveForwardStart() else MoveForwardStop() end
  </Binding>
  <Binding name="STRAFERIGHT" runOnUp="true">
    if keystate == "down" then StrafeRightStart() else StrafeRightStop() end
  </Binding>
  <Binding name="OPENPANEL">OpenPanel()</Binding>
  <Binding name="MAC_MEDIA" platform="mac">MacMedia()</Binding>
</Bindings>"#;

const ASSIGNMENTS: &str = r#"BINDINGMODE 2
bind CTRL-SHIFT-PAGEDOWN MOVEFORWARD
bind CTRL-- MOVEFORWARD
bind ALT-BUTTON2 STRAFERIGHT
bind MOUSEWHEELUP SPELL Frostbolt
bind MOUSEWHEELDOWN OPENPANEL
bind F1 MAC_MEDIA
bind W MOVEFORWARD
bind D STRAFERIGHT
"#;

/// A release uses the pressed assignment even after every modifier is released.
#[test]
fn modified_keyboard_binding_routes_down_repeat_and_up() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, assignments) = binding_state()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    let mut invocations = Vec::new();
    let modifiers = KeyModifiers::from_bits(LEFT_CONTROL | LEFT_SHIFT);

    assert_eq!(
        router.route(
            &key_event(
                PRIMARY_WINDOW,
                SdlScanCode::PageDown,
                ButtonState::Pressed,
                modifiers,
                false,
            ),
            modifiers,
            &assignments,
            &catalog,
            |invocation| invocations.push(snapshot(invocation)),
        ),
        1
    );
    assert_eq!(router.active_release_count(), 1);
    assert_eq!(
        router.route(
            &key_event(
                PRIMARY_WINDOW,
                SdlScanCode::PageDown,
                ButtonState::Pressed,
                modifiers,
                true,
            ),
            modifiers,
            &assignments,
            &catalog,
            |invocation| invocations.push(snapshot(invocation)),
        ),
        0
    );
    assert_eq!(
        router.route(
            &key_event(
                PRIMARY_WINDOW,
                SdlScanCode::PageDown,
                ButtonState::Released,
                KeyModifiers::NONE,
                false,
            ),
            KeyModifiers::NONE,
            &assignments,
            &catalog,
            |invocation| invocations.push(snapshot(invocation)),
        ),
        1
    );

    assert_eq!(
        invocations,
        [
            (
                InputBindingPhase::Down,
                "CTRL-SHIFT-PAGEDOWN".to_owned(),
                Some("MOVEFORWARD".to_owned())
            ),
            (
                InputBindingPhase::Up,
                "CTRL-SHIFT-PAGEDOWN".to_owned(),
                Some("MOVEFORWARD".to_owned())
            ),
        ]
    );
    assert_eq!(router.active_release_count(), 0);
    Ok(())
}

/// A literal minus key remains distinguishable from modifier separators.
#[test]
fn minus_key_resolves_the_exact_ctrl_double_hyphen_chord() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, assignments) = binding_state()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    let mut keys = Vec::new();
    let modifiers = KeyModifiers::from_bits(LEFT_CONTROL);

    let count = router.route(
        &key_event(
            PRIMARY_WINDOW,
            SdlScanCode::Minus,
            ButtonState::Pressed,
            modifiers,
            false,
        ),
        modifiers,
        &assignments,
        &catalog,
        |invocation| keys.push(invocation.assignment().key().as_str().to_owned()),
    );

    assert_eq!(count, 1);
    assert_eq!(keys, ["CTRL--"]);
    Ok(())
}

/// Mouse buttons use stock numbering and wheel events are press-only actions.
#[test]
fn mouse_and_wheel_bindings_use_stock_tokens() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, assignments) = binding_state()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    let mut invocations = Vec::new();
    let alt = KeyModifiers::from_bits(LEFT_ALT);

    for state in [ButtonState::Pressed, ButtonState::Released] {
        router.route(
            &PlatformEvent::MouseButton(MouseButtonEvent {
                window_id: PRIMARY_WINDOW,
                button: MouseButton::Right,
                state,
                click_count: 1,
                x: 40.0,
                y: 50.0,
            }),
            alt,
            &assignments,
            &catalog,
            |invocation| invocations.push(snapshot(invocation)),
        );
    }
    router.route(
        &PlatformEvent::MouseWheel(MouseWheelEvent {
            window_id: PRIMARY_WINDOW,
            x: 0.0,
            y: 1.0,
            direction: MouseWheelDirection::Normal,
        }),
        KeyModifiers::NONE,
        &assignments,
        &catalog,
        |invocation| invocations.push(snapshot(invocation)),
    );
    router.route(
        &PlatformEvent::MouseWheel(MouseWheelEvent {
            window_id: PRIMARY_WINDOW,
            x: 0.0,
            y: 1.0,
            direction: MouseWheelDirection::Flipped,
        }),
        KeyModifiers::NONE,
        &assignments,
        &catalog,
        |invocation| invocations.push(snapshot(invocation)),
    );

    assert_eq!(
        invocations,
        [
            (
                InputBindingPhase::Down,
                "ALT-BUTTON2".to_owned(),
                Some("STRAFERIGHT".to_owned())
            ),
            (
                InputBindingPhase::Up,
                "ALT-BUTTON2".to_owned(),
                Some("STRAFERIGHT".to_owned())
            ),
            (InputBindingPhase::Down, "MOUSEWHEELUP".to_owned(), None),
            (
                InputBindingPhase::Down,
                "MOUSEWHEELDOWN".to_owned(),
                Some("OPENPANEL".to_owned())
            ),
        ]
    );
    Ok(())
}

/// Focus loss releases all held movement in deterministic press order.
#[test]
fn focus_loss_drains_active_run_on_up_bindings() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, assignments) = binding_state()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    let mut invocations = Vec::new();
    for scan_code in [SdlScanCode::W, SdlScanCode::D] {
        router.route(
            &key_event(
                PRIMARY_WINDOW,
                scan_code,
                ButtonState::Pressed,
                KeyModifiers::NONE,
                false,
            ),
            KeyModifiers::NONE,
            &assignments,
            &catalog,
            |invocation| invocations.push(snapshot(invocation)),
        );
    }

    let release_count = router.route(
        &PlatformEvent::Window {
            window_id: PRIMARY_WINDOW,
            event: WindowEvent::FocusLost,
        },
        KeyModifiers::NONE,
        &assignments,
        &catalog,
        |invocation| invocations.push(snapshot(invocation)),
    );

    assert_eq!(release_count, 2);
    assert_eq!(router.active_release_count(), 0);
    assert_eq!(
        invocations.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        [
            InputBindingPhase::Down,
            InputBindingPhase::Down,
            InputBindingPhase::Up,
            InputBindingPhase::Up,
        ]
    );
    assert_eq!(invocations[2].1, "W");
    assert_eq!(invocations[3].1, "D");
    Ok(())
}

/// Foreign windows, horizontal-only wheels, and Mac declarations are not guessed.
#[test]
fn unsupported_or_foreign_transitions_do_not_dispatch() -> Result<(), Box<dyn Error>> {
    let (_fixture, catalog, assignments) = binding_state()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    let mut count = 0;
    let mut dispatch = |_invocation| count += 1;

    assert_eq!(
        router.route(
            &key_event(
                OTHER_WINDOW,
                SdlScanCode::W,
                ButtonState::Pressed,
                KeyModifiers::NONE,
                false,
            ),
            KeyModifiers::NONE,
            &assignments,
            &catalog,
            &mut dispatch,
        ),
        0
    );
    assert_eq!(
        router.route(
            &key_event(
                PRIMARY_WINDOW,
                SdlScanCode::F1,
                ButtonState::Pressed,
                KeyModifiers::NONE,
                false,
            ),
            KeyModifiers::NONE,
            &assignments,
            &catalog,
            &mut dispatch,
        ),
        0
    );
    assert_eq!(
        router.route(
            &PlatformEvent::MouseWheel(MouseWheelEvent {
                window_id: PRIMARY_WINDOW,
                x: 1.0,
                y: 0.0,
                direction: MouseWheelDirection::Normal,
            }),
            KeyModifiers::NONE,
            &assignments,
            &catalog,
            &mut dispatch,
        ),
        0
    );
    assert_eq!(count, 0);
    Ok(())
}

fn binding_state() -> Result<(ClientFixture, UiBindingCatalog, UiBindingAssignments), Box<dyn Error>>
{
    let fixture =
        ClientFixture::with_common_files(&[("Interface\\FrameXML\\Bindings.xml", BINDINGS_XML)])?;
    let data_root = ClientDataRoot::new(fixture.data_root())?;
    let mut assets = AssetStore::mount(ArchiveCatalog::discover(data_root, Locale::EnUs)?)?;
    let catalog = UiBindingCatalog::load_builtin(&mut assets)?;
    let assignments = UiBindingAssignments::parse(ASSIGNMENTS, UiBindingMode::Character, &catalog)?;
    Ok((fixture, catalog, assignments))
}

/// The production dispatch boundary uses FrameXML's Lua state, respects UI
/// capture, publishes rendering changes, and preserves modifier-independent up.
#[test]
fn live_frame_binding_executes_and_releases_through_ui_capture() -> Result<(), Box<dyn Error>> {
    let (_fixture, mut frame) = live_binding_frame()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    let down = key_event(
        PRIMARY_WINDOW,
        SdlScanCode::W,
        ButtonState::Pressed,
        KeyModifiers::NONE,
        false,
    );
    assert_eq!(
        router.route_to_frame(&down, KeyModifiers::NONE, true, &mut frame)?,
        0
    );
    assert_eq!(
        frame.localized_text("BINDING_LOG")?.as_deref(),
        Some("ready;")
    );
    assert_eq!(
        router.route_to_frame(&down, KeyModifiers::NONE, false, &mut frame)?,
        1
    );
    assert_eq!(frame.render_plan().mesh().batches()[0].opacity(), 0.0);
    let repeat = key_event(
        PRIMARY_WINDOW,
        SdlScanCode::W,
        ButtonState::Pressed,
        KeyModifiers::NONE,
        true,
    );
    assert_eq!(
        router.route_to_frame(&repeat, KeyModifiers::NONE, false, &mut frame)?,
        0
    );
    let up = key_event(
        PRIMARY_WINDOW,
        SdlScanCode::W,
        ButtonState::Released,
        KeyModifiers::from_bits(LEFT_SHIFT),
        false,
    );
    assert_eq!(
        router.route_to_frame(&up, KeyModifiers::NONE, true, &mut frame)?,
        1
    );
    assert_eq!(
        frame.localized_text("BINDING_LOG")?.as_deref(),
        Some("ready;down;up;")
    );
    assert_eq!(
        frame.localized_text("keystate")?.as_deref(),
        Some("sentinel")
    );
    for parameter in ["pressure", "angle", "precision"] {
        assert_eq!(frame.localized_text(parameter)?.as_deref(), Some("global"));
    }
    let debug = key_event(
        PRIMARY_WINDOW,
        SdlScanCode::F2,
        ButtonState::Pressed,
        KeyModifiers::NONE,
        false,
    );
    assert_eq!(
        router.route_to_frame(&debug, KeyModifiers::NONE, false, &mut frame)?,
        0
    );
    assert!(frame.invoke_binding("DEBUGONLY", true).is_err());
    assert_eq!(frame.render_plan().mesh().batches()[0].opacity(), 1.0);
    assert_eq!(router.active_release_count(), 0);
    Ok(())
}

/// A failing release must not swallow later releases in the same focus-loss
/// batch, and changes made before the error must reach retained presentation.
#[test]
fn live_frame_binding_error_preserves_mutations_and_drains_all_releases()
-> Result<(), Box<dyn Error>> {
    let (_fixture, mut frame) = live_binding_frame()?;
    let mut router = InputBindingRouter::new(PRIMARY_WINDOW);
    for key in [SdlScanCode::D, SdlScanCode::W] {
        router.route_to_frame(
            &key_event(
                PRIMARY_WINDOW,
                key,
                ButtonState::Pressed,
                KeyModifiers::NONE,
                false,
            ),
            KeyModifiers::NONE,
            false,
            &mut frame,
        )?;
    }
    let background = PlatformEvent::ApplicationDidEnterBackground;
    let error = router
        .route_to_frame(&background, KeyModifiers::NONE, true, &mut frame)
        .err()
        .ok_or("missing authored failure")?;
    assert!(error.to_string().contains("release fault"));
    assert_eq!(router.active_release_count(), 0);
    assert_eq!(
        frame.localized_text("BINDING_LOG")?.as_deref(),
        Some("ready;down;fault;up;")
    );
    assert_eq!(frame.render_plan().mesh().batches()[0].opacity(), 0.25);
    assert_eq!(
        router.route_to_frame(&background, KeyModifiers::NONE, true, &mut frame)?,
        0
    );
    Ok(())
}

fn live_binding_frame() -> Result<(ClientFixture, solarity_ui::FrameManager), Box<dyn Error>> {
    let empty_slots = binding_fixture_dbc(0, 3);
    let crit_base = binding_fixture_dbc(11, 1);
    let crit_ratio = binding_fixture_dbc(1100, 1);
    let fixture = ClientFixture::with_common_files(&[
        ("DBFilesClient\\PaperDollItemFrame.dbc", &empty_slots),
        ("DBFilesClient\\gtChanceToMeleeCritBase.dbc", &crit_base),
        ("DBFilesClient\\gtChanceToMeleeCrit.dbc", &crit_ratio),
        ("DBFilesClient\\gtChanceToSpellCritBase.dbc", &crit_base),
        ("DBFilesClient\\gtChanceToSpellCrit.dbc", &crit_ratio),
        ("DBFilesClient\\gtOCTRegenHP.dbc", &crit_ratio),
        ("DBFilesClient\\gtRegenHPPerSpt.dbc", &crit_ratio),
        ("DBFilesClient\\gtRegenMPPerSpt.dbc", &crit_ratio),
        ("Interface\\FrameXML\\FrameXML.toc", b"LiveBindings.xml\n"),
        (
            "Interface\\FrameXML\\LiveBindings.xml",
            br#"<Ui>
<Frame name="BindingTarget"><Size x="40" y="20"/><Anchors><Anchor point="TOPLEFT"/></Anchors>
  <Scripts><OnLoad>
    BINDING_LOG = "ready;"; keystate = "sentinel"
    pressure = "global"; angle = "global"; precision = "global"
  </OnLoad></Scripts>
  <Layers><Layer level="ARTWORK"><Texture file="Interface\Glues\BindingFixture"/></Layer></Layers>
</Frame></Ui>"#,
        ),
        (
            "Interface\\FrameXML\\Bindings.xml",
            br#"<Bindings>
<Binding name="HOLD" runOnUp="true">
  assert(pressure == (keystate == "down" and 1 or 0))
  assert(angle == -1 and precision == 0)
  BINDING_LOG = BINDING_LOG .. keystate .. ";"
  if keystate == "down" then BindingTarget:Hide() else BindingTarget:Show() end
</Binding>
<Binding name="FAULT" runOnUp="true">
  if keystate == "up" then
    BindingTarget:SetAlpha(0.25)
    BINDING_LOG = BINDING_LOG .. "fault;"
    error("release fault")
  end
</Binding>
<Binding name="DEBUGONLY" debug="true">error("debug command executed")</Binding>
<Binding name="TIMEDMOVEMENT" runOnUp="true">
  if keystate == "down" then
    MoveForwardStart(777); TurnLeftStart("ignored"); JumpOrAscendStart()
  else
    MoveForwardStop(999); TurnLeftStop(); AscendStop()
  end
</Binding>
</Bindings>"#,
        ),
        (
            "WTF\\DefaultBindings.wtf",
            b"bind W HOLD\nbind D FAULT\nbind F2 DEBUGONLY\n",
        ),
    ])?;
    let assets = AssetStore::mount(ArchiveCatalog::discover(
        ClientDataRoot::new(fixture.data_root())?,
        Locale::EnUs,
    )?)?;
    let frame = solarity_ui::FrameManager::start_shared(
        solarity_asset::AssetStoreHandle::new(assets),
        solarity_ui::UiScriptEnvironment::new(1280, 720, false)?,
        &[],
        &solarity_ui::AddonCatalog::default(),
    )?;
    Ok((fixture, frame))
}

#[test]
fn movement_lua_captures_input_clock_and_keeps_release_order() -> Result<(), Box<dyn Error>> {
    use solarity_ui::{UiMovementAction as Action, UiMovementControl as Control};
    let (_fixture, mut frame) = live_binding_frame()?;
    frame.set_input_event_time(u32::MAX - 2);
    frame.invoke_binding("TIMEDMOVEMENT", true)?;
    frame.set_input_event_time(4);
    frame.invoke_binding("TIMEDMOVEMENT", false)?;
    for (action, timestamp_ms) in [
        (
            Action::Hold {
                control: Control::Forward,
                pressed: true,
            },
            u32::MAX - 2,
        ),
        (
            Action::Hold {
                control: Control::TurnLeft,
                pressed: true,
            },
            u32::MAX - 2,
        ),
        (Action::JumpOrAscend, u32::MAX - 2),
        (
            Action::Hold {
                control: Control::Forward,
                pressed: false,
            },
            4,
        ),
        (
            Action::Hold {
                control: Control::TurnLeft,
                pressed: false,
            },
            4,
        ),
        (
            Action::Hold {
                control: Control::Ascend,
                pressed: false,
            },
            4,
        ),
    ] {
        assert_eq!(
            frame.take_movement_command(),
            Some(solarity_ui::UiMovementCommand {
                action,
                timestamp_ms
            })
        );
    }
    assert_eq!(frame.take_movement_command(), None);
    Ok(())
}

fn binding_fixture_dbc(rows: u32, fields: u32) -> Vec<u8> {
    let mut bytes = b"WDBC".to_vec();
    for value in [rows, fields, fields * 4, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.resize(20 + (rows * fields * 4) as usize + 1, 0);
    bytes
}

fn key_event(
    window_id: WindowId,
    scan_code: SdlScanCode,
    state: ButtonState,
    modifiers: KeyModifiers,
    is_repeat: bool,
) -> PlatformEvent {
    PlatformEvent::Key(KeyStateEvent {
        window_id,
        state,
        key_code: None,
        scan_code: Some(ScanCode::new(scan_code as i32)),
        modifiers,
        is_repeat,
    })
}

fn snapshot(
    invocation: solarity_runtime::InputBindingInvocation<'_>,
) -> (InputBindingPhase, String, Option<String>) {
    (
        invocation.phase(),
        invocation.assignment().key().as_str().to_owned(),
        invocation
            .definition()
            .map(|definition| definition.name().to_owned()),
    )
}

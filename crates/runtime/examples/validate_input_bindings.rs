//! Validates that physical Windows input can reach every stock default chord.

use std::collections::HashSet;
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use sdl3::keyboard::Scancode as SdlScanCode;
use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale};
use solarity_runtime::{
    ButtonState, InputBindingRouter, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseWheelDirection, MouseWheelEvent, PlatformEvent, ScanCode, WindowId,
};
use solarity_ui::{
    AddonCatalog, UiBindingAction, UiBindingAssignments, UiBindingCatalog, UiBindingPlatform,
};

const WINDOW: WindowId = WindowId::new(1);
const MODIFIER_COMBINATIONS: [u16; 8] = [
    0x0000, 0x0001, 0x0040, 0x0100, 0x0041, 0x0101, 0x0140, 0x0141,
];

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let data_root = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(usage_error)?;
    let locale = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(usage_error)?
        .parse::<Locale>()?;
    if arguments.next().is_some() {
        return Err(usage_error().into());
    }

    let data_root = ClientDataRoot::new(data_root)?;
    let archives = ArchiveCatalog::discover(data_root, locale)?;
    let mut store = AssetStore::mount(archives)?;
    let addons = AddonCatalog::discover(&mut store)?;
    let mut catalog = UiBindingCatalog::load_builtin(&mut store)?;
    for addon in addons.addons() {
        catalog.append_addon(&mut store, addon)?;
    }
    let assignments = UiBindingAssignments::load_defaults(&mut store, &catalog)?;
    let mut router = InputBindingRouter::new(WINDOW);
    let mut reached = HashSet::new();

    for bits in MODIFIER_COMBINATIONS {
        let modifiers = KeyModifiers::from_bits(bits);
        for raw in 0..SdlScanCode::Count as i32 {
            let Some(scan_code) = SdlScanCode::from_i32(raw) else {
                continue;
            };
            let press = key_event(scan_code, ButtonState::Pressed, modifiers);
            router.route(&press, modifiers, &assignments, &catalog, |invocation| {
                reached.insert(invocation.assignment().key().as_str().to_owned());
            });
            let release = key_event(scan_code, ButtonState::Released, KeyModifiers::NONE);
            router.route(
                &release,
                KeyModifiers::NONE,
                &assignments,
                &catalog,
                |_invocation| {},
            );
        }
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::AuxiliaryOne,
            MouseButton::AuxiliaryTwo,
        ] {
            for state in [ButtonState::Pressed, ButtonState::Released] {
                let event = PlatformEvent::MouseButton(MouseButtonEvent {
                    window_id: WINDOW,
                    button,
                    state,
                    click_count: 1,
                    x: 0.0,
                    y: 0.0,
                });
                router.route(&event, modifiers, &assignments, &catalog, |invocation| {
                    reached.insert(invocation.assignment().key().as_str().to_owned());
                });
            }
        }
        for y in [-1.0, 1.0] {
            let event = PlatformEvent::MouseWheel(MouseWheelEvent {
                window_id: WINDOW,
                x: 0.0,
                y,
                direction: MouseWheelDirection::Normal,
            });
            router.route(&event, modifiers, &assignments, &catalog, |invocation| {
                reached.insert(invocation.assignment().key().as_str().to_owned());
            });
        }
    }

    let routable = assignments
        .bindings()
        .iter()
        .filter(|assignment| match assignment.action() {
            UiBindingAction::Command(name) => catalog
                .binding(name)
                .is_some_and(|binding| binding.is_available_on(UiBindingPlatform::Windows)),
            _ => true,
        })
        .collect::<Vec<_>>();
    let unreachable = routable
        .iter()
        .filter(|assignment| !reached.contains(assignment.key().as_str()))
        .map(|assignment| assignment.key().as_str())
        .collect::<Vec<_>>();
    if !unreachable.is_empty() {
        return Err(IoError::new(
            ErrorKind::InvalidData,
            format!(
                "unreachable stock default chords: {}",
                unreachable.join(", ")
            ),
        )
        .into());
    }

    println!(
        "validated {} Windows-routable default chords through physical input routing ({} inactive platform/undeclared defaults)",
        routable.len(),
        assignments.bindings().len() - routable.len()
    );
    Ok(())
}

fn key_event(scan_code: SdlScanCode, state: ButtonState, modifiers: KeyModifiers) -> PlatformEvent {
    PlatformEvent::Key(KeyStateEvent {
        window_id: WINDOW,
        state,
        key_code: None,
        scan_code: Some(ScanCode::new(scan_code as i32)),
        modifiers,
        is_repeat: false,
    })
}

fn usage_error() -> IoError {
    IoError::new(
        ErrorKind::InvalidInput,
        "usage: validate_input_bindings <Data directory> <locale>",
    )
}

//! Lossless translation for the stock-relevant SDL event surface.

use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};
use sdl3::mouse::{MouseButton as SdlMouseButton, MouseWheelDirection as SdlWheelDirection};

use crate::platform::{
    ButtonState, KeyCode, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseMotionEvent, MouseWheelDirection, MouseWheelEvent, PlatformEvent, ScanCode,
    TextEditingEvent, TextInputEvent, TimedPlatformEvent, WindowEvent, WindowId,
};

/// Converts one SDL event without exposing SDL vocabulary to runtime consumers.
pub(super) fn translate(event: SdlEvent) -> Option<TimedPlatformEvent> {
    // SDL3 timestamps use SDL_GetTicksNS. Convert before truncating so the
    // stock 32-bit millisecond wrap occurs after 49.7 days, not 4.3 seconds.
    let timestamp_ms = (event.get_timestamp() / 1_000_000) as u32;
    translate_payload(event).map(|event| TimedPlatformEvent::new(event, timestamp_ms))
}

fn translate_payload(event: SdlEvent) -> Option<PlatformEvent> {
    match event {
        SdlEvent::Quit { .. } => Some(PlatformEvent::QuitRequested),
        SdlEvent::AppTerminating { .. } => Some(PlatformEvent::ApplicationTerminating),
        SdlEvent::AppLowMemory { .. } => Some(PlatformEvent::ApplicationLowMemory),
        SdlEvent::AppWillEnterBackground { .. } => {
            Some(PlatformEvent::ApplicationWillEnterBackground)
        }
        SdlEvent::AppDidEnterBackground { .. } => {
            Some(PlatformEvent::ApplicationDidEnterBackground)
        }
        SdlEvent::AppWillEnterForeground { .. } => {
            Some(PlatformEvent::ApplicationWillEnterForeground)
        }
        SdlEvent::AppDidEnterForeground { .. } => {
            Some(PlatformEvent::ApplicationDidEnterForeground)
        }
        SdlEvent::Window {
            window_id,
            win_event,
            ..
        } => translate_window(win_event).map(|event| PlatformEvent::Window {
            window_id: WindowId::from_sdl(window_id),
            event,
        }),
        SdlEvent::KeyDown {
            window_id,
            keycode,
            scancode,
            keymod,
            repeat,
            ..
        } => Some(PlatformEvent::Key(KeyStateEvent {
            window_id: WindowId::from_sdl(window_id),
            state: ButtonState::Pressed,
            key_code: keycode.map(|key| KeyCode::from_sdl(key as u32)),
            scan_code: scancode.map(|key| ScanCode::from_sdl(key as i32)),
            modifiers: KeyModifiers::from_sdl(keymod.bits()),
            is_repeat: repeat,
        })),
        SdlEvent::KeyUp {
            window_id,
            keycode,
            scancode,
            keymod,
            ..
        } => Some(PlatformEvent::Key(KeyStateEvent {
            window_id: WindowId::from_sdl(window_id),
            state: ButtonState::Released,
            key_code: keycode.map(|key| KeyCode::from_sdl(key as u32)),
            scan_code: scancode.map(|key| ScanCode::from_sdl(key as i32)),
            modifiers: KeyModifiers::from_sdl(keymod.bits()),
            is_repeat: false,
        })),
        SdlEvent::TextInput {
            window_id, text, ..
        } => Some(PlatformEvent::TextInput(TextInputEvent {
            window_id: WindowId::from_sdl(window_id),
            text,
        })),
        SdlEvent::TextEditing {
            window_id,
            text,
            start,
            length,
            ..
        } => Some(PlatformEvent::TextEditing(TextEditingEvent {
            window_id: WindowId::from_sdl(window_id),
            text,
            start,
            length,
        })),
        SdlEvent::MouseMotion {
            window_id,
            x,
            y,
            xrel,
            yrel,
            ..
        } => Some(PlatformEvent::MouseMotion(MouseMotionEvent {
            window_id: WindowId::from_sdl(window_id),
            x,
            y,
            delta_x: xrel,
            delta_y: yrel,
        })),
        SdlEvent::MouseButtonDown {
            window_id,
            mouse_btn,
            clicks,
            x,
            y,
            ..
        } => Some(PlatformEvent::MouseButton(MouseButtonEvent {
            window_id: WindowId::from_sdl(window_id),
            button: translate_mouse_button(mouse_btn),
            state: ButtonState::Pressed,
            click_count: clicks,
            x,
            y,
        })),
        SdlEvent::MouseButtonUp {
            window_id,
            mouse_btn,
            clicks,
            x,
            y,
            ..
        } => Some(PlatformEvent::MouseButton(MouseButtonEvent {
            window_id: WindowId::from_sdl(window_id),
            button: translate_mouse_button(mouse_btn),
            state: ButtonState::Released,
            click_count: clicks,
            x,
            y,
        })),
        SdlEvent::MouseWheel {
            window_id,
            x,
            y,
            direction,
            ..
        } => Some(PlatformEvent::MouseWheel(MouseWheelEvent {
            window_id: WindowId::from_sdl(window_id),
            x,
            y,
            direction: translate_wheel_direction(direction),
        })),
        SdlEvent::ClipboardUpdate { .. } => Some(PlatformEvent::ClipboardChanged),
        // Controller, touch, pen, camera, renderer, and custom SDL events are
        // not part of the recovered stock desktop input boundary.
        _ => None,
    }
}

/// Preserves window state that affects swapchains, focus, or input routing.
fn translate_window(event: SdlWindowEvent) -> Option<WindowEvent> {
    match event {
        SdlWindowEvent::None => None,
        SdlWindowEvent::Shown => Some(WindowEvent::Shown),
        SdlWindowEvent::Hidden => Some(WindowEvent::Hidden),
        SdlWindowEvent::Exposed => Some(WindowEvent::Exposed),
        SdlWindowEvent::Moved(x, y) => Some(WindowEvent::Moved { x, y }),
        SdlWindowEvent::Resized(width, height) => Some(WindowEvent::Resized { width, height }),
        SdlWindowEvent::PixelSizeChanged(width, height) => {
            Some(WindowEvent::PixelSizeChanged { width, height })
        }
        SdlWindowEvent::Minimized => Some(WindowEvent::Minimized),
        SdlWindowEvent::Maximized => Some(WindowEvent::Maximized),
        SdlWindowEvent::Occluded => Some(WindowEvent::Occluded),
        SdlWindowEvent::Restored => Some(WindowEvent::Restored),
        SdlWindowEvent::MouseEnter => Some(WindowEvent::MouseEntered),
        SdlWindowEvent::MouseLeave => Some(WindowEvent::MouseLeft),
        SdlWindowEvent::FocusGained => Some(WindowEvent::FocusGained),
        SdlWindowEvent::FocusLost => Some(WindowEvent::FocusLost),
        SdlWindowEvent::CloseRequested => Some(WindowEvent::CloseRequested),
        // Hit testing is an SDL callback concern and ICC profile data is not
        // consumed by the stock renderer contract currently represented here.
        SdlWindowEvent::HitTest(_, _) | SdlWindowEvent::ICCProfChanged => None,
        SdlWindowEvent::DisplayChanged(display) => Some(WindowEvent::DisplayChanged { display }),
    }
}

/// Maps SDL's five standard desktop pointer buttons.
const fn translate_mouse_button(button: SdlMouseButton) -> MouseButton {
    match button {
        SdlMouseButton::Left => MouseButton::Left,
        SdlMouseButton::Middle => MouseButton::Middle,
        SdlMouseButton::Right => MouseButton::Right,
        SdlMouseButton::X1 => MouseButton::AuxiliaryOne,
        SdlMouseButton::X2 => MouseButton::AuxiliaryTwo,
        SdlMouseButton::Unknown => MouseButton::Unknown,
    }
}

/// Converts the scroll convention while retaining unknown future values.
const fn translate_wheel_direction(direction: SdlWheelDirection) -> MouseWheelDirection {
    match direction {
        SdlWheelDirection::Normal => MouseWheelDirection::Normal,
        SdlWheelDirection::Flipped => MouseWheelDirection::Flipped,
        SdlWheelDirection::Unknown(_) => MouseWheelDirection::Unknown,
    }
}

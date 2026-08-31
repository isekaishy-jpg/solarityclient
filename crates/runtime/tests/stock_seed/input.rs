//! External stock-compatibility tests for retained physical input state.

use solarity_runtime::{
    ButtonState, InputControl, KeyModifiers, KeyStateEvent, MouseButton, MouseButtonEvent,
    MouseMotionEvent, MouseWheelDirection, MouseWheelEvent, PlatformEvent, ScanCode, WindowEvent,
    WindowId,
};

const PRIMARY_WINDOW: WindowId = WindowId::new(7);
const OTHER_WINDOW: WindowId = WindowId::new(8);
const KEY_W: ScanCode = ScanCode::new(26);

/// Physical state is committed once while repeated downs remain event-only edges.
#[test]
fn physical_key_state_ignores_repeat_and_foreign_windows() {
    let mut input = InputControl::new(PRIMARY_WINDOW);
    let press = key_event(PRIMARY_WINDOW, ButtonState::Pressed, false, 1);

    assert!(input.admit(&press));
    assert!(input.is_key_down(KEY_W));
    assert_eq!(input.held_key_count(), 1);
    assert_eq!(input.modifiers().bits(), 1);
    assert_eq!(input.revision(), 1);

    assert!(!input.admit(&key_event(PRIMARY_WINDOW, ButtonState::Pressed, true, 1,)));
    assert!(!input.admit(&key_event(OTHER_WINDOW, ButtonState::Released, false, 0,)));
    assert_eq!(input.revision(), 1);

    assert!(input.admit(&key_event(PRIMARY_WINDOW, ButtonState::Released, false, 0,)));
    assert!(!input.is_key_down(KEY_W));
    assert_eq!(input.held_key_count(), 0);
    assert_eq!(input.modifiers(), KeyModifiers::NONE);
    assert_eq!(input.revision(), 2);
}

/// Focus loss releases digital controls without fabricating platform-up events.
#[test]
fn focus_loss_clears_held_keys_buttons_and_modifiers() {
    let mut input = InputControl::new(PRIMARY_WINDOW);
    assert!(input.admit(&PlatformEvent::Window {
        window_id: PRIMARY_WINDOW,
        event: WindowEvent::FocusGained,
    }));
    assert!(input.admit(&key_event(PRIMARY_WINDOW, ButtonState::Pressed, false, 2,)));
    assert!(input.admit(&PlatformEvent::MouseButton(MouseButtonEvent {
        window_id: PRIMARY_WINDOW,
        button: MouseButton::Right,
        state: ButtonState::Pressed,
        click_count: 1,
        x: 400.0,
        y: 300.0,
    })));
    assert!(input.is_focused());
    assert!(input.is_key_down(KEY_W));
    assert!(input.is_mouse_button_down(MouseButton::Right));

    assert!(input.admit(&PlatformEvent::Window {
        window_id: PRIMARY_WINDOW,
        event: WindowEvent::FocusLost,
    }));
    assert!(!input.is_focused());
    assert!(!input.is_key_down(KEY_W));
    assert!(!input.is_mouse_button_down(MouseButton::Right));
    assert_eq!(input.modifiers(), KeyModifiers::NONE);
    assert_eq!(
        input.pointer_position().map(|position| position.x()),
        Some(400.0)
    );
}

/// Relative motion accumulates once and SDL's flipped wheel convention normalizes.
#[test]
fn pointer_frame_motion_is_accumulated_and_taken() {
    let mut input = InputControl::new(PRIMARY_WINDOW);
    assert!(input.admit(&PlatformEvent::MouseMotion(MouseMotionEvent {
        window_id: PRIMARY_WINDOW,
        x: 40.0,
        y: 20.0,
        delta_x: 3.0,
        delta_y: -2.0,
    })));
    assert!(input.admit(&PlatformEvent::MouseMotion(MouseMotionEvent {
        window_id: PRIMARY_WINDOW,
        x: 41.0,
        y: 25.0,
        delta_x: 1.0,
        delta_y: 5.0,
    })));
    assert!(input.admit(&PlatformEvent::MouseWheel(MouseWheelEvent {
        window_id: PRIMARY_WINDOW,
        x: 0.5,
        y: -2.0,
        direction: MouseWheelDirection::Flipped,
    })));

    let motion = input.take_frame_motion();
    assert_eq!(
        motion.pointer_position().map(|position| position.x()),
        Some(41.0)
    );
    assert_eq!(
        motion.pointer_position().map(|position| position.y()),
        Some(25.0)
    );
    assert_eq!(motion.delta_x(), 4.0);
    assert_eq!(motion.delta_y(), 3.0);
    assert_eq!(motion.wheel_x(), -0.5);
    assert_eq!(motion.wheel_y(), 2.0);

    let next = input.take_frame_motion();
    assert_eq!(next.pointer_position(), motion.pointer_position());
    assert_eq!(next.delta_x(), 0.0);
    assert_eq!(next.delta_y(), 0.0);
    assert_eq!(next.wheel_x(), 0.0);
    assert_eq!(next.wheel_y(), 0.0);
}

/// Later SDL wheel directions are not guessed into stock's known convention.
#[test]
fn unknown_wheel_direction_has_no_normal_direction_fallback() {
    let mut input = InputControl::new(PRIMARY_WINDOW);
    assert!(!input.admit(&PlatformEvent::MouseWheel(MouseWheelEvent {
        window_id: PRIMARY_WINDOW,
        x: 0.0,
        y: 1.0,
        direction: MouseWheelDirection::Unknown,
    })));
    assert_eq!(input.take_frame_motion().wheel_y(), 0.0);
}

fn key_event(
    window_id: WindowId,
    state: ButtonState,
    is_repeat: bool,
    modifiers: u16,
) -> PlatformEvent {
    PlatformEvent::Key(KeyStateEvent {
        window_id,
        state,
        key_code: None,
        scan_code: Some(KEY_W),
        modifiers: KeyModifiers::from_bits(modifiers),
        is_repeat,
    })
}

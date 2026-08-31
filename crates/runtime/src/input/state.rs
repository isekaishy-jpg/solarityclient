//! Allocation-conscious retained state behind [`super::InputControl`].

use std::collections::HashSet;

use crate::input::{InputFrameMotion, PointerPosition};
use crate::platform::{KeyModifiers, MouseButton, ScanCode};

/// Expected simultaneous-key count used only to avoid early reallocation.
const INITIAL_HELD_KEY_CAPACITY: usize = 16;

/// Mutable input image; all transitions are committed by `InputControl`.
pub(super) struct InputState {
    held_keys: HashSet<ScanCode>,
    mouse_buttons: u8,
    modifiers: KeyModifiers,
    pointer_position: Option<PointerPosition>,
    delta_x: f32,
    delta_y: f32,
    wheel_x: f32,
    wheel_y: f32,
    focused: bool,
    revision: u64,
}

impl InputState {
    /// Allocates the small physical-key set once for the process lifetime.
    pub(super) fn new() -> Self {
        Self {
            held_keys: HashSet::with_capacity(INITIAL_HELD_KEY_CAPACITY),
            mouse_buttons: 0,
            modifiers: KeyModifiers::NONE,
            pointer_position: None,
            delta_x: 0.0,
            delta_y: 0.0,
            wheel_x: 0.0,
            wheel_y: 0.0,
            focused: false,
            revision: 0,
        }
    }

    pub(super) fn set_focused(&mut self, focused: bool) -> bool {
        if self.focused == focused {
            return false;
        }
        self.focused = focused;
        true
    }

    pub(super) const fn is_focused(&self) -> bool {
        self.focused
    }

    pub(super) fn set_modifiers(&mut self, modifiers: KeyModifiers) -> bool {
        if self.modifiers == modifiers {
            return false;
        }
        self.modifiers = modifiers;
        true
    }

    pub(super) const fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }

    pub(super) fn set_key(&mut self, scan_code: ScanCode, down: bool) -> bool {
        if down {
            self.held_keys.insert(scan_code)
        } else {
            self.held_keys.remove(&scan_code)
        }
    }

    pub(super) fn is_key_down(&self, scan_code: ScanCode) -> bool {
        self.held_keys.contains(&scan_code)
    }

    pub(super) fn held_key_count(&self) -> usize {
        self.held_keys.len()
    }

    pub(super) fn set_mouse_button(&mut self, button: MouseButton, down: bool) -> bool {
        let Some(mask) = mouse_button_mask(button) else {
            return false;
        };
        let previous = self.mouse_buttons;
        if down {
            self.mouse_buttons |= mask;
        } else {
            self.mouse_buttons &= !mask;
        }
        previous != self.mouse_buttons
    }

    pub(super) const fn is_mouse_button_down(&self, button: MouseButton) -> bool {
        let Some(mask) = mouse_button_mask(button) else {
            return false;
        };
        self.mouse_buttons & mask != 0
    }

    pub(super) fn set_pointer_position(&mut self, x: f32, y: f32) -> bool {
        let position = PointerPosition::new(x, y);
        if self.pointer_position == Some(position) {
            return false;
        }
        self.pointer_position = Some(position);
        true
    }

    pub(super) fn move_pointer(&mut self, x: f32, y: f32, delta_x: f32, delta_y: f32) -> bool {
        let position_changed = self.set_pointer_position(x, y);
        self.delta_x += delta_x;
        self.delta_y += delta_y;
        position_changed || delta_x != 0.0 || delta_y != 0.0
    }

    pub(super) fn scroll(&mut self, x: f32, y: f32) -> bool {
        self.wheel_x += x;
        self.wheel_y += y;
        x != 0.0 || y != 0.0
    }

    pub(super) const fn pointer_position(&self) -> Option<PointerPosition> {
        self.pointer_position
    }

    pub(super) fn take_frame_motion(&mut self) -> InputFrameMotion {
        let motion = InputFrameMotion::new(
            self.pointer_position,
            self.delta_x,
            self.delta_y,
            self.wheel_x,
            self.wheel_y,
        );
        self.delta_x = 0.0;
        self.delta_y = 0.0;
        self.wheel_x = 0.0;
        self.wheel_y = 0.0;
        motion
    }

    pub(super) fn clear_held(&mut self) -> bool {
        let changed = !self.held_keys.is_empty()
            || self.mouse_buttons != 0
            || self.modifiers != KeyModifiers::NONE;
        self.held_keys.clear();
        self.mouse_buttons = 0;
        self.modifiers = KeyModifiers::NONE;
        changed
    }

    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn commit_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

/// Matches FrameXML's BUTTON1..BUTTON5 physical identity.
const fn mouse_button_mask(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(1 << 0),
        MouseButton::Right => Some(1 << 1),
        MouseButton::Middle => Some(1 << 2),
        MouseButton::AuxiliaryOne => Some(1 << 3),
        MouseButton::AuxiliaryTwo => Some(1 << 4),
        MouseButton::Unknown => None,
    }
}

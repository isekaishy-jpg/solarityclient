//! Main-thread admission of platform events into retained input state.

use crate::input::state::InputState;
use crate::input::{InputFrameMotion, PointerPosition};
use crate::platform::{
    ButtonState, KeyModifiers, MouseButton, MouseWheelDirection, PlatformEvent, ScanCode,
    WindowEvent, WindowId,
};

/// Platform-neutral keyboard and mouse state owned by the client main thread.
///
/// Build 12340's `InputControl.cpp` owns physical input before FrameXML binding
/// or player-control commands interpret it. This boundary follows that order:
/// SDL publishes typed events, this owner commits raw state, and later input
/// consumers may resolve bindings without polling SDL or retaining its types.
pub struct InputControl {
    primary_window: WindowId,
    state: InputState,
}

impl InputControl {
    /// Creates an empty state image for the process's primary client window.
    #[must_use]
    pub fn new(primary_window: WindowId) -> Self {
        Self {
            primary_window,
            state: InputState::new(),
        }
    }

    /// Commits one translated platform event in source order.
    ///
    /// Events owned by another window do not enter this state image. Repeated
    /// key-down events retain the existing held bit, while their event remains
    /// available to later binding/UI dispatch. Losing application or window
    /// focus clears every held control so a missing release cannot stick.
    ///
    /// Returns whether the retained state or current-frame motion changed.
    pub fn admit(&mut self, event: &PlatformEvent) -> bool {
        let changed = match event {
            PlatformEvent::ApplicationDidEnterBackground => {
                self.state.set_focused(false) | self.state.clear_held()
            }
            PlatformEvent::Window { window_id, event } if *window_id == self.primary_window => {
                match event {
                    WindowEvent::FocusGained => self.state.set_focused(true),
                    WindowEvent::FocusLost => {
                        self.state.set_focused(false) | self.state.clear_held()
                    }
                    _ => false,
                }
            }
            PlatformEvent::Key(event) if event.window_id == self.primary_window => {
                let modifiers_changed = self.state.set_modifiers(event.modifiers);
                let key_changed = event.scan_code.is_some_and(|scan_code| {
                    self.state
                        .set_key(scan_code, event.state == ButtonState::Pressed)
                });
                modifiers_changed | key_changed
            }
            PlatformEvent::MouseMotion(event) if event.window_id == self.primary_window => self
                .state
                .move_pointer(event.x, event.y, event.delta_x, event.delta_y),
            PlatformEvent::MouseButton(event) if event.window_id == self.primary_window => {
                let pointer_changed = self.state.set_pointer_position(event.x, event.y);
                let button_changed = self
                    .state
                    .set_mouse_button(event.button, event.state == ButtonState::Pressed);
                pointer_changed | button_changed
            }
            PlatformEvent::MouseWheel(event) if event.window_id == self.primary_window => {
                let direction = match event.direction {
                    MouseWheelDirection::Normal => 1.0,
                    MouseWheelDirection::Flipped => -1.0,
                    MouseWheelDirection::Unknown => return false,
                };
                self.state.scroll(event.x * direction, event.y * direction)
            }
            _ => false,
        };
        if changed {
            self.state.commit_revision();
        }
        changed
    }

    /// Returns the window whose events are admitted by this owner.
    #[must_use]
    pub const fn primary_window(&self) -> WindowId {
        self.primary_window
    }

    /// Returns whether the primary window currently owns keyboard focus.
    #[must_use]
    pub const fn is_focused(&self) -> bool {
        self.state.is_focused()
    }

    /// Returns the modifier/lock mask from the latest admitted key event.
    #[must_use]
    pub const fn modifiers(&self) -> KeyModifiers {
        self.state.modifiers()
    }

    /// Returns whether a physical keyboard location is currently held.
    #[must_use]
    pub fn is_key_down(&self, scan_code: ScanCode) -> bool {
        self.state.is_key_down(scan_code)
    }

    /// Returns the number of simultaneously held physical keys.
    #[must_use]
    pub fn held_key_count(&self) -> usize {
        self.state.held_key_count()
    }

    /// Returns whether one of stock's five admitted pointer buttons is held.
    #[must_use]
    pub const fn is_mouse_button_down(&self, button: MouseButton) -> bool {
        self.state.is_mouse_button_down(button)
    }

    /// Returns the latest logical pointer position, when SDL has supplied one.
    #[must_use]
    pub const fn pointer_position(&self) -> Option<PointerPosition> {
        self.state.pointer_position()
    }

    /// Takes accumulated pointer/wheel motion for one consumer frame.
    ///
    /// Held keys, buttons, focus, modifiers, and absolute pointer position are
    /// retained. Only relative and wheel deltas are reset.
    pub fn take_frame_motion(&mut self) -> InputFrameMotion {
        self.state.take_frame_motion()
    }

    /// Returns the wrapping state revision used by change-driven consumers.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.state.revision()
    }

    /// Releases all held controls without changing focus or pointer position.
    ///
    /// Returns whether any retained digital state changed.
    pub fn clear_held(&mut self) -> bool {
        let changed = self.state.clear_held();
        if changed {
            self.state.commit_revision();
        }
        changed
    }
}

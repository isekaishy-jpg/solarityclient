//! Shared modifier-key image projected from the runtime input owner.

use std::cell::Cell;
use std::rc::Rc;

/// One atomic image of the six physical modifier keys exposed by FrameXML.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiModifierKeys {
    left_shift: bool,
    right_shift: bool,
    left_control: bool,
    right_control: bool,
    left_alt: bool,
    right_alt: bool,
}

impl UiModifierKeys {
    /// Creates a complete side-specific modifier image.
    #[must_use]
    pub const fn new(
        left_shift: bool,
        right_shift: bool,
        left_control: bool,
        right_control: bool,
        left_alt: bool,
        right_alt: bool,
    ) -> Self {
        Self {
            left_shift,
            right_shift,
            left_control,
            right_control,
            left_alt,
            right_alt,
        }
    }

    /// Reports whether the physical left Shift key is held.
    #[must_use]
    pub const fn left_shift(self) -> bool {
        self.left_shift
    }

    /// Reports whether the physical right Shift key is held.
    #[must_use]
    pub const fn right_shift(self) -> bool {
        self.right_shift
    }

    /// Reports whether either physical Shift key is held.
    #[must_use]
    pub const fn shift(self) -> bool {
        self.left_shift || self.right_shift
    }

    /// Reports whether the physical left Control key is held.
    #[must_use]
    pub const fn left_control(self) -> bool {
        self.left_control
    }

    /// Reports whether the physical right Control key is held.
    #[must_use]
    pub const fn right_control(self) -> bool {
        self.right_control
    }

    /// Reports whether either physical Control key is held.
    #[must_use]
    pub const fn control(self) -> bool {
        self.left_control || self.right_control
    }

    /// Reports whether the physical left Alt key is held.
    #[must_use]
    pub const fn left_alt(self) -> bool {
        self.left_alt
    }

    /// Reports whether the physical right Alt key is held.
    #[must_use]
    pub const fn right_alt(self) -> bool {
        self.right_alt
    }

    /// Reports whether either physical Alt key is held.
    #[must_use]
    pub const fn alt(self) -> bool {
        self.left_alt || self.right_alt
    }
}

/// Cloneable main-thread modifier boundary shared with native Lua closures.
///
/// The initial all-released image is the real process input state before any
/// admitted key transition, not a missing-state substitute.
#[derive(Clone, Debug, Default)]
pub struct UiModifierKeyState {
    keys: Rc<Cell<UiModifierKeys>>,
}

impl UiModifierKeyState {
    /// Creates an all-released modifier image.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces all six modifier keys from one admitted runtime snapshot.
    pub fn set(&self, keys: UiModifierKeys) {
        self.keys.set(keys);
    }

    /// Returns the latest complete modifier image.
    #[must_use]
    pub fn keys(&self) -> UiModifierKeys {
        self.keys.get()
    }

    /// Releases all modifiers after focus loss or application backgrounding.
    pub fn clear(&self) {
        self.keys.set(UiModifierKeys::default());
    }
}

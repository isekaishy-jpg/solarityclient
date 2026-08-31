//! Public frame values produced by the retained input owner.

/// Latest logical pointer coordinates in the primary client window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPosition {
    x: f32,
    y: f32,
}

impl PointerPosition {
    pub(super) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns the horizontal logical-window coordinate.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the vertical logical-window coordinate.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }
}

/// Pointer and wheel movement accumulated since the previous frame take.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputFrameMotion {
    pointer_position: Option<PointerPosition>,
    delta_x: f32,
    delta_y: f32,
    wheel_x: f32,
    wheel_y: f32,
}

impl InputFrameMotion {
    pub(super) const fn new(
        pointer_position: Option<PointerPosition>,
        delta_x: f32,
        delta_y: f32,
        wheel_x: f32,
        wheel_y: f32,
    ) -> Self {
        Self {
            pointer_position,
            delta_x,
            delta_y,
            wheel_x,
            wheel_y,
        }
    }

    /// Returns the latest absolute position, if any motion/button event supplied it.
    #[must_use]
    pub const fn pointer_position(self) -> Option<PointerPosition> {
        self.pointer_position
    }

    /// Returns accumulated horizontal relative pointer movement.
    #[must_use]
    pub const fn delta_x(self) -> f32 {
        self.delta_x
    }

    /// Returns accumulated vertical relative pointer movement.
    #[must_use]
    pub const fn delta_y(self) -> f32 {
        self.delta_y
    }

    /// Returns normalized horizontal wheel movement.
    #[must_use]
    pub const fn wheel_x(self) -> f32 {
        self.wheel_x
    }

    /// Returns normalized vertical wheel movement.
    #[must_use]
    pub const fn wheel_y(self) -> f32 {
        self.wheel_y
    }
}

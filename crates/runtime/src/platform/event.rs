//! SDL-independent platform event vocabulary.

/// Stable identifier for an SDL-created client window.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WindowId(u32);

impl WindowId {
    /// Returns the platform identifier for diagnostic correlation.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Wraps an identifier emitted by the owned SDL event source.
    pub(super) const fn from_sdl(value: u32) -> Self {
        Self(value)
    }
}

/// Physical keyboard location independent of the active keyboard layout.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScanCode(i32);

impl ScanCode {
    /// Returns SDL's stable physical-key numeric value.
    #[must_use]
    pub const fn value(self) -> i32 {
        self.0
    }

    /// Preserves the SDL scancode without exposing an SDL enum downstream.
    pub(super) const fn from_sdl(value: i32) -> Self {
        Self(value)
    }
}

/// Layout-resolved key value suitable for UI binding and text controls.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeyCode(u32);

impl KeyCode {
    /// Returns SDL's Unicode-or-special-key numeric value.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Preserves the SDL keycode without exposing an SDL enum downstream.
    pub(super) const fn from_sdl(value: u32) -> Self {
        Self(value)
    }
}

/// Modifier mask captured atomically with a keyboard event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyModifiers(u16);

impl KeyModifiers {
    /// Returns the unmodified SDL-compatible mask for binding serialization.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Preserves every modifier bit, including mode and lock state.
    pub(super) const fn from_sdl(bits: u16) -> Self {
        Self(bits)
    }
}

/// Press or release state shared by keyboard and pointer buttons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ButtonState {
    /// The control transitioned into its active state.
    Pressed,
    /// The control transitioned into its inactive state.
    Released,
}

/// Complete physical and logical identity for a keyboard transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyStateEvent {
    /// Window that held keyboard focus when SDL emitted the event.
    pub window_id: WindowId,
    /// Whether the key was pressed or released.
    pub state: ButtonState,
    /// Layout-resolved key when SDL could identify one.
    pub key_code: Option<KeyCode>,
    /// Physical key location when SDL could identify one.
    pub scan_code: Option<ScanCode>,
    /// Modifier and lock state captured with the transition.
    pub modifiers: KeyModifiers,
    /// Whether this press came from keyboard repeat rather than a new transition.
    pub is_repeat: bool,
}

/// Text committed by the active keyboard layout or input method.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputEvent {
    /// Window receiving the committed text.
    pub window_id: WindowId,
    /// UTF-8 text after SDL input-method processing.
    pub text: String,
}

/// In-progress input-method composition used by editable UI controls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextEditingEvent {
    /// Window receiving the composition update.
    pub window_id: WindowId,
    /// Current UTF-8 composition text.
    pub text: String,
    /// SDL composition selection start.
    pub start: i32,
    /// SDL composition selection length.
    pub length: i32,
}

/// Pointer button identity supported by the stock-style mouse boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseButton {
    /// Primary pointer button.
    Left,
    /// Wheel or middle pointer button.
    Middle,
    /// Secondary pointer button.
    Right,
    /// First auxiliary pointer button.
    AuxiliaryOne,
    /// Second auxiliary pointer button.
    AuxiliaryTwo,
    /// A platform button not understood by the current SDL binding.
    Unknown,
}

/// Absolute and relative mouse movement in logical window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseMotionEvent {
    /// Window receiving the movement.
    pub window_id: WindowId,
    /// Current horizontal logical coordinate.
    pub x: f32,
    /// Current vertical logical coordinate.
    pub y: f32,
    /// Horizontal movement since the prior event.
    pub delta_x: f32,
    /// Vertical movement since the prior event.
    pub delta_y: f32,
}

/// Mouse button transition and its cursor position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseButtonEvent {
    /// Window receiving the transition.
    pub window_id: WindowId,
    /// Which pointer control changed state.
    pub button: MouseButton,
    /// Whether the pointer control was pressed or released.
    pub state: ButtonState,
    /// SDL click count, including double-click aggregation.
    pub click_count: u8,
    /// Horizontal logical coordinate at the transition.
    pub x: f32,
    /// Vertical logical coordinate at the transition.
    pub y: f32,
}

/// Meaning of positive wheel values on the current platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MouseWheelDirection {
    /// Positive values use SDL's conventional direction.
    Normal,
    /// Positive values are reversed for natural scrolling.
    Flipped,
    /// SDL reported a direction introduced after this boundary was written.
    Unknown,
}

/// High-resolution pointer wheel motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseWheelEvent {
    /// Window receiving the wheel motion.
    pub window_id: WindowId,
    /// Horizontal high-resolution scroll amount.
    pub x: f32,
    /// Vertical high-resolution scroll amount.
    pub y: f32,
    /// Direction convention attached to the values.
    pub direction: MouseWheelDirection,
}

/// Window lifecycle state needed by rendering, UI focus, and pause policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowEvent {
    /// The primary window became visible.
    Shown,
    /// The primary window became hidden.
    Hidden,
    /// The window needs its contents redrawn.
    Exposed,
    /// The desktop window moved to the supplied logical coordinates.
    Moved {
        /// Horizontal desktop coordinate.
        x: i32,
        /// Vertical desktop coordinate.
        y: i32,
    },
    /// The logical window dimensions changed.
    Resized {
        /// New logical width.
        width: i32,
        /// New logical height.
        height: i32,
    },
    /// The drawable pixel extent changed and the swapchain must be reconsidered.
    PixelSizeChanged {
        /// New physical pixel width.
        width: i32,
        /// New physical pixel height.
        height: i32,
    },
    /// The window was minimized.
    Minimized,
    /// The window was maximized.
    Maximized,
    /// Another desktop surface fully occluded the window.
    Occluded,
    /// The window returned from minimized or maximized state.
    Restored,
    /// The pointer entered the window.
    MouseEntered,
    /// The pointer left the window.
    MouseLeft,
    /// The window gained keyboard focus.
    FocusGained,
    /// The window lost keyboard focus.
    FocusLost,
    /// The operating system requested that this window close.
    CloseRequested,
    /// The desktop display hosting the window changed.
    DisplayChanged {
        /// SDL display identifier now hosting the window.
        display: i32,
    },
}

/// Events admitted from SDL into the client runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformEvent {
    /// The process received a global termination request.
    QuitRequested,
    /// SDL reported a lifecycle transition for the primary application.
    ApplicationTerminating,
    /// The operating system warned the process about memory pressure.
    ApplicationLowMemory,
    /// The application is about to lose foreground execution.
    ApplicationWillEnterBackground,
    /// The application has lost foreground execution.
    ApplicationDidEnterBackground,
    /// The application is about to regain foreground execution.
    ApplicationWillEnterForeground,
    /// The application has regained foreground execution.
    ApplicationDidEnterForeground,
    /// State change for a client window.
    Window {
        /// Window owning the state transition.
        window_id: WindowId,
        /// SDL-independent state transition.
        event: WindowEvent,
    },
    /// Physical or logical keyboard transition.
    Key(KeyStateEvent),
    /// Text committed by an input method.
    TextInput(TextInputEvent),
    /// In-progress input-method composition.
    TextEditing(TextEditingEvent),
    /// Pointer movement.
    MouseMotion(MouseMotionEvent),
    /// Pointer button transition.
    MouseButton(MouseButtonEvent),
    /// Pointer wheel movement.
    MouseWheel(MouseWheelEvent),
    /// Clipboard contents changed outside the client.
    ClipboardChanged,
}

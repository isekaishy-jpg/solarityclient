//! Typed initial window policy.

/// Presentation mode requested before Vulkan surface creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowMode {
    /// A decorated desktop window that the user can resize.
    Windowed,
    /// A borderless desktop-composited window covering the primary display.
    FullscreenWindowed,
}

impl WindowMode {
    /// Accepts only the two modes represented by the runtime contract.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "windowed" => Some(Self::Windowed),
            "fullscreen-windowed" => Some(Self::FullscreenWindowed),
            _ => None,
        }
    }
}

/// Validated logical dimensions and presentation mode for the primary window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowConfiguration {
    width: u32,
    height: u32,
    mode: WindowMode,
}

impl WindowConfiguration {
    /// Stores dimensions already validated by the command-line boundary.
    pub(crate) const fn new(width: u32, height: u32, mode: WindowMode) -> Self {
        Self {
            width,
            height,
            mode,
        }
    }

    /// Returns the initial logical width in SDL window coordinates.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the initial logical height in SDL window coordinates.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the requested decorated or borderless-desktop presentation mode.
    #[must_use]
    pub const fn mode(self) -> WindowMode {
        self.mode
    }
}

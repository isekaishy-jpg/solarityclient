//! Persistent renderer-independent camera target, mode, zoom, and orientation state.

use shipyard::Component;

/// Durable ordinary-player orbit state seeded from build-12340 view 2.
#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct PlayerViewState {
    distance: f32,
    pitch_radians: f32,
    yaw_offset_radians: f32,
    view: u8,
}

impl PlayerViewState {
    /// Build-12340 `CGCamera::s_cameraViewDataDefault[2]`.
    pub const STOCK_VIEW_2: Self = Self {
        distance: 5.55,
        pitch_radians: 0.174_532_92,
        yaw_offset_radians: 0.0,
        view: 2,
    };

    /// Creates a view already admitted by camera CVar and binding policy.
    #[must_use]
    pub const fn new(distance: f32, pitch_radians: f32, yaw_offset_radians: f32, view: u8) -> Self {
        Self {
            distance,
            pitch_radians,
            yaw_offset_radians,
            view,
        }
    }

    /// Returns the requested pivot-to-eye distance.
    #[must_use]
    pub const fn distance(self) -> f32 {
        self.distance
    }

    /// Returns the requested orbit pitch in radians.
    #[must_use]
    pub const fn pitch_radians(self) -> f32 {
        self.pitch_radians
    }

    /// Returns the view yaw relative to authoritative player facing.
    #[must_use]
    pub const fn yaw_offset_radians(self) -> f32 {
        self.yaw_offset_radians
    }

    /// Returns the active saved-view slot.
    #[must_use]
    pub const fn view(self) -> u8 {
        self.view
    }
}

impl Default for PlayerViewState {
    fn default() -> Self {
        Self::STOCK_VIEW_2
    }
}

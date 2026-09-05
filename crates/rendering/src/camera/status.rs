//! Stable camera and frustum validation failures.

use thiserror::Error;

/// Final camera input cannot form stock renderer state.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldCameraError {
    /// Eye, target, or up contains NaN or infinity.
    #[error("world camera basis contains a non-finite component")]
    NonFiniteBasis,
    /// Eye and target do not define a usable direction.
    #[error("world camera eye and target do not define a view direction")]
    ViewDirection,
    /// The supplied roll reference is parallel to the view direction.
    #[error("world camera up direction is parallel to the view direction")]
    UpDirection,
    /// Vertical FOV is not finite and strictly between zero and pi.
    #[error("world camera field of view is outside the projection range")]
    FieldOfView,
    /// Parallel projection bounds are non-finite, empty, or reversed.
    #[error("world camera orthographic bounds are invalid")]
    OrthographicBounds,
    /// Near and far planes are not finite and ordered, or a perspective near
    /// plane is not positive.
    #[error("world camera clipping planes are invalid")]
    ClipRange,
    /// A followed subject origin or collision pivot contains NaN or infinity.
    #[error("world camera subject contains a non-finite component")]
    NonFiniteSubject,
    /// Viewport width divided by height is not positive and finite.
    #[error("world camera aspect ratio is invalid")]
    AspectRatio,
    /// A partial normalized screen rectangle is non-finite or empty.
    #[error("world camera screen window is invalid")]
    ScreenWindow,
    /// A visibility primitive contains a non-finite component or radius.
    #[error("world visibility bounds contain a non-finite component")]
    NonFiniteBounds,
}

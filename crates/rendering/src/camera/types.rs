//! Renderer-owned world camera and immutable per-frame matrices.

use glam::{Mat4, Vec3};

/// Build-12340's 90-degree camera FOV after its stock 0.6 projection factor.
pub const WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS: f32 = 0.942_477_8;

/// Fixed in-world near plane installed by stock map loading.
pub const WORLD_NEAR_CLIP: f32 = 0.2;

/// Start of the ordinary world viewport depth interval.
pub const WORLD_DEPTH_MINIMUM: f32 = 0.0;

/// End of the world interval before stock's horizon and sky depth ranges.
pub const WORLD_DEPTH_MAXIMUM: f32 = 0.94;

/// Followed-object positions retained separately from the projection basis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCameraSubject {
    orbit_pivot: Vec3,
    position: Vec3,
}

impl WorldCameraSubject {
    /// Identifies the elevated collision pivot and authoritative object origin.
    #[must_use]
    pub const fn new(orbit_pivot: Vec3, position: Vec3) -> Self {
        Self {
            orbit_pivot,
            position,
        }
    }

    /// Returns the height-adjusted pivot used by camera obstruction traces.
    #[must_use]
    pub const fn orbit_pivot(self) -> Vec3 {
        self.orbit_pivot
    }

    /// Returns the followed object's authoritative world origin.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }
}

/// Final camera state after gameplay orbit, collision, and transition policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCamera {
    position: Vec3,
    target: Vec3,
    up: Vec3,
    vertical_field_of_view_radians: f32,
    near_clip: f32,
    far_clip: f32,
    subject: Option<WorldCameraSubject>,
}

impl WorldCamera {
    /// Creates the ordinary build-12340 world camera projection state.
    #[must_use]
    pub const fn stock(position: Vec3, target: Vec3, up: Vec3, far_clip: f32) -> Self {
        Self {
            position,
            target,
            up,
            vertical_field_of_view_radians: WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS,
            near_clip: WORLD_NEAR_CLIP,
            far_clip,
            subject: None,
        }
    }

    /// Creates an ordinary player camera with stock's distinct followed points.
    #[must_use]
    pub const fn stock_following(
        position: Vec3,
        target: Vec3,
        up: Vec3,
        orbit_pivot: Vec3,
        subject: Vec3,
        far_clip: f32,
    ) -> Self {
        Self {
            position,
            target,
            up,
            vertical_field_of_view_radians: WORLD_VERTICAL_FIELD_OF_VIEW_RADIANS,
            near_clip: WORLD_NEAR_CLIP,
            far_clip,
            subject: Some(WorldCameraSubject::new(orbit_pivot, subject)),
        }
    }

    /// Creates camera state for stock's non-world camera consumers.
    #[must_use]
    pub const fn new(
        position: Vec3,
        target: Vec3,
        up: Vec3,
        vertical_field_of_view_radians: f32,
        near_clip: f32,
        far_clip: f32,
    ) -> Self {
        Self {
            position,
            target,
            up,
            vertical_field_of_view_radians,
            near_clip,
            far_clip,
            subject: None,
        }
    }

    /// Returns the eye position used for fog, sorting, and view transforms.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Returns stock's one-unit view target rather than the followed subject.
    #[must_use]
    pub const fn target(self) -> Vec3 {
        self.target
    }

    /// Returns the continuous orbit roll reference supplied by camera policy.
    #[must_use]
    pub const fn up(self) -> Vec3 {
        self.up
    }

    /// Returns the vertical field of view in radians.
    #[must_use]
    pub const fn vertical_field_of_view_radians(self) -> f32 {
        self.vertical_field_of_view_radians
    }

    /// Returns the positive near clipping distance.
    #[must_use]
    pub const fn near_clip(self) -> f32 {
        self.near_clip
    }

    /// Returns the positive far clipping distance.
    #[must_use]
    pub const fn far_clip(self) -> f32 {
        self.far_clip
    }

    /// Returns followed-object positions when this is a subject camera.
    #[must_use]
    pub const fn subject(self) -> Option<WorldCameraSubject> {
        self.subject
    }
}

/// Validated camera basis and matrices shared by one rendered frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCameraFrame {
    camera: WorldCamera,
    aspect_ratio: f32,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    view: Mat4,
    projection: Mat4,
    view_projection: Mat4,
}

impl WorldCameraFrame {
    /// Stores one frame after camera-source validation and normalization.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        camera: WorldCamera,
        aspect_ratio: f32,
        forward: Vec3,
        right: Vec3,
        up: Vec3,
        view: Mat4,
        projection: Mat4,
    ) -> Self {
        Self {
            camera,
            aspect_ratio,
            forward,
            right,
            up,
            view,
            projection,
            view_projection: projection * view,
        }
    }

    /// Returns the validated source camera.
    #[must_use]
    pub const fn camera(self) -> WorldCamera {
        self.camera
    }

    /// Returns viewport width divided by height.
    #[must_use]
    pub const fn aspect_ratio(self) -> f32 {
        self.aspect_ratio
    }

    /// Returns the normalized world-space viewing direction.
    #[must_use]
    pub const fn forward(self) -> Vec3 {
        self.forward
    }

    /// Returns the normalized world-space right direction.
    #[must_use]
    pub const fn right(self) -> Vec3 {
        self.right
    }

    /// Returns the normalized, direction-orthogonal world-space up direction.
    #[must_use]
    pub const fn up(self) -> Vec3 {
        self.up
    }

    /// Returns the right-handed world-to-view matrix.
    #[must_use]
    pub const fn view(self) -> Mat4 {
        self.view
    }

    /// Returns the Vulkan zero-to-one, clip-Y-corrected projection matrix.
    #[must_use]
    pub const fn projection(self) -> Mat4 {
        self.projection
    }

    /// Returns the matrix uploaded to world scene descriptors.
    #[must_use]
    pub const fn view_projection(self) -> Mat4 {
        self.view_projection
    }
}

/// Normalized screen bounds used by portals and partial-cell visibility.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldScreenWindow {
    minimum_x: f32,
    minimum_y: f32,
    maximum_x: f32,
    maximum_y: f32,
}

impl WorldScreenWindow {
    /// The complete normalized viewport.
    pub const FULL: Self = Self {
        minimum_x: -1.0,
        minimum_y: -1.0,
        maximum_x: 1.0,
        maximum_y: 1.0,
    };

    /// Creates an authored normalized screen window.
    #[must_use]
    pub const fn new(minimum_x: f32, minimum_y: f32, maximum_x: f32, maximum_y: f32) -> Self {
        Self {
            minimum_x,
            minimum_y,
            maximum_x,
            maximum_y,
        }
    }

    /// Returns the left normalized boundary.
    #[must_use]
    pub const fn minimum_x(self) -> f32 {
        self.minimum_x
    }

    /// Returns the bottom normalized boundary.
    #[must_use]
    pub const fn minimum_y(self) -> f32 {
        self.minimum_y
    }

    /// Returns the right normalized boundary.
    #[must_use]
    pub const fn maximum_x(self) -> f32 {
        self.maximum_x
    }

    /// Returns the top normalized boundary.
    #[must_use]
    pub const fn maximum_y(self) -> f32 {
        self.maximum_y
    }

    /// Confirms that the four finite boundaries enclose positive area.
    pub(super) fn validate(self) -> bool {
        [
            self.minimum_x,
            self.minimum_y,
            self.maximum_x,
            self.maximum_y,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self.minimum_x < self.maximum_x
            && self.minimum_y < self.maximum_y
    }
}

impl Default for WorldScreenWindow {
    fn default() -> Self {
        Self::FULL
    }
}

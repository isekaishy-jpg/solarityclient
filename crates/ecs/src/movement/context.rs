//! Durable movement clocks, trajectory, and transport attachment state.

use glam::Vec3;

/// Conditional movement state retained alongside flags, speeds, and transform.
///
/// The native `MovementInfo` initializer at `0x004F4C50` zeroes the clocks and
/// trajectory values. Optional fields here preserve their actual admission;
/// later physics must not synthesize attachment or launch state from flags.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldMovementContext {
    /// Wrapping movement clock of the most recently admitted state, in milliseconds.
    pub timestamp_ms: u32,
    /// Admitted transport-relative position, facing, seat, and clocks.
    pub transport: Option<WorldMovementTransport>,
    /// Admitted swimming/flying/always-pitching angle in radians.
    pub pitch_radians: Option<f32>,
    /// Elapsed fall duration in milliseconds.
    pub fall_time_ms: u32,
    /// Launch parameters retained for the falling arc, including after its apex.
    pub falling: Option<WorldMovementFall>,
    /// Admitted authored spline elevation in yards.
    pub spline_elevation: Option<f32>,
}

/// Movement-owned attachment to a transport or vehicle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldMovementTransport {
    /// Exact admitted parent GUID.
    pub guid: u64,
    /// Parent-relative position in yards.
    pub position: Vec3,
    /// Parent-relative facing in radians.
    pub orientation: f32,
    /// Parent movement clock in milliseconds.
    pub time_ms: u32,
    /// Signed seat; -1 denotes no vehicle seat.
    pub seat: i8,
    /// Optional second interpolation clock in milliseconds.
    pub interpolated_time_ms: Option<u32>,
}

/// Persistent jump launch state, separate from live vertical velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldMovementFall {
    /// Vertical launch velocity in yards per second.
    pub vertical_speed: f32,
    /// Sine of the launch direction in the movement coordinate space.
    pub direction_sin: f32,
    /// Cosine of the launch direction in the movement coordinate space.
    pub direction_cos: f32,
    /// Horizontal launch speed in yards per second.
    pub horizontal_speed: f32,
}

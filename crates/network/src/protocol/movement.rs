//! Build-12340 movement snapshot fields shared by object and movement packets.

/// Movement data accompanying the living object's flags and world transform.
///
/// This is the `MovementInfo` field order emitted by `0x004F4ED0`. Optional
/// fields retain packet presence; an absent field is not an invented zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectMovementContext {
    /// Wrapping sender movement clock in milliseconds.
    pub timestamp_ms: u32,
    /// Transport-relative transform and clock, when ON_TRANSPORT is set.
    pub transport: Option<ObjectMovementTransport>,
    /// Pitch in radians for swimming, flying, or ALWAYS_ALLOW_PITCHING.
    pub pitch_radians: Option<f32>,
    /// Elapsed fall time in milliseconds; present even without FALLING.
    pub fall_time_ms: u32,
    /// Launch parameters retained throughout a falling arc.
    pub falling: Option<ObjectMovementFall>,
    /// Authored spline elevation when SPLINE_ELEVATION is set.
    pub spline_elevation: Option<f32>,
}

/// Exact transport attachment carried inside a living movement block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectMovementTransport {
    /// Packed wire GUID, including a transmitted zero GUID.
    pub guid: u64,
    /// Position relative to the transport, in yards.
    pub position: [f32; 3],
    /// Facing relative to the transport, in radians.
    pub orientation: f32,
    /// Transport clock in milliseconds.
    pub time_ms: u32,
    /// Signed vehicle seat; -1 is the native unseated sentinel.
    pub seat: i8,
    /// Second transport clock, present only with INTERPOLATED_MOVEMENT.
    pub interpolated_time_ms: Option<u32>,
}

/// Original jump launch values, not the current vertical velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectMovementFall {
    /// Vertical launch velocity in yards per second.
    pub vertical_speed: f32,
    /// Cosine of the retained horizontal launch direction.
    pub direction_cos: f32,
    /// Sine of the retained horizontal launch direction.
    pub direction_sin: f32,
    /// Horizontal launch speed in yards per second.
    pub horizontal_speed: f32,
}

//! Retained build-12340 path snapshot read by native `004F4B50`/`004F4AE0`.

/// The optional final-facing union, in native angle/target/point precedence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementSplineFacing {
    /// Keep the path's movement direction at completion.
    Direction,
    /// Face this world-space angle in radians.
    Angle(f32),
    /// Face the object with this unpacked GUID.
    Target(u64),
    /// Face this point in the path's coordinate system.
    Point([f32; 3]),
}

/// Complete spline snapshot; nodes include the stock path's control points.
///
/// Network decoding retains exact values. Path admission and evaluation belong
/// to the movement owner, independently of the object's last packet position.
#[derive(Clone, Debug, PartialEq)]
pub struct MovementSplineSnapshot {
    /// Exact flags, including cyclic, falling, and final-facing state.
    pub flags: u32,
    /// Final-facing union selected by the flags.
    pub facing: MovementSplineFacing,
    /// Time already spent on the path when the snapshot was sent.
    pub elapsed_ms: u32,
    /// Authored total traversal time in milliseconds.
    pub duration_ms: u32,
    /// Server path identity, independent of the object's GUID.
    pub id: u32,
    /// The three float words stored at native offsets 0x204 through 0x20c.
    pub timing_parameters: [f32; 3],
    /// Native effect start clock at offset 0x210, in milliseconds.
    pub effect_start_ms: u32,
    /// Exact ordered path points, including endpoint controls.
    pub nodes: Vec<[f32; 3]>,
    /// Native path evaluation mode byte.
    pub mode: u8,
    /// Destination supplied separately from the control-point array.
    pub destination: [f32; 3],
}

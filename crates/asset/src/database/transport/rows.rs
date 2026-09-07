//! Stored transport rows; behavior interprets the native flags and clock units.

/// One 11-word TaxiPathNode record, including endpoint spline controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaxiPathNode {
    /// Primary database record identifier.
    pub id: u32,
    /// Template-selected route identifier.
    pub path_id: u32,
    /// Authored index, retained without sorting or repairing stored node order.
    pub node_index: u32,
    /// Map containing this continuous route section.
    pub map_id: u32,
    /// Server Z-up world coordinates.
    pub position: [f32; 3],
    /// Bit 0 splits after this node; bit 1 creates a station stop.
    pub flags: u32,
    /// Authored station dwell time in seconds.
    pub delay_seconds: u32,
    /// Event identifiers dispatched at their independently calculated times.
    pub arrival_event: u32,
    /// Event identifier dispatched after station dwell.
    pub departure_event: u32,
}

/// One 11-word TransportPhysics record consumed by native 7F7B30/7F7DD0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportPhysicsRecord {
    /// Primary key selected by the server template's physics property.
    pub id: u32,
    /// Bob amplitude/frequency, roll amplitude/frequency, pitch amplitude/frequency,
    /// bank amplitude, maximum turn, maximum speed, and idle gain, in that order.
    pub parameters: [f32; 10],
}

/// One 7-word TransportAnimation record for the type-11 transport family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportAnimationNode {
    /// Primary database record identifier.
    pub id: u32,
    /// GameObject template entry, distinct from its display identifier.
    pub entry: u32,
    /// Key time in milliseconds.
    pub time_ms: u32,
    /// Authored local position.
    pub position: [f32; 3],
    /// Authored animation-sequence identifier.
    pub sequence_id: u32,
}

/// One 7-word TransportRotation record for the type-11 transport family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransportRotationNode {
    /// Primary database record identifier.
    pub id: u32,
    /// GameObject template entry, distinct from its display identifier.
    pub entry: u32,
    /// Key time in milliseconds.
    pub time_ms: u32,
    /// Authored XYZW quaternion; preserved without normalization.
    pub rotation: [f32; 4],
}

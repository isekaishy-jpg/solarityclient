//! Map- and hardware-aware effective world viewing distance.

use solarity_ecs::WorldMapId;
use thiserror::Error;

/// Registered build-12340 `farclip` default.
pub const DEFAULT_WORLD_VIEW_DISTANCE: f32 = 777.0;

/// Native minimum applied after the script-visible CVar range.
pub const WORLD_VIEW_DISTANCE_MINIMUM: f32 = 183.333_33;

/// Maximum used by old-map policy and memory-constrained newer maps.
pub const LEGACY_WORLD_VIEW_DISTANCE_MAXIMUM: f32 = 791.666_7;

/// Maximum used by explicitly overridden old maps or eligible newer maps.
pub const EXTENDED_WORLD_VIEW_DISTANCE_MAXIMUM: f32 = 1_583.333_4;

/// Physical-memory boundary tested strictly by build 12340.
const EXTENDED_VIEW_PHYSICAL_MEMORY_THRESHOLD_BYTES: u64 = 0x4000_0000;

/// Inputs read by the executable before world residency is derived.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldViewDistanceRequest {
    requested: f32,
    map_id: WorldMapId,
    far_clip_override: bool,
    total_physical_memory_bytes: u64,
}

impl WorldViewDistanceRequest {
    /// Captures the CVar, active map, and process-start memory report.
    #[must_use]
    pub const fn new(requested: f32, map_id: WorldMapId, total_physical_memory_bytes: u64) -> Self {
        Self {
            requested,
            map_id,
            far_clip_override: false,
            total_physical_memory_bytes,
        }
    }

    /// Applies the hidden nonzero `farClipOverride` CVar.
    #[must_use]
    pub const fn with_far_clip_override(mut self) -> Self {
        self.far_clip_override = true;
        self
    }
}

/// Ceiling branch selected by stock map and memory policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldViewDistanceLimit {
    /// Old-map or memory-constrained maximum of 791.6667 units.
    Legacy,
    /// Override- or memory-enabled maximum of 1583.33337 units.
    Extended,
}

/// Effective distance shared by camera, fog, residency, and spatial demand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldViewDistance {
    value: f32,
    limit: WorldViewDistanceLimit,
}

impl WorldViewDistance {
    /// Returns the effective world-space distance after native clamping.
    #[must_use]
    pub const fn value(self) -> f32 {
        self.value
    }

    /// Returns the stock ceiling branch selected for the active world.
    #[must_use]
    pub const fn limit(self) -> WorldViewDistanceLimit {
        self.limit
    }
}

/// Invalid state read from the configured world-view CVar.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldViewDistanceError {
    /// The requested distance is NaN or infinite.
    #[error("requested world view distance is not finite")]
    NonFiniteRequested,
}

/// Applies build 12340's map-aware `0x00780770` viewing-distance clamp.
///
/// Maps below 530 plus maps 543 and 575 are legacy-policy maps. They require
/// the hidden override for the extended ceiling. Every other map requires a
/// physical-memory report strictly greater than one GiB.
///
/// # Errors
///
/// Returns [`WorldViewDistanceError::NonFiniteRequested`] when CVar state
/// cannot participate in the native clamp.
pub fn resolve_world_view_distance(
    request: WorldViewDistanceRequest,
) -> Result<WorldViewDistance, WorldViewDistanceError> {
    if !request.requested.is_finite() {
        return Err(WorldViewDistanceError::NonFiniteRequested);
    }

    let map_id = request.map_id.value();
    let uses_legacy_map_policy = map_id < 530 || map_id == 543 || map_id == 575;
    let extended_allowed = if uses_legacy_map_policy {
        request.far_clip_override
    } else {
        request.total_physical_memory_bytes > EXTENDED_VIEW_PHYSICAL_MEMORY_THRESHOLD_BYTES
    };
    let (maximum, limit) = if extended_allowed {
        (
            EXTENDED_WORLD_VIEW_DISTANCE_MAXIMUM,
            WorldViewDistanceLimit::Extended,
        )
    } else {
        (
            LEGACY_WORLD_VIEW_DISTANCE_MAXIMUM,
            WorldViewDistanceLimit::Legacy,
        )
    };

    Ok(WorldViewDistance {
        value: request
            .requested
            .clamp(WORLD_VIEW_DISTANCE_MINIMUM, maximum),
        limit,
    })
}

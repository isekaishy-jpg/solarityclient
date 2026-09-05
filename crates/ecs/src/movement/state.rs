//! Authoritative living-object movement state retained outside rendering.

use shipyard::Component;

use super::context::WorldMovementContext;

/// Nine living-object speeds in build-12340 packet order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldMovementSpeeds {
    values: [f32; 9],
}

impl WorldMovementSpeeds {
    /// Creates walk, run, run-back, swim, swim-back, flight, flight-back,
    /// turn-rate, and pitch-rate state from the exact wire vector.
    #[must_use]
    pub const fn new(values: [f32; 9]) -> Self {
        Self { values }
    }

    /// Returns all speed values without collapsing distinct movement modes.
    #[must_use]
    pub const fn values(self) -> [f32; 9] {
        self.values
    }

    /// Returns ground walking speed in yards per second.
    #[must_use]
    pub const fn walk(self) -> f32 {
        self.values[0]
    }

    /// Returns forward ground running speed in yards per second.
    #[must_use]
    pub const fn run(self) -> f32 {
        self.values[1]
    }

    /// Returns backward ground running speed in yards per second.
    #[must_use]
    pub const fn run_back(self) -> f32 {
        self.values[2]
    }

    /// Returns forward swimming speed in yards per second.
    #[must_use]
    pub const fn swim(self) -> f32 {
        self.values[3]
    }

    /// Returns backward swimming speed in yards per second.
    #[must_use]
    pub const fn swim_back(self) -> f32 {
        self.values[4]
    }

    /// Returns forward flight speed in yards per second.
    #[must_use]
    pub const fn flight(self) -> f32 {
        self.values[5]
    }

    /// Returns backward flight speed in yards per second.
    #[must_use]
    pub const fn flight_back(self) -> f32 {
        self.values[6]
    }

    /// Returns angular yaw speed in radians per second.
    #[must_use]
    pub const fn turn_rate(self) -> f32 {
        self.values[7]
    }

    /// Returns angular pitch speed in radians per second.
    #[must_use]
    pub const fn pitch_rate(self) -> f32 {
        self.values[8]
    }
}

/// Latest complete living movement state for one visible object.
#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct WorldMovementState {
    flags: u64,
    speeds: WorldMovementSpeeds,
    context: WorldMovementContext,
}

impl WorldMovementState {
    /// Creates state from exact flags, speeds, and the admitted movement context.
    #[must_use]
    pub const fn new(
        flags: u64,
        speeds: WorldMovementSpeeds,
        context: WorldMovementContext,
    ) -> Self {
        Self {
            flags,
            speeds,
            context,
        }
    }

    /// Returns the exact combined 48-bit movement flag word.
    #[must_use]
    pub const fn flags(self) -> u64 {
        self.flags
    }

    /// Returns the complete speed vector accompanying the flags.
    #[must_use]
    pub const fn speeds(self) -> WorldMovementSpeeds {
        self.speeds
    }

    /// Returns the transport parent named by the authoritative movement block.
    #[must_use]
    pub const fn transport_guid(self) -> Option<u64> {
        match self.context.transport {
            Some(transport) => Some(transport.guid),
            None => None,
        }
    }

    /// Returns the retained clocks, transport attachment, pitch, and launch state.
    #[must_use]
    pub const fn context(self) -> WorldMovementContext {
        self.context
    }
}

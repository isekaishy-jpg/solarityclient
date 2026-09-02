//! Ordered loading-card progress derived from complete first-world readiness.

/// Real transition milestones represented by retained loading-card generations.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum RuntimeLoadingStage {
    /// `CMSG_PLAYER_LOGIN` is in flight.
    AwaitingWorld,
    /// The server accepted the character and published world coordinates.
    WorldAccepted,
    /// Map lighting and environment state are available.
    EnvironmentReady,
    /// The controlled player model is resident.
    PlayerReady,
    /// Terrain and all first-frame inputs are resident.
    SceneReady,
}

impl RuntimeLoadingStage {
    pub(crate) const ALL: [Self; 5] = [
        Self::AwaitingWorld,
        Self::WorldAccepted,
        Self::EnvironmentReady,
        Self::PlayerReady,
        Self::SceneReady,
    ];

    pub(crate) const fn progress(self) -> f32 {
        match self {
            Self::AwaitingWorld => 0.05,
            Self::WorldAccepted => 0.30,
            Self::EnvironmentReady => 0.64,
            Self::PlayerReady => 0.82,
            Self::SceneReady => 1.00,
        }
    }

    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Current first-world subsystem facts used to select a cumulative stage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RuntimeLoadingReadiness {
    /// The authenticated session owns an active world.
    pub(crate) world_accepted: bool,
    /// Lighting and environment state are ready for that world.
    pub(crate) environment_ready: bool,
    /// The controlled player's render inputs are resident.
    pub(crate) player_ready: bool,
    /// Terrain and its complete GPU generation are resident.
    pub(crate) scene_ready: bool,
    /// The local movement parent's object/resource dependency has completed.
    pub(crate) transport_resource_ready: bool,
}

impl RuntimeLoadingReadiness {
    /// Returns the highest stage whose complete prerequisite chain is ready.
    pub(crate) const fn stage(self) -> RuntimeLoadingStage {
        if !self.world_accepted {
            return RuntimeLoadingStage::AwaitingWorld;
        }
        if !self.environment_ready {
            return RuntimeLoadingStage::WorldAccepted;
        }
        if !self.player_ready {
            return RuntimeLoadingStage::EnvironmentReady;
        }
        // Stock `0x006E7F50 -> 0x00409800` retains the card when the local
        // movement state names a transport until that game object resolves.
        if !self.scene_ready || !self.transport_resource_ready {
            return RuntimeLoadingStage::PlayerReady;
        }
        RuntimeLoadingStage::SceneReady
    }
}

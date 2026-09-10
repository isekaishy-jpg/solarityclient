//! Frusta and fog writes retained by native group callbacks.

use super::{
    WorldModelVisibilityError, WorldModelVisibilityVisit, WorldSceneCameraFrame, WorldSceneFrustum,
};

/// Fog-bank state written before a native group callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldModelSceneFog {
    /// Direct 7A6B60/7AD1F0 callbacks retain the preceding global bank.
    Inherited,
    /// 7AC060 stores a nonzero indoor bank before calling the group consumer.
    Indoor,
    /// 7AC060 stores zero before calling the group consumer.
    Outdoor,
}

/// One ordered group callback and its complete current world-space clip.
#[derive(Clone, Copy, Debug)]
pub struct WorldModelSceneGroupVisit {
    /// Authored group index; repeated portal paths produce repeated visits.
    pub group: usize,
    /// The callback's fog-state write, including direct callbacks with no write.
    pub fog: WorldModelSceneFog,
    /// The current native frustum copied by 799310 for later batch selection.
    pub frustum: WorldSceneFrustum,
}

impl WorldModelVisibilityVisit {
    /// Resolves the scene clip copied by this portal-traversal callback.
    ///
    /// Initial visits inherit the caller's exact full or outdoor-window frustum.
    /// Recursive 7AC060 visits add one to their clip rectangle, spill to f32,
    /// then divide by two through 7A6DA0/7A6DD0 before 790E20 crops the original
    /// camera corners. Recropping an initial full window changes native stores.
    ///
    /// # Errors
    /// Rejects malformed recursive windows or degenerate resulting clip faces.
    pub fn frustum(
        self,
        camera: WorldSceneCameraFrame,
        inherited: WorldSceneFrustum,
    ) -> Result<WorldSceneFrustum, WorldModelVisibilityError> {
        if self.depth == 0 {
            return Ok(inherited);
        }
        camera.frustum_for_window(self.screen_window.map(|value| (value + 1.) * 0.5))
    }
}

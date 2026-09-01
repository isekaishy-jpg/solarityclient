//! Initial controlled-player contact with the resident world surface.

use glam::Vec3;
use solarity_ecs::WorldTransform;
use thiserror::Error;

/// One terrain support admitted for the controlled player after world entry.
///
/// `Wow.exe` `0x00762E00` dispatches ordinary and falling motion to
/// `0x007620F0`/`0x007618B0`; `0x00988370` initializes the falling state. A
/// live pinned-executable probe additionally observed the login transform move
/// from server Z `383.0` to resident support Z `382.7837` before camera
/// consumption. This value object keeps that write out of camera/render code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEntryGroundContact {
    position: Vec3,
}

impl WorldEntryGroundContact {
    /// Resolves the server transform onto an exact resident surface height.
    ///
    /// # Errors
    ///
    /// Returns [`WorldEntryGroundContactError`] for a non-finite transform or
    /// surface, since either would contaminate the ECS movement component.
    pub fn resolve(
        transform: WorldTransform,
        surface_height: f32,
    ) -> Result<Self, WorldEntryGroundContactError> {
        let mut position = transform.position();
        if !position.is_finite() || !transform.orientation().is_finite() {
            return Err(WorldEntryGroundContactError::NonFiniteTransform);
        }
        if !surface_height.is_finite() {
            return Err(WorldEntryGroundContactError::NonFiniteSurface);
        }
        position.z = surface_height;
        Ok(Self { position })
    }

    /// Returns the movement-owned transform retaining server orientation.
    #[must_use]
    pub const fn transform(self, orientation: f32) -> WorldTransform {
        WorldTransform::new(self.position, orientation)
    }
}

/// Invalid input at the first controlled-player ground-contact boundary.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WorldEntryGroundContactError {
    /// The authoritative server transform is NaN or infinite.
    #[error("world-entry player transform is not finite")]
    NonFiniteTransform,
    /// The resident world returned a NaN or infinite ground height.
    #[error("world-entry player ground height is not finite")]
    NonFiniteSurface,
}

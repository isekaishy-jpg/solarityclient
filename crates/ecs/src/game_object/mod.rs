//! Game-object identity and statistics evidenced by the `GameObject` source family.

mod game_object_c;
mod game_object_stats;
mod movement;
mod pose;

pub use game_object_c::GameObjectPresentation;
pub use movement::{GameObjectMovement, GameObjectTransport};
pub use pose::GameObjectAnimatedPose;

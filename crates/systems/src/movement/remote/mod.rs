//! Server timestamp admission and retained ordinary movement commands.

mod blend;
mod clock;

pub use blend::{RemoteMovementBlend, RemoteMovementPose};
pub use clock::{RemoteMovementAdmission, RemoteMovementClock, RemoteMovementReceipt};

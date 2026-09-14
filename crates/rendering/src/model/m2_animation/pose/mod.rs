//! Complete GPU palettes and demand-limited CPU bone consumers share exact math.

mod compose;
mod lookup;

pub use compose::M2BoneSamples;
pub use compose::{M2BonePose, M2BonePoseOverrides, M2FingerPoseHands};
pub use lookup::M2BoneTransforms;

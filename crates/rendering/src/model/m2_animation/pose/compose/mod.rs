//! Parent-first M2 bone transform composition.

mod samples;
mod sampling;
mod storage;

pub use samples::M2BoneSamples;

use glam::Mat4;

use super::super::{M2AnimationClock, M2BonePoseError};

/// Per-hand selection for the model-authored `HandsClosed` finger pose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum M2FingerPoseHands {
    /// Do not replace either finger tree.
    None,
    /// Replace key-bone groups 8 through 12.
    Right,
    /// Replace key-bone groups 13 through 17.
    Left,
    /// Replace both authored finger trees.
    Both,
}

impl M2FingerPoseHands {
    /// Returns whether this mask includes one requested hand.
    #[must_use]
    pub const fn includes(self, hand: Self) -> bool {
        matches!(
            (self, hand),
            (Self::Right | Self::Both, Self::Right) | (Self::Left | Self::Both, Self::Left)
        )
    }
}

/// One complete model-bone matrix palette ready for GPU upload.
#[derive(Debug, Default)]
pub struct M2BonePose {
    transforms: Vec<Mat4>,
    local: Vec<Mat4>,
    sequence_clocks: Vec<Option<M2AnimationClock>>,
    identity_pose: bool,
    // Declared last so all vector payloads retire before their shared byte charge.
    memory: Option<storage::PoseMemory>,
}

impl PartialEq for M2BonePose {
    fn eq(&self, other: &Self) -> bool {
        self.transforms == other.transforms
            && self.local == other.local
            && self.sequence_clocks == other.sequence_clocks
            && self.identity_pose == other.identity_pose
    }
}

/// Instance-owned modifications applied while composing the authored pose.
#[derive(Clone, Copy, Debug, Default)]
pub struct M2BonePoseOverrides<'a> {
    /// Billboard exceptions selected by the character compositor.
    pub model_oriented_billboard_bones: &'a [bool],
    /// An authored held-item pose restricted to the selected finger trees.
    pub finger_pose: Option<(M2AnimationClock, M2FingerPoseHands)>,
    /// Additional local transforms indexed by the model's semantic key bones.
    /// Missing key bones are ignored, as in the native model setter.
    pub bone_transforms: &'a [(u16, Mat4)],
    /// Sequence clocks rooted at semantic bones, inherited by descendants.
    /// The nearest root wins; later entries replace earlier entries at a root.
    pub bone_sequences: &'a [(u16, M2AnimationClock)],
}

/// Precomputed camera transforms shared by every billboard bone in a pose.
#[derive(Clone, Copy)]
struct BillboardView {
    model_view: Mat4,
    inverse_model_view: Mat4,
}

/// Ensures matrix inversion cannot inject non-finite billboard transforms.
fn finite_matrix(value: Mat4) -> bool {
    value.to_cols_array().into_iter().all(f32::is_finite)
}

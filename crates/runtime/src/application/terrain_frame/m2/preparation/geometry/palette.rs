//! Owned pose overrides for visible models with no ordered CPU bone consumer.

use glam::Mat4;
use solarity_rendering::{M2AnimationClock, M2BonePoseOverrides, M2FingerPoseHands};

/// Retained override storage avoids allocating a separate pose task per frame.
#[derive(Default)]
pub(super) struct PaletteInput {
    pub(super) pending: bool,
    fingers: Option<(M2AnimationClock, M2FingerPoseHands)>,
    transforms: Vec<(u16, Mat4)>,
    sequences: Vec<(u16, M2AnimationClock)>,
}

impl PaletteInput {
    pub(super) fn prepare(&mut self, input: Option<M2BonePoseOverrides<'_>>) {
        self.pending = input.is_some();
        self.transforms.clear();
        self.sequences.clear();
        self.fingers = None;
        if let Some(input) = input {
            self.fingers = input.finger_pose;
            self.transforms.extend_from_slice(input.bone_transforms);
            self.sequences.extend_from_slice(input.bone_sequences);
        }
    }

    pub(super) fn overrides<'a>(&'a self, orientation: &'a [bool]) -> M2BonePoseOverrides<'a> {
        M2BonePoseOverrides {
            model_oriented_billboard_bones: orientation,
            finger_pose: self.fingers,
            bone_transforms: &self.transforms,
            bone_sequences: &self.sequences,
        }
    }
}

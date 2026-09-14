//! CPU consumers read named bones without requiring an uploadable full palette.

use glam::Mat4;
use solarity_asset::{M2AnimationSet, M2Attachment};

use super::super::sample::sample_discrete;
use super::super::{M2AnimationClock, M2BonePoseError};
use super::M2BonePose;

/// Exact bone lookup shared by full render palettes and selected CPU samples.
/// A missing sample is never substituted with an identity transform.
pub trait M2BoneTransforms {
    /// Number of authored bones, independent of the number sampled this frame.
    fn bone_count(&self) -> usize;

    /// Returns a current sampled transform; absent or unrequested bones return None.
    fn bone_transform(&self, index: usize) -> Option<Mat4>;

    /// Resolves the same authored enable track and placement as a complete pose.
    ///
    /// # Errors
    /// Returns the ordinary clock error or a missing attachment-bone error.
    fn attachment_transform(
        &self,
        animations: &M2AnimationSet,
        attachment: &M2Attachment,
        clock: M2AnimationClock,
        model_transform: Mat4,
    ) -> Result<Option<Mat4>, M2BonePoseError> {
        let clock = clock.resolve(animations)?;
        if sample_discrete(animations, attachment.enabled(), clock, 1_u8) == 0 {
            return Ok(None);
        }
        let bone = self
            .bone_transform(usize::from(attachment.bone_index()))
            .ok_or(M2BonePoseError::AttachmentBoneIndex {
                requested: attachment.bone_index(),
                available: self.bone_count(),
            })?;
        Ok(Some(
            model_transform * bone * Mat4::from_translation(attachment.position()),
        ))
    }
}

impl M2BoneTransforms for M2BonePose {
    fn bone_count(&self) -> usize {
        self.transforms().len()
    }
    fn bone_transform(&self, index: usize) -> Option<Mat4> {
        self.transforms().get(index).copied()
    }
}

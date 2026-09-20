//! Owner-local attachment requests and ordered frame publication.

mod requests;
mod samples;

use glam::Mat4;
use solarity_rendering::CharacterAttachmentPoint;

pub(super) use requests::{AttachmentRequests, MountedOwners};
pub(super) use samples::AttachmentSamples;

/// Item attachment membership retained until placement topology changes.
pub(super) type ItemRequests = AttachmentRequests<(u64, CharacterAttachmentPoint)>;
/// Visual membership grouped by its owning item, preserving duplicate requests.
pub(super) type VisualRequests = AttachmentRequests<(u64, CharacterAttachmentPoint, u32)>;
/// Ordered mount samples; absent and explicitly hidden parents remain distinct.
pub(super) type RiderSamples = AttachmentSamples<(u64, Option<Mat4>)>;
/// Ordered body-to-item samples.
pub(super) type ItemSamples = AttachmentSamples<(u64, CharacterAttachmentPoint, Option<Mat4>)>;
/// Ordered item-to-visual samples.
pub(super) type VisualSamples =
    AttachmentSamples<(u64, CharacterAttachmentPoint, u32, Option<Mat4>)>;

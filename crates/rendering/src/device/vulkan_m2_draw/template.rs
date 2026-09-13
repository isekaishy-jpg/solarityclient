//! Immutable resource joins retained by a model source, independent of placement.

use super::M2PreparedDraw;
use crate::{M2MaterialUniform, device::VulkanError};

/// A validated mesh/material/pipeline combination. Constructed by the renderer
/// at source admission; placements supply only their changing instance state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M2DrawTemplate(pub(in crate::device) M2PreparedDraw);

impl M2DrawTemplate {
    /// Captures the already validated zero-offset source packet.
    pub(in crate::device) const fn new(draw: M2PreparedDraw) -> Self {
        Self(draw)
    }
    /// Instantiates the validated combination without reopening resource registries.
    ///
    /// # Errors
    /// Returns a palette-range error if the global bone offset overflows.
    pub fn instantiate(
        self,
        material: M2MaterialUniform,
        bone_offset: u32,
        flags: u32,
    ) -> Result<M2PreparedDraw, VulkanError> {
        self.0.instantiate(material, bone_offset, flags)
    }
}

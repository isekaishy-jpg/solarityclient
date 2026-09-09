//! Camera-root exterior admission over the complete native portal pipeline.

use glam::Vec3;

use super::{
    WorldModelPortalProjector, WorldModelSceneVisibilityEvent, WorldModelVisibilityError,
    WorldModelVisibilityQuery, WorldSceneCameraFrame,
};
use crate::collision::PlacedWorldModelCollision;

/// Reusable scene storage for deciding whether an interior opens outdoor lists.
#[derive(Default)]
pub struct WorldModelExteriorSceneQuery {
    projector: WorldModelPortalProjector,
    visibility: WorldModelVisibilityQuery,
}

impl WorldModelExteriorSceneQuery {
    /// Resolves the primary camera root's exterior portal admission.
    ///
    /// 79A870 clears exterior windows after visiting a secondary camera root.
    /// The primary root's 7AD1F0 traversal then decides whether outdoor depth
    /// lists run. Only accepted portal callbacks to groups masked by 0x10008
    /// reach 790AD0's exterior bank. Other 50148 callbacks affect a separate bank.
    /// The native constructor 9CE7E0 installs the recursion limit of ten.
    ///
    /// # Errors
    /// Returns malformed camera/root transforms, group indices or projection
    /// inputs before admission can be published to movement consumers.
    pub fn query_camera_root(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        initial_groups: &[usize],
    ) -> Result<bool, WorldModelVisibilityError> {
        let frame = camera.for_root(root.transform, root.inverse_transform)?;
        let forward_plane = camera.local_forward_plane(root.inverse_transform)?;
        let model = &root.model;
        let projected = self.projector.project(model, frame)?;
        let events = self.visibility.query_scene(
            model,
            frame.local_camera,
            initial_groups,
            10,
            projected,
        )?;
        for event in events {
            let WorldModelSceneVisibilityEvent::ExteriorPortal { reference } = event else {
                continue;
            };
            let reference = model.portal_references()[*reference];
            if model.group_info()[usize::from(reference.group_index())].flags() & 0x10008 == 0 {
                continue;
            }
            let portal = model.portals()[usize::from(reference.portal_index())];
            let start = usize::from(portal.vertex_start());
            let end = start + usize::from(portal.vertex_count());
            if self
                .projector
                .project_exterior_polygon(
                    &model.portal_vertices()[start..end],
                    Vec3::from_array(portal.normal()),
                    reference.side(),
                    forward_plane,
                    frame,
                )?
                .is_some()
            {
                // Every surviving 7A70D0 depth is nonnegative, which is the
                // 79A870 gate. Subsequent windows cannot revoke this admission.
                return Ok(true);
            }
        }
        Ok(false)
    }
}

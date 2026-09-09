//! Unit collision admission retained from the scene for the next movement pass.

#[cfg(test)]
#[path = "../../../../tests/application/unit_scene.rs"]
mod tests;

use std::collections::HashSet;

use glam::Vec3;
use solarity_rendering::WorldCameraFrame;
use solarity_systems::{
    MovementCollisionBounds, PlacedWorldModelCollision, WorldModelCameraRegistration,
    WorldModelCameraSceneQuery, WorldModelRegistrationSelection, WorldModelVisibilityError,
    WorldSceneCameraFrame, WorldSceneDepthFrame,
};

use super::{
    ResidentTerrainMap, RuntimeMovementRegistrationError, RuntimeTerrainCoordinator,
    RuntimeWorldModelMovementOwner,
};

/// Reuses camera traversal scratch and eligible root/group destinations each frame.
#[derive(Default)]
pub(super) struct UnitSceneAdmission {
    query: WorldModelCameraSceneQuery,
    groups: HashSet<(RuntimeWorldModelMovementOwner, usize)>,
    outdoor: Option<WorldSceneDepthFrame>,
}

impl UnitSceneAdmission {
    /// 79A870 visits both camera roots, but only the primary opens outdoor lists.
    fn prepare(
        &mut self,
        active: &mut ResidentTerrainMap,
        camera: WorldCameraFrame,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        self.groups.clear();
        self.outdoor = None;
        let source = camera.camera();
        let eye = source.position();
        let target = source.target();
        if let Some(registration) = active.camera_registration(eye)? {
            let scene = WorldSceneCameraFrame::perspective(
                eye,
                target,
                source.view_direction(),
                source.up(),
                source
                    .vertical_field_of_view_radians()
                    .ok_or(WorldModelVisibilityError::InvalidCameraProjection)?,
                camera.aspect_ratio(),
                [source.near_clip(), source.far_clip()],
            )?;
            let primary = active.movement.roots[registration.primary.owner].owner();
            for selected in registration
                .secondary
                .into_iter()
                .chain([registration.primary])
            {
                let reference = active.movement.roots[selected.owner];
                let owner = reference.owner();
                let root = active.registration_root_mut(reference)?;
                let exterior = self.record_camera_root(
                    root,
                    scene,
                    WorldModelCameraRegistration {
                        owner,
                        group: selected.group,
                        secondary_group: selected.secondary_group,
                    },
                    primary,
                )?;
                if owner == primary && exterior {
                    self.outdoor = Some(WorldSceneDepthFrame::new(eye, target)?);
                }
            }
        } else {
            self.outdoor = Some(WorldSceneDepthFrame::new(eye, target)?);
        }
        Ok(())
    }

    /// 79A260 excludes secondary-root exterior/direct groups from unit callbacks.
    fn record_camera_root(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        registration: WorldModelCameraRegistration<RuntimeWorldModelMovementOwner>,
        primary: RuntimeWorldModelMovementOwner,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        let initial = [
            registration.group,
            registration.secondary_group.unwrap_or(registration.group),
        ];
        let count = 1 + usize::from(registration.secondary_group.is_some());
        let exterior = self
            .query
            .query_camera_root(root, camera, &initial[..count])?;
        for &group in self.query.groups() {
            if registration.owner == primary
                || root.model().group_info()[group].flags() & 0x10008 == 0
            {
                self.groups.insert((registration.owner, group));
            }
        }
        Ok(exterior)
    }

    /// 7C2A70 links both primary banks; fallback floor banks are not scene lists.
    /// 793270 marks indoor collision before testing the unit's visibility window.
    fn admits_registration(
        &self,
        selection: WorldModelRegistrationSelection<RuntimeWorldModelMovementOwner>,
        bounds: MovementCollisionBounds,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        if selection
            .selected()
            .is_some_and(|candidate| candidate.hit().is_interior())
        {
            return Ok(selection.primary().into_iter().flatten().any(|candidate| {
                self.groups
                    .contains(&(candidate.owner(), candidate.hit().group_index()))
            }));
        }
        Ok(self
            .outdoor
            .map(|depth| depth.m2_depth_bin([bounds.minimum(), bounds.maximum()]))
            .transpose()?
            .flatten()
            .is_some())
    }
}

impl RuntimeTerrainCoordinator {
    /// Prepares the current camera's scene destinations before dynamic unit updates.
    pub(in crate::application) fn prepare_unit_scene(
        &mut self,
        camera: WorldCameraFrame,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        let mut admission = std::mem::take(&mut active.movement.unit_scene);
        let result = admission.prepare(active, camera);
        // Return retained storage even when resident geometry rejects the query.
        active.movement.unit_scene = admission;
        result
    }

    /// Matches a unit's current point registration against prepared scene lists.
    pub(in crate::application) fn unit_scene_admits(
        &mut self,
        position: Vec3,
        bounds: MovementCollisionBounds,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        let Some(active) = &mut self.active else {
            return Ok(false);
        };
        let selection = active.unit_registration(position)?;
        active
            .movement
            .unit_scene
            .admits_registration(selection, bounds)
    }
}

//! Unit collision admission retained from the scene for the next movement pass.

#[cfg(test)]
#[path = "../../../../tests/application/unit_scene.rs"]
mod tests;

use std::collections::HashSet;

use glam::Vec3;
use solarity_rendering::WorldCameraFrame;
use solarity_systems::{
    MovementCollisionBounds, PlacedWorldModelCollision, WorldModelCameraRegistration,
    WorldModelCameraSceneQuery, WorldModelExteriorPortalWindow, WorldModelRegistrationSelection,
    WorldModelVisibilityError, WorldSceneCameraFrame, WorldSceneDepthFrame,
};

use super::{
    MovementRootReference, ResidentTerrainMap, RuntimeMovementRegistrationError,
    RuntimeTerrainCoordinator, RuntimeWorldModelMovementOwner,
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
        let mut primary_owner = None;
        let mut outdoor_window = None;
        if let Some(registration) = active.camera_registration(eye)? {
            let primary = active.movement.roots[registration.primary.owner].owner();
            primary_owner = Some(primary);
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
                if owner == primary {
                    outdoor_window = exterior.map(|window| window.screen_window);
                }
            }
        } else {
            outdoor_window = Some([0., 0., 1., 1.]);
        }
        if let Some(window) = outdoor_window {
            let depth = WorldSceneDepthFrame::new(eye, target)?;
            self.outdoor = Some(depth);
            for index in 0..active.movement.roots.len() {
                let reference = active.movement.roots[index];
                // 792AD0 sends roots marked 0x400 to an overlap list. Its
                // separate 792BD0/799F80 passes are not connected here yet.
                if !matches!(reference, MovementRootReference::Static(_)) {
                    continue;
                }
                let root = active.registration_root_mut(reference)?;
                self.record_outdoor_root(
                    root,
                    scene,
                    reference.owner(),
                    primary_owner,
                    depth,
                    window,
                )?;
            }
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
    ) -> Result<Option<WorldModelExteriorPortalWindow>, RuntimeMovementRegistrationError> {
        let initial = [
            registration.group,
            registration.secondary_group.unwrap_or(registration.group),
        ];
        let count = 1 + usize::from(registration.secondary_group.is_some());
        self.query
            .query_camera_root(root, camera, &initial[..count])?;
        for &group in self.query.groups() {
            if registration.owner == primary
                || root.model().group_info()[group].flags() & 0x10008 == 0
            {
                self.groups.insert((registration.owner, group));
            }
        }
        Ok(self.query.exterior_window())
    }

    /// 792AD0 depth-lists exterior MOGI groups before 79A160/7B3A10 traversal.
    /// All bins run; with optional occlusion disabled, their order cannot alter
    /// the final set of 79A260 unit callback destinations.
    fn record_outdoor_root(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        owner: RuntimeWorldModelMovementOwner,
        primary: Option<RuntimeWorldModelMovementOwner>,
        depth: WorldSceneDepthFrame,
        screen_window: [f32; 4],
    ) -> Result<(), RuntimeMovementRegistrationError> {
        for (group, info) in root.model().group_info().iter().enumerate() {
            if info.flags() & 0x10008 == 0
                || depth.depth_bin(root.scene_group_bounds(group)?)?.is_none()
            {
                continue;
            }
            for &visited in self
                .query
                .query_outdoor_group(root, camera, group, screen_window)?
            {
                if Some(owner) == primary
                    || root.model().group_info()[visited].flags() & 0x10008 == 0
                {
                    self.groups.insert((owner, visited));
                }
            }
        }
        Ok(())
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
            .map(|depth| depth.depth_bin([bounds.minimum(), bounds.maximum()]))
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

//! Native scene traversal shared by graphics and subsequent unit movement.

#[cfg(test)]
#[path = "../../../../../tests/application/unit_scene.rs"]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    ops::ControlFlow,
};

use glam::Vec3;
use solarity_rendering::WorldCameraFrame;
use solarity_systems::{
    MovementCollisionBounds, PlacedWorldModelCollision, WorldModelCameraRegistration,
    WorldModelCameraSceneQuery, WorldModelExteriorPortalWindow, WorldModelRegistrationSelection,
    WorldModelVisibilityError, WorldSceneCameraFrame, WorldSceneDepthFrame,
};

use super::super::{
    MovementRootReference, ResidentTerrainMap, RuntimeMovementRegistrationError,
    RuntimeTerrainCoordinator, RuntimeWorldModelMovementOwner,
};
use super::{
    graphics::{WorldModelSceneGraphics, WorldModelSceneGroup},
    outdoor::{OutdoorSceneGroup, OutdoorSceneGroups},
};

/// Reuses camera traversal scratch and eligible root/group destinations each frame.
#[derive(Default)]
pub(in crate::application::terrain_coordinator::movement) struct WorldSceneAdmission {
    query: WorldModelCameraSceneQuery,
    graphics: WorldModelSceneGraphics,
    outdoor_groups: OutdoorSceneGroups,
    groups: HashSet<(RuntimeWorldModelMovementOwner, usize)>,
    /// Every 799310 group callback participates in later moving-root overlap.
    visible_bounds: HashMap<(RuntimeWorldModelMovementOwner, usize), MovementCollisionBounds>,
    /// 799F80's direct callbacks also accept exterior-registered units.
    overlap_groups: HashSet<(RuntimeWorldModelMovementOwner, usize)>,
    outdoor: Option<WorldSceneDepthFrame>,
    sky: super::sky::WorldSceneSky,
}

impl WorldSceneAdmission {
    /// 79A870 visits both camera roots, but only the primary opens outdoor lists.
    fn prepare(
        &mut self,
        active: &mut ResidentTerrainMap,
        camera: WorldCameraFrame,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        self.groups.clear();
        self.graphics.begin();
        self.outdoor_groups.clear();
        self.visible_bounds.clear();
        self.overlap_groups.clear();
        self.outdoor = None;
        self.sky.begin();
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
                if registration.secondary.is_none() {
                    self.sky.seed(
                        root.model(),
                        [Some(selected.group), selected.secondary_group],
                    );
                }
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
            self.sky.outdoors();
        }
        if let Some(window) = outdoor_window {
            let depth = WorldSceneDepthFrame::new(eye, target)?;
            self.outdoor = Some(depth);
            for index in 0..active.movement.roots.len() {
                let reference = active.movement.roots[index];
                // 792AD0 sends roots marked 0x400 to a separate ordered list.
                if !matches!(reference, MovementRootReference::Static(_)) {
                    continue;
                }
                let root = active.registration_root_mut(reference)?;
                let _ = self.queue_outdoor_root(root, scene, index, reference.owner(), depth)?;
            }
            if primary_owner.is_none() {
                // 792BD0 converts the moving list only for outdoor cameras.
                // Its first out-of-range entry stops the entire remaining list.
                for index in 0..active.movement.roots.len() {
                    let reference = active.movement.roots[index];
                    if !matches!(reference, MovementRootReference::GameObject(_)) {
                        continue;
                    }
                    let root = active.registration_root_mut(reference)?;
                    if self
                        .queue_outdoor_root(root, scene, index, reference.owner(), depth)?
                        .is_break()
                    {
                        break;
                    }
                }
            }
            // The native moving conversion joins existing static depth bins.
            // Visit complete bins only after both insertion passes finish.
            while let Some(entry) = self.outdoor_groups.next_group() {
                let reference = active.movement.roots[entry.root];
                let root = active.registration_root_mut(reference)?;
                self.record_outdoor_group(
                    root,
                    scene,
                    reference.owner(),
                    primary_owner,
                    entry.group,
                    window,
                )?;
            }
        }
        if let Some(primary) = primary_owner {
            // 799F80 follows both camera-root and ordinary outdoor passes,
            // even when the primary camera has no true-exterior portal.
            for index in 0..active.movement.roots.len() {
                let reference = active.movement.roots[index];
                if !matches!(reference, MovementRootReference::GameObject(_)) {
                    continue;
                }
                let root = active.registration_root_mut(reference)?;
                self.record_overlap_root(root, scene, reference.owner(), primary)?;
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
        self.sky.record_root(root.model(), &self.query);
        if registration.owner == primary {
            self.sky.record_primary(&self.query);
        }
        self.record_group_callbacks(root, camera, registration.owner, Some(primary))?;
        Ok(self.query.exterior_window())
    }

    /// 792AD0 depth-lists exterior MOGI groups before 79A160/7B3A10 traversal.
    /// The separate visitation pass preserves increasing depth and append order.
    fn queue_outdoor_root(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        root_index: usize,
        owner: RuntimeWorldModelMovementOwner,
        depth: WorldSceneDepthFrame,
    ) -> Result<ControlFlow<()>, RuntimeMovementRegistrationError> {
        let envelope = camera.enclosing_bounds();
        for (group, info) in root.model().group_info().iter().enumerate() {
            if info.flags() & 0x10008 == 0 {
                continue;
            }
            let bounds = root.scene_group_bounds(group)?;
            if !envelope.intersects(MovementCollisionBounds::new(bounds[0], bounds[1])?) {
                continue;
            }
            let Some(bin) = depth.depth_bin(bounds)? else {
                if matches!(owner, RuntimeWorldModelMovementOwner::GameObject { .. }) {
                    return Ok(ControlFlow::Break(()));
                }
                continue;
            };
            self.outdoor_groups.bins[usize::from(bin)].push(OutdoorSceneGroup {
                root: root_index,
                group,
            });
        }
        Ok(ControlFlow::Continue(()))
    }

    /// 79A160 enters one depth-listed group with the primary exterior window.
    fn record_outdoor_group(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        owner: RuntimeWorldModelMovementOwner,
        primary: Option<RuntimeWorldModelMovementOwner>,
        group: usize,
        screen_window: [f32; 4],
    ) -> Result<(), RuntimeMovementRegistrationError> {
        self.query
            .query_outdoor_group(root, camera, group, screen_window)?;
        self.record_group_callbacks(root, camera, owner, primary)?;
        let depth = camera.depth_frame()?;
        let frustum = camera.frustum_for_window(screen_window)?;
        let bounds = root.scene_group_bounds(group)?;
        if frustum.intersects_bounds(MovementCollisionBounds::new(bounds[0], bounds[1])?)
            && let Some(bin) = depth.depth_bin(bounds)?
        {
            self.graphics
                .record_outdoor_doodads(root, owner, group, bin, (depth, frustum));
        }
        Ok(())
    }

    /// 799F80 visits moving exterior entries in original root/group list order.
    /// Earlier portal callbacks can establish overlap for a later moving root.
    fn record_overlap_root(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        owner: RuntimeWorldModelMovementOwner,
        primary: RuntimeWorldModelMovementOwner,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        for (group, info) in root.model().group_info().iter().enumerate() {
            if info.flags() & 0x10008 == 0 {
                continue;
            }
            let [minimum, maximum] = root.scene_group_bounds(group)?;
            let bounds = MovementCollisionBounds::new(minimum, maximum)?;
            if !camera.enclosing_bounds().intersects(bounds)
                || !self.overlap_bounds_visible(camera, bounds)?
            {
                continue;
            }
            self.query
                .query_outdoor_group(root, camera, group, [0., 0., 1., 1.])?;
            self.record_group_callbacks(root, camera, owner, Some(primary))?;
            self.graphics.record_direct_doodads(owner, group);
            self.overlap_groups.insert((owner, group));
        }
        Ok(())
    }

    /// The indoor overlap pass uses the full viewport and bypasses overlap only
    /// after a true-exterior portal has enabled outdoor traversal (ADF59C >= 0).
    fn overlap_bounds_visible(
        &self,
        camera: WorldSceneCameraFrame,
        bounds: MovementCollisionBounds,
    ) -> Result<bool, RuntimeMovementRegistrationError> {
        if self.outdoor.is_none()
            && !self
                .visible_bounds
                .values()
                .any(|visible| visible.intersects(bounds))
        {
            return Ok(false);
        }
        Ok(camera
            .frustum_for_window([0., 0., 1., 1.])?
            .intersects_bounds(bounds))
    }

    /// Retains all visible bounds while applying 79A260's narrower unit gate.
    fn record_group_callbacks(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        owner: RuntimeWorldModelMovementOwner,
        primary: Option<RuntimeWorldModelMovementOwner>,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        let depth = camera.depth_frame()?;
        for &visit in self.query.visits() {
            let group = visit.group;
            let [minimum, maximum] = root.scene_group_bounds(group)?;
            let models_allowed =
                Some(owner) == primary || root.model().group_info()[group].flags() & 0x10008 == 0;
            self.graphics.record(
                root,
                owner,
                visit,
                models_allowed,
                depth.leading_depth([minimum, maximum])?,
            )?;
            self.visible_bounds.insert(
                (owner, group),
                MovementCollisionBounds::new(minimum, maximum)?,
            );
            if models_allowed {
                self.groups.insert((owner, group));
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
        if selection.primary().into_iter().flatten().any(|candidate| {
            self.overlap_groups
                .contains(&(candidate.owner(), candidate.hit().group_index()))
        }) {
            return Ok(true);
        }
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
    /// Supplies sky visibility after both camera roots and before GPU preparation.
    pub(in crate::application) fn world_model_sky_window(
        &self,
    ) -> Result<Option<solarity_rendering::WorldSkyWindow>, solarity_rendering::WorldCameraError>
    {
        self.active
            .as_ref()
            .map_or(Ok(None), |active| active.movement.scene.sky.window())
    }

    /// Distinguishes a closed sky bank from an open bank clipped offscreen.
    pub(in crate::application) fn has_world_model_sky_window(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.movement.scene.sky.has_window())
    }

    /// Borrows the last recursive camera-root MOSB selected for this frame.
    pub(in crate::application) fn world_model_skybox(&self) -> Option<&str> {
        self.active
            .as_ref()
            .and_then(|active| active.movement.scene.sky.skybox())
    }
    /// Supplies 79A160's exterior-group references in increasing depth order.
    pub(in crate::application) fn world_model_outdoor_doodads(
        &self,
    ) -> impl Iterator<
        Item = (
            WorldSceneDepthFrame,
            solarity_systems::WorldSceneFrustum,
            u8,
            RuntimeWorldModelMovementOwner,
            &[u16],
        ),
    > {
        self.active
            .as_ref()
            .into_iter()
            .flat_map(|active| active.movement.scene.graphics.outdoor_doodads())
    }
    /// Supplies native ordered WMO model callbacks after scene preparation.
    pub(in crate::application) fn visit_world_model_doodads(
        &self,
        visitor: impl FnMut(
            RuntimeWorldModelMovementOwner,
            &[u16],
            &[solarity_systems::WorldSceneFrustum],
            f32,
            bool,
        ),
    ) {
        if let Some(active) = &self.active {
            active.movement.scene.graphics.visit_doodads(visitor);
        }
    }
    /// Supplies the current ordered graphics groups after scene preparation.
    pub(in crate::application) fn world_model_scene_groups(&self) -> &[WorldModelSceneGroup] {
        self.active
            .as_ref()
            .map_or(&[], |active| active.movement.scene.graphics.groups())
    }

    /// Retains the bank restored by the final WMO group submitted this frame.
    pub(in crate::application) fn complete_world_model_scene(&mut self, last: Option<usize>) {
        if let Some(active) = &mut self.active {
            active.movement.scene.graphics.complete(last);
        }
    }

    /// Prepares graphics callbacks and unit destinations before dynamic updates.
    pub(in crate::application) fn prepare_world_scene(
        &mut self,
        camera: WorldCameraFrame,
    ) -> Result<(), RuntimeMovementRegistrationError> {
        let Some(active) = &mut self.active else {
            return Ok(());
        };
        let mut admission = std::mem::take(&mut active.movement.scene);
        let result = admission.prepare(active, camera);
        // Return retained storage even when resident geometry rejects the query.
        active.movement.scene = admission;
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
        active.movement.scene.admits_registration(selection, bounds)
    }
}

//! Camera-root group and exterior admission over the native portal pipeline.

use glam::Vec3;

use super::{
    WorldModelExteriorPortalWindow, WorldModelPortalProjector, WorldModelSceneFog,
    WorldModelSceneGroupVisit, WorldModelSceneVisibilityEvent, WorldModelVisibilityError,
    WorldModelVisibilityQuery, WorldSceneCameraFrame,
};
use crate::collision::{MovementCollisionBounds, PlacedWorldModelCollision};

/// Reusable camera-root group callbacks and exterior-portal admission.
#[derive(Default)]
pub struct WorldModelCameraSceneQuery {
    projector: WorldModelPortalProjector,
    visibility: WorldModelVisibilityQuery,
    groups: Vec<usize>,
    visits: Vec<WorldModelSceneGroupVisit>,
    exterior_window: Option<WorldModelExteriorPortalWindow>,
}

impl WorldModelCameraSceneQuery {
    /// Resolves 7AD1F0's camera-root groups and returns exterior admission.
    ///
    /// 79A870 clears exterior windows after visiting a secondary camera root.
    /// The primary root's 7AD1F0 traversal then decides whether outdoor depth
    /// lists run. Only accepted portal callbacks to groups masked by 0x10008
    /// reach 790AD0's exterior bank. Other 50148 callbacks affect a separate bank.
    /// The native constructor 9CE7E0 installs the recursion limit of ten.
    /// Group callbacks are retained in order, including the final 0x10000
    /// group-info bounds pass. Callers consume them before querying another root.
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
        self.groups.clear();
        self.visits.clear();
        self.exterior_window = None;
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
                if let WorldModelSceneVisibilityEvent::Group(visit) = event {
                    self.groups.push(visit.group);
                    self.visits.push(WorldModelSceneGroupVisit {
                        group: visit.group,
                        fog: if visit.indoor_fog {
                            WorldModelSceneFog::Indoor
                        } else {
                            WorldModelSceneFog::Outdoor
                        },
                        frustum: visit.frustum(camera, camera.frustum())?,
                    });
                }
                continue;
            };
            let reference = model.portal_references()[*reference];
            if model.group_info()[usize::from(reference.group_index())].flags() & 0x10008 == 0 {
                continue;
            }
            let portal = model.portals()[usize::from(reference.portal_index())];
            let start = usize::from(portal.vertex_start());
            let end = start + usize::from(portal.vertex_count());
            if let Some(window) = self.projector.project_exterior_polygon(
                &model.portal_vertices()[start..end],
                Vec3::from_array(portal.normal()),
                reference.side(),
                forward_plane,
                frame,
            )? {
                self.exterior_window = Some(match self.exterior_window {
                    Some(previous) => merge_exterior_windows(previous, window),
                    None => window,
                });
            }
        }
        // 7AD1F0 follows recursive visits with direct callbacks for 0x10000
        // MOGI groups inside the full world frustum. These do not add portals.
        for (group, info) in model.group_info().iter().enumerate() {
            if info.flags() & 0x10000 == 0 {
                continue;
            }
            let [minimum, maximum] = info.bounds().map(Vec3::from_array);
            let bounds =
                MovementCollisionBounds::new(minimum, maximum)?.transformed(root.transform)?;
            if camera.intersects_bounds(bounds) {
                self.groups.push(group);
                self.visits.push(WorldModelSceneGroupVisit {
                    group,
                    fog: WorldModelSceneFog::Inherited,
                    frustum: camera.frustum(),
                });
            }
        }
        Ok(self.exterior_window.is_some())
    }

    /// Resolves 7B3A10's entry into one group from an exterior scene window.
    ///
    /// The window uses normalized min-Y, min-X, max-Y, max-X coordinates.
    /// MOGI flag 0x10000 selects a direct callback; flag 8 enters 7AD350's
    /// outdoor-fog recursion after the cropped world AABB test. Projection
    /// retains the fixed full-camera planes throughout recursion. Callers own
    /// outdoor depth/overlap list admission; optional occlusion is disabled.
    ///
    /// # Errors
    /// Rejects invalid group indices, camera/root transforms, windows or bounds.
    pub fn query_outdoor_group(
        &mut self,
        root: &PlacedWorldModelCollision,
        camera: WorldSceneCameraFrame,
        group: usize,
        screen_window: [f32; 4],
    ) -> Result<&[usize], WorldModelVisibilityError> {
        self.groups.clear();
        self.visits.clear();
        self.exterior_window = None;
        let model = &root.model;
        let info = model
            .group_info()
            .get(group)
            .ok_or(WorldModelVisibilityError::InvalidGroup)?;
        let frustum = camera.frustum_for_window(screen_window)?;
        let [minimum, maximum] = info.bounds().map(Vec3::from_array);
        let bounds = MovementCollisionBounds::new(minimum, maximum)?.transformed(root.transform)?;
        if !frustum.intersects_bounds(bounds) {
            return Ok(&self.groups);
        }
        if info.flags() & 0x10000 != 0 {
            self.groups.push(group);
            self.visits.push(WorldModelSceneGroupVisit {
                group,
                fog: WorldModelSceneFog::Inherited,
                frustum,
            });
        } else if info.flags() & 8 != 0 {
            let frame = camera.for_root(root.transform, root.inverse_transform)?;
            let projected = self.projector.project(model, frame)?;
            // 7B3A10 retains the multiply/subtract until each float store.
            let window = screen_window.map(|value| (f64::from(value) * 2. - 1.) as f32);
            for visit in self.visibility.query_outdoor(
                model,
                frame.local_camera,
                group,
                10,
                window,
                projected,
            )? {
                self.groups.push(visit.group);
                self.visits.push(WorldModelSceneGroupVisit {
                    group: visit.group,
                    fog: if visit.indoor_fog {
                        WorldModelSceneFog::Indoor
                    } else {
                        WorldModelSceneFog::Outdoor
                    },
                    frustum: visit.frustum(camera, frustum)?,
                });
            }
        }
        Ok(&self.groups)
    }

    /// Returns 790AD0's merged true-exterior window after a camera-root query.
    /// The primary root's bank supplies 79A790's outdoor depth traversal window.
    #[must_use]
    pub const fn exterior_window(&self) -> Option<WorldModelExteriorPortalWindow> {
        self.exterior_window
    }

    /// Returns ordered group callbacks; read only after a successful root query.
    #[must_use]
    pub fn groups(&self) -> &[usize] {
        &self.groups
    }

    /// Returns every callback's exact clip and fog write in traversal order.
    /// Repeated groups must retain all their regions for graphics consumption.
    #[must_use]
    pub fn visits(&self) -> &[WorldModelSceneGroupVisit] {
        &self.visits
    }
}

/// 7905B0/78F2F0 merge exterior bounds and retain the greatest portal depth.
/// Equal values choose the incoming store, preserving native signed-zero ties.
fn merge_exterior_windows(
    previous: WorldModelExteriorPortalWindow,
    incoming: WorldModelExteriorPortalWindow,
) -> WorldModelExteriorPortalWindow {
    WorldModelExteriorPortalWindow {
        screen_window: std::array::from_fn(|axis| {
            let old = previous.screen_window[axis];
            let new = incoming.screen_window[axis];
            if (axis < 2 && old < new) || (axis >= 2 && old > new) {
                old
            } else {
                new
            }
        }),
        depth: if previous.depth > incoming.depth {
            previous.depth
        } else {
            incoming.depth
        },
    }
}

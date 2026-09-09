//! Unit-lifetime surface smoothing and the ordinary ground model-placement lane.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use glam::{Mat4, Vec3};
use solarity_asset::DecodedM2Model;
use solarity_ecs::WorldObjectIdentity;
use solarity_rendering::M2GroundNormal;

use super::{RuntimeTerrainFrameError, UnitAnimationBehavior, UnitAnimationScene};
use crate::application::model_playback::M2Playback;

/// Retained independently of animation/model replacements for the same unit.
pub(super) struct UnitGroundPose {
    target: Cell<Vec3>,
    scene_collision: Cell<bool>,
    normal: RefCell<M2GroundNormal>,
    last_scene_time_ms: Cell<Option<f32>>,
}

impl Default for UnitGroundPose {
    fn default() -> Self {
        Self {
            target: Cell::new(Vec3::Z),
            scene_collision: Cell::new(false),
            normal: RefCell::new(M2GroundNormal::default()),
            last_scene_time_ms: Cell::new(None),
        }
    }
}

impl UnitAnimationScene {
    /// Unit_C consumes and clears its prior scene callback bit after catch-up.
    pub fn take_scene_collision(&self, identity: WorldObjectIdentity) -> bool {
        retained_ground_pose(&mut self.ground_poses.borrow_mut(), identity)
            .scene_collision
            .replace(false)
    }

    /// The movement service publishes world-space normals before the scene tick.
    pub fn set_ground_normal(&self, identity: WorldObjectIdentity, normal: Vec3) {
        retained_ground_pose(&mut self.ground_poses.borrow_mut(), identity)
            .target
            .set(normal);
    }

    /// Gives a newly loaded model the movement/normal state already owned by its unit.
    pub(super) fn ground_pose(&self, identity: WorldObjectIdentity) -> Rc<UnitGroundPose> {
        Rc::clone(retained_ground_pose(
            &mut self.ground_poses.borrow_mut(),
            identity,
        ))
    }
}

/// Reusing a GUID must not carry the former entity lifetime's surface state.
fn retained_ground_pose(
    poses: &mut BTreeMap<u64, (WorldObjectIdentity, Rc<UnitGroundPose>)>,
    identity: WorldObjectIdentity,
) -> &Rc<UnitGroundPose> {
    let entry = poses
        .entry(identity.guid())
        .or_insert_with(|| (identity, Rc::new(UnitGroundPose::default())));
    if entry.0 != identity {
        *entry = (identity, Rc::new(UnitGroundPose::default()));
    }
    &entry.1
}

impl UnitAnimationBehavior {
    /// 793060 sets scene visitation before the later frustum/occlusion draw tests.
    pub fn admit_scene_collision(&self) {
        self.ground.scene_collision.set(true);
    }

    /// 71FD80 -> 7197D0 -> 82DD80 places the ordinary, unlinked unit body.
    /// Flight/banking and attachment transforms have their own placement lanes.
    pub fn ground_transform(
        &self,
        position: Vec3,
        scale: f32,
        scene_time_ms: f32,
        frame_seconds: f32,
    ) -> Result<Mat4, RuntimeTerrainFrameError> {
        self.ground_model_transform(
            position,
            scale,
            scene_time_ms,
            frame_seconds,
            &self.model,
            &self.playback.borrow(),
        )
    }

    /// GetModel selects the mount's flags and primary timer while the unit
    /// retains the shared normal and yaw. Rider timers never supply its weight.
    #[allow(clippy::too_many_arguments)]
    pub fn ground_model_transform(
        &self,
        position: Vec3,
        scale: f32,
        scene_time_ms: f32,
        frame_seconds: f32,
        model: &DecodedM2Model,
        playback: &M2Playback,
    ) -> Result<Mat4, RuntimeTerrainFrameError> {
        let mut normal = self.ground.normal.borrow_mut();
        if self.ground.last_scene_time_ms.get() != Some(scene_time_ms) {
            // 4F8D10 passes the scene context's +B14 frame duration. A unit
            // returning to the callback list must not smooth across its absence.
            normal
                .advance(self.ground.target.get(), frame_seconds)
                .map_err(|_| RuntimeTerrainFrameError::InvalidUnitM2Transform)?;
            self.ground.last_scene_time_ms.set(Some(scene_time_ms));
        }
        let weight = playback
            .script_timer
            .zip(model.animations().sequences().get(playback.sequence))
            .map_or(0.0, |(timer, sequence)| {
                timer.ground_alignment_weight(sequence.flags(), scene_time_ms as u32)
            });
        normal
            .transform(
                position,
                self.body_pose().placement_yaw,
                scale,
                model.flags(),
                weight,
            )
            .map_err(|_| RuntimeTerrainFrameError::InvalidUnitM2Transform)
    }

    /// 71FD80 dispatches active swim/fly banking outside the ground-placement lane.
    pub fn uses_ground_placement(&self) -> bool {
        self.input.get().movement_flags & 0x0220_0000 == 0
    }
}

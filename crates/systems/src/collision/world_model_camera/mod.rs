//! Camera interior registration, distinct from Unit_C's floor/fallback banks.

mod portal;

use crate::collision::{
    PlacedWorldModelCollision, WorldModelCollisionError, WorldModelRegistrationKind,
    movement_collection::transform_point, world_model_registration::segment_intersects_box,
};
use glam::Vec3;

/// A selected camera group and the owning scene's stable root identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldModelCameraRegistration<Owner> {
    /// Owning admitted root lifetime.
    pub owner: Owner,
    /// The first camera group; 790920 uses only this group's liquid volume.
    pub group: usize,
    /// Interior neighbor retained when the downward ray selects a portal.
    pub secondary_group: Option<usize>,
}

/// Native 7D59B0's independent static/transformed camera ray banks.
pub struct WorldModelCameraRegistrationQuery<Owner> {
    start: Vec3,
    end: Vec3,
    maximum: [f32; 2],
    selected: [Option<WorldModelCameraRegistration<Owner>>; 2],
}

impl<Owner: Copy> WorldModelCameraRegistrationQuery<Owner> {
    /// Begins after terrain limits the 1760-unit downward camera ray (795D40).
    ///
    /// # Errors
    /// Rejects invalid endpoints or maximum fraction.
    pub fn new(start: Vec3, end: Vec3, maximum: f32) -> Result<Self, WorldModelCollisionError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        if !maximum.is_finite() || maximum < 0.0 {
            return Err(WorldModelCollisionError::InvalidMaximumFraction);
        }
        Ok(Self {
            start,
            end,
            maximum: [maximum; 2],
            selected: [None; 2],
        })
    }

    /// Visits an eligible placed root in native scene order.
    /// The scene excludes roots carrying native flag 0x20 before this call.
    ///
    /// # Errors
    /// Returns invalid BSP or transformed-point errors from the admitted root.
    pub fn probe_root(
        &mut self,
        owner: Owner,
        kind: WorldModelRegistrationKind,
        placement: &mut PlacedWorldModelCollision,
    ) -> Result<(), WorldModelCollisionError> {
        let bank = usize::from(kind == WorldModelRegistrationKind::Transformed);
        if !segment_intersects_box(placement.root_bounds, self.start, self.end) {
            return Ok(());
        }
        let start = transform_point(placement.inverse_transform, self.start);
        let end = transform_point(placement.inverse_transform, self.end);
        if !start.is_finite() || !end.is_finite() {
            return Err(WorldModelCollisionError::NonFiniteSegment);
        }
        if !segment_intersects_box(placement.model.bounds().map(Vec3::from_array), start, end) {
            return Ok(());
        }
        let mut hit = None;
        for index in 0..placement.model.groups().len() {
            let group = &placement.model.groups()[index];
            if group.flags() & 0x410080 != 0
                || !segment_intersects_box(
                    placement.model.group_info()[index]
                        .bounds()
                        .map(Vec3::from_array),
                    start,
                    end,
                )
            {
                continue;
            }
            if let Some(floor) =
                placement.probe_camera_group_floor(index, start, end, self.maximum[bank])?
            {
                self.maximum[bank] = floor.fraction();
                hit = Some((
                    index,
                    None,
                    placement.model.groups()[index].flags() & 8 == 0,
                ));
            }
        }
        if let Some(portal) = portal::probe(&placement.model, start, end)
            && f64::from(portal.fraction()) - f64::from(self.maximum[bank]) < f64::from(0.0001_f32)
        {
            self.maximum[bank] = portal.fraction();
            let group = portal.source_group();
            let neighbor = portal.destination_group();
            let secondary =
                (placement.model.group_info()[neighbor].flags() & 8 == 0).then_some(neighbor);
            hit = Some((
                group,
                secondary,
                placement.model.group_info()[group].flags() & 8 == 0,
            ));
        }
        if let Some((group, secondary_group, interior)) = hit {
            // An exterior hit also clears an earlier root in this bank.
            self.selected[bank] = interior.then_some(WorldModelCameraRegistration {
                owner,
                group,
                secondary_group,
            });
        }
        Ok(())
    }

    /// Camera registration prefers the static bank (unlike unit registration).
    #[must_use]
    pub fn finish(self) -> Option<WorldModelCameraRegistration<Owner>> {
        self.selected[0].or(self.selected[1])
    }
}

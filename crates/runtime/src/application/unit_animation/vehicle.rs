//! Vehicle_C's sixteen passenger owners and model-key completion routing.

use super::*;
use std::rc::Weak;

#[cfg(test)]
#[path = "../../../tests/application/vehicle_owner_table.rs"]
mod tests;

#[derive(Default)]
pub(super) struct UnitVehicleAnimations {
    owned: u64,
    owners: [Option<VehicleAnimationOwner>; 16],
}

struct VehicleAnimationOwner {
    guid: u64,
    key: u8,
    retired: Weak<Cell<bool>>,
}

/// 756D10 and 756F80 normalize unsigned selectors, including -1.
fn normalized_key(key: i32) -> u8 {
    if key as u32 > 34 { 26 } else { key as u8 }
}

impl UnitVehicleAnimations {
    /// 756CD0 deliberately excludes a literal 26 before normalization.
    fn controls(&self, key: i32) -> bool {
        key != 26 && self.owned & (1 << normalized_key(key)) != 0
    }

    fn register(&mut self, guid: u64, key: i32, retired: &Rc<Cell<bool>>) {
        // 7489C0 ignores registration failure after a successful model request.
        if let Some(slot) = self.owners.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(VehicleAnimationOwner {
                guid,
                key: normalized_key(key),
                retired: Rc::downgrade(retired),
            });
        }
    }

    fn interrupt(&mut self, key: i32) {
        self.finish(key, true);
    }

    fn finish(&mut self, key: i32, interrupted: bool) -> bool {
        let key = normalized_key(key);
        let active = self.prune(key);
        if interrupted || !active {
            self.owned &= !(1 << key);
        }
        active
    }

    /// 747980 stays active while GUID lookup can still resolve the passenger.
    fn prune(&mut self, key: u8) -> bool {
        let mut active = false;
        for slot in &mut self.owners {
            if let Some(owner) = slot.as_ref()
                && owner.key == key
            {
                if owner
                    .retired
                    .upgrade()
                    .is_some_and(|retired| !retired.get())
                {
                    active = true;
                } else {
                    *slot = None;
                }
            }
        }
        active
    }

    fn remove(&mut self, guid: u64, key: i32) -> bool {
        for slot in &mut self.owners {
            if slot.as_ref().is_some_and(|owner| owner.guid == guid) {
                *slot = None;
            }
        }
        let key = normalized_key(key);
        self.owned & (1 << key) != 0 && !self.owners.iter().flatten().any(|owner| owner.key == key)
    }
}

impl UnitAnimationBehavior {
    pub(super) fn vehicle_controls_body_key(&self, key: i32) -> bool {
        self.vehicle_animations
            .borrow()
            .as_ref()
            .is_some_and(|state| state.borrow().controls(key))
    }

    pub(super) fn interrupt_vehicle_animation(&self, key: i32) {
        if let Some(state) = self.vehicle_animations.borrow().as_ref()
            && state.borrow().controls(key)
        {
            state.borrow_mut().interrupt(key);
        }
    }

    /// 756F80 addresses D4 (mount first), then 756D10 records the passenger.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn start_vehicle_ride_animation(
        &self,
        guid: u64,
        retired: &Rc<Cell<bool>>,
        key: i32,
        animation: i32,
        now: u32,
        random: &mut CrtRand,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        if animation as u32 >= 506 {
            return Ok(false);
        }
        let mount = self.mount_model.borrow();
        let (model, playback) = mount
            .as_ref()
            .map_or((&*self.model, &self.playback), |mount| {
                (&*mount.model, &mount.playback)
            });
        let key = normalized_key(key);
        self.apply_vehicle_animation(
            model,
            &mut playback.borrow_mut(),
            key,
            animation as u16,
            0,
            now,
            M2SequenceStartPhase::BeforeSceneUpdate,
            true,
            random,
        )?;
        let state = Rc::clone(
            self.vehicle_animations
                .borrow_mut()
                .get_or_insert_with(|| Rc::new(RefCell::new(UnitVehicleAnimations::default()))),
        );
        let mut state = state.borrow_mut();
        state.owned |= 1 << key;
        state.register(guid, i32::from(key), retired);
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn apply_vehicle_animation(
        &self,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        key: u8,
        animation: u16,
        overdue: u32,
        now: u32,
        phase: M2SequenceStartPhase,
        interrupt: bool,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(resolved) = model
            .animations()
            .resolve_model_animation(&self.animations, u32::from(animation))
        else {
            return Ok(());
        };
        if interrupt
            && let Some((previous, callback_key)) =
                playback.model_key_playback(model, i32::from(key))
            && previous.has_pending_sequence_callback()
        {
            self.interrupt_vehicle_animation(callback_key);
            if self.mount_model.borrow().is_none() {
                self.complete_passenger_animation(callback_key);
            } else if matches!(previous.animation_id, 39 | 187) {
                self.landing.set(false);
            }
        }
        if key == 26 {
            playback.apply_resolved_model_sequence_variation(
                model,
                resolved.animation_id(),
                None,
                resolved.mode(),
                1.,
                overdue as i32,
                now,
                phase,
                true,
                random,
            )?;
        } else {
            playback.apply_bone_sequence(
                model,
                u16::from(key),
                resolved.animation_id(),
                None,
                resolved.mode(),
                1.,
                overdue as i32,
                now,
                phase,
                random,
            )?;
        }
        Ok(())
    }

    /// 7577E0 removes all records for this GUID, but releases only this key.
    pub(in crate::application) fn release_vehicle_ride_animation(
        &self,
        guid: u64,
        key: i32,
        now: u32,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let Some(state) = self.vehicle_animations.borrow().as_ref().cloned() else {
            return Ok(());
        };
        if !state.borrow_mut().remove(guid, key) {
            return Ok(());
        }
        let mount = self.mount_model.borrow();
        let (model, playback) = mount
            .as_ref()
            .map_or((&*self.model, &self.playback), |mount| {
                (&*mount.model, &mount.playback)
            });
        let mut playback = playback.borrow_mut();
        if self.release_vehicle_key(model, &mut playback, normalized_key(key), now) {
            self.resume_vehicle_animation(
                model,
                &mut playback,
                now,
                M2SequenceStartPhase::BeforeSceneUpdate,
                random,
            )?;
        }
        Ok(())
    }

    fn release_vehicle_key(
        &self,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        key: u8,
        now: u32,
    ) -> bool {
        if let Some(state) = self.vehicle_animations.borrow().as_ref() {
            state.borrow_mut().interrupt(i32::from(key));
        }
        let bone = (key != 26)
            .then(|| model.animations().key_bone(u16::from(key)))
            .flatten();
        if bone.is_some_and(|bone| bone.parent().is_some()) {
            playback.clear_bone_sequence(model, u16::from(key), true, now);
            false
        } else {
            // 735A60 cannot clear a root; 757420 resumes the ordinary unit.
            true
        }
    }

    fn resume_vehicle_animation(
        &self,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        now: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let input = self.input.get();
        if self.mount_model.borrow().is_some() {
            let request = self.request_for_model(
                UnitAnimationInput {
                    mounted: false,
                    ..input
                },
                &self.playback.borrow(),
                model,
            );
            if let Some(animation) = request
                && let Some(resolved) = self.commit_mount_sequence(
                    model,
                    playback,
                    animation.into(),
                    input,
                    now,
                    phase,
                    random,
                )?
            {
                self.commit_mounted_body(
                    &mut self.playback.borrow_mut(),
                    resolved,
                    input,
                    now,
                    phase,
                    random,
                )?;
            }
        } else if let Some(animation) = self.request(input, playback) {
            self.select(playback, animation.into(), input, now, phase, random)?;
        }
        Ok(())
    }

    /// Returns true when 73C090/73C140 dispatches to Vehicle_C rather than Unit_C.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) fn complete_vehicle_animation(
        &self,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        key: i32,
        animation: u16,
        overdue: u32,
        random: &mut CrtRand,
    ) -> Result<bool, RuntimeTerrainFrameError> {
        let Some(state) = self.vehicle_animations.borrow().as_ref().cloned() else {
            return Ok(false);
        };
        let mut state = state.borrow_mut();
        if !state.controls(key) {
            return Ok(false);
        }
        let active = state.finish(key, false);
        let key = normalized_key(key);
        drop(state);
        if active {
            // The synchronous interruption caused by this replay is suppressed
            // by native CA1608. Do not feed it back through interrupt().
            self.apply_vehicle_animation(
                model,
                playback,
                key,
                animation,
                overdue,
                playback.scene_time_ms,
                M2SequenceStartPhase::DuringSceneUpdate,
                false,
                random,
            )?;
        } else {
            let now = playback.scene_time_ms;
            if self.release_vehicle_key(model, playback, key, now) {
                self.resume_vehicle_animation(
                    model,
                    playback,
                    now,
                    M2SequenceStartPhase::DuringSceneUpdate,
                    random,
                )?;
            }
        }
        Ok(true)
    }
}

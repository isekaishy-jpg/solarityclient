//! Unit-owned mount requests and the separate 73BFF0 completion path.

use super::*;

#[derive(Clone)]
pub(super) struct UnitMountModel {
    pub model: Arc<DecodedM2Model>,
    pub playback: Rc<RefCell<M2Playback>>,
}

impl UnitAnimationBehavior {
    pub(super) fn mount_sequence_request(
        &self,
        animation: u16,
        body: &M2Playback,
        model: &DecodedM2Model,
        mount: &M2Playback,
    ) -> UnitSequenceRequest {
        // 71DBC0 checks the requested corpse behavior, 717260 checks the
        // BODY's current death family, then 826870 queries the MOUNT root.
        let variation = (matches!(
            self.animation_behavior(animation),
            6 | 132 | 467 | 468 | 472
        ) && matches!(self.behavior(body), 1 | 6 | 131 | 132 | 466..=468 | 472))
        .then(|| model.animations().model_variation_ordinal(mount.sequence))
        .flatten();
        UnitSequenceRequest {
            animation,
            variation,
            resolve_unit_tier: true,
        }
    }

    pub fn bind_mount(&self, model: &Arc<DecodedM2Model>, playback: Rc<RefCell<M2Playback>>) {
        *self.mount_model.borrow_mut() = Some(UnitMountModel {
            model: Arc::clone(model),
            playback,
        });
        // Model readiness re-evaluates the current unit request even when no
        // movement field changed while this independent model was loading.
        if self.pending.borrow().is_empty() {
            self.pending.borrow_mut().push_back(PendingUnitAnimation {
                input: self.input.get(),
                event: UnitMovementAnimationEventKind::Changed,
            });
        }
    }

    pub(super) fn mount_request(
        &self,
        pending: PendingUnitAnimation,
        body: &M2Playback,
        model: &DecodedM2Model,
    ) -> Option<u16> {
        let input = pending.input;
        let ordinary = || {
            self.transition_request_for_model(
                UnitAnimationInput {
                    mounted: false,
                    ..input
                },
                body,
                model,
            )
        };
        match pending.event {
            UnitMovementAnimationEventKind::Jump if !input.dead() => Some(37),
            UnitMovementAnimationEventKind::Land {
                previous_flags,
                forced,
                slow,
            } if !input.dead() => match resolve_unit_landing_animation(
                previous_flags,
                input.movement_flags,
                forced,
                slow,
            ) {
                UnitMovementAnimationDecision::Select(animation) => Some(animation),
                UnitMovementAnimationDecision::Retain => None,
                UnitMovementAnimationDecision::Continue => ordinary(),
            },
            // 73B140 sends wound behaviors to the body's 736640 adapter,
            // bypassing the ordinary 7385C0 mount dispatch altogether.
            UnitMovementAnimationEventKind::VisualKit { animation, .. }
                if matches!(self.animation_behavior(animation), 8..=10) =>
            {
                None
            }
            UnitMovementAnimationEventKind::VisualKit { animation, .. } => Some(animation),
            _ => ordinary(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_mount_sequence(
        &self,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        request: UnitSequenceRequest,
        input: UnitAnimationInput,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<Option<UnitSequenceRequest>, RuntimeTerrainFrameError> {
        // 7385C0 rejects non-death requests before model/tier resolution.
        if input.dead()
            && !matches!(
                self.animation_behavior(request.animation),
                1 | 6 | 131 | 132 | 466..=468 | 472,
            )
        {
            return Ok(None);
        }
        if model.animations().sequences().is_empty() || model.animations().bones().is_empty() {
            return Ok(None);
        }
        let Some(resolved) = resolve_unit_model_animation(
            &self.animations,
            UnitLocomotionAnimation::new(request.animation),
            input.tier,
            |id| model.animations().available_variation_count(id).is_some(),
        ) else {
            return Err(RuntimeTerrainFrameError::M2AnimationSelection {
                model: model.path().clone(),
                animation_id: request.animation,
            });
        };
        let animation = resolved.animation_id();
        let body_request = UnitSequenceRequest {
            animation,
            resolve_unit_tier: false,
            ..request
        };
        if !mount_animation_behavior(self.animation_behavior(animation)) {
            return Ok(Some(body_request));
        }
        let timing = model_sequence_timing(model, playback, animation, input, scene_time_ms);
        let interrupted = playback
            .has_pending_sequence_callback()
            .then_some(playback.animation_id);
        let selected = playback.select_mount_animation(
            model,
            animation,
            request.variation,
            timing,
            scene_time_ms as f32,
            phase,
            random,
        )?;
        if selected && interrupted.is_some() {
            self.interrupt_vehicle_animation(-1);
        }
        if selected && interrupted.is_some_and(|id| matches!(id, 39 | 187)) {
            // 73BFF0 tests the interrupted raw ID, before any DBC conversion.
            self.landing.set(false);
        }
        Ok(Some(body_request))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_mounted_body(
        &self,
        playback: &mut M2Playback,
        request: UnitSequenceRequest,
        input: UnitAnimationInput,
        scene_time_ms: u32,
        phase: M2SequenceStartPhase,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let behavior = self.animation_behavior(request.animation);
        if mounted_full_body_behavior(behavior) {
            // 738CF3 forces a body request even while the mount callback has
            // temporarily disabled ordinary body writes. Resolution already
            // used the mount; the body now applies its own CM2Model fallback.
            return self.select(playback, request, input, scene_time_ms, phase, random);
        }
        if !mount_animation_behavior(behavior)
            && mounted_upper_animation_behavior(behavior)
            && let Some(key) = self.upper_body_key()
        {
            self.commit_sequence(
                playback,
                request,
                input,
                scene_time_ms,
                phase,
                Some(key),
                None,
                random,
            )?;
        }
        self.select(playback, 91.into(), input, scene_time_ms, phase, random)
    }

    pub fn complete_mount_animation(
        &self,
        model: &DecodedM2Model,
        playback: &mut M2Playback,
        key: i32,
        animation: u16,
        random: &mut CrtRand,
    ) -> Result<(), RuntimeTerrainFrameError> {
        let input = self.input.get();
        let behavior = self.animation_behavior(animation);
        // Normal completion enters 73B510, whose landing switch uses behavior.
        // The mount wrapper disables ordinary body writes; death still forces
        // the body through the later 738CF3 override.
        if behavior == 39 {
            self.landing.set(false);
        }
        let completion =
            match resolve_unit_movement_animation_completion(behavior, input.movement_flags) {
                UnitMovementAnimationDecision::Select(animation) => {
                    UnitPrimaryAnimationCompletion::Select(animation)
                }
                _ => resolve_unit_primary_animation_completion(
                    behavior,
                    input.stand,
                    input.dead(),
                    false,
                ),
            };
        let request = match completion {
            UnitPrimaryAnimationCompletion::Retain => return Ok(()),
            UnitPrimaryAnimationCompletion::Continue => self
                .request_for_model(
                    UnitAnimationInput {
                        mounted: false,
                        ..input
                    },
                    &self.playback.borrow(),
                    model,
                )
                .map(UnitSequenceRequest::from),
            UnitPrimaryAnimationCompletion::Select(animation) => Some(animation.into()),
            UnitPrimaryAnimationCompletion::SelectWithCurrentVariation(animation) => {
                if animation == 472 {
                    Some(self.mount_sequence_request(
                        animation,
                        &self.playback.borrow(),
                        model,
                        playback,
                    ))
                } else {
                    let slot = u16::try_from(key)
                        .ok()
                        .and_then(|key| playback.bone_playback(key))
                        .unwrap_or(playback);
                    Some(UnitSequenceRequest {
                        animation,
                        variation: model.animations().model_variation_ordinal(slot.sequence),
                        resolve_unit_tier: false,
                    })
                }
            }
        };
        let Some(request) = request else {
            return Ok(());
        };
        let now = playback.scene_time_ms;
        if request.resolve_unit_tier {
            if let Some(resolved) = self.commit_mount_sequence(
                model,
                playback,
                request,
                input,
                now,
                M2SequenceStartPhase::DuringSceneUpdate,
                random,
            )? && mounted_full_body_behavior(self.animation_behavior(resolved.animation))
            {
                self.commit_mounted_body(
                    &mut self.playback.borrow_mut(),
                    resolved,
                    input,
                    now,
                    M2SequenceStartPhase::DuringSceneUpdate,
                    random,
                )?;
            }
            return Ok(());
        }
        // Corpse completion submits directly to this model/key and preserves
        // the outgoing variation ordinal, including held fallback endpoints.
        let Some(resolved) = model
            .animations()
            .resolve_model_animation(&self.animations, u32::from(request.animation))
        else {
            return Ok(());
        };
        if let Ok(key) = u16::try_from(key) {
            playback.apply_bone_sequence(
                model,
                key,
                resolved.animation_id(),
                request.variation,
                resolved.mode(),
                1.,
                0,
                now,
                M2SequenceStartPhase::DuringSceneUpdate,
                random,
            )?;
        } else {
            playback.apply_resolved_model_sequence_variation(
                model,
                resolved.animation_id(),
                request.variation,
                resolved.mode(),
                1.,
                0,
                now,
                M2SequenceStartPhase::DuringSceneUpdate,
                true,
                random,
            )?;
        }
        Ok(())
    }
}

/// 71D6B0 tests the behavior of the already model/tier-resolved animation ID.
fn mount_animation_behavior(behavior: u16) -> bool {
    matches!(behavior,
        0 | 1 | 3..=6 | 8..=13 | 37..=45 | 92..=95 | 119 | 120 | 127 | 131 | 132 | 135 | 143 | 187 | 193
    )
}

/// 71D800 admits these resolved behaviors on the mounted body's upper slot.
fn mounted_upper_animation_behavior(behavior: u16) -> bool {
    matches!(behavior,
        2 | 8..=10 | 14..=36 | 46..=49 | 51..=74 | 76..=78 | 80..=90
        | 105..=113 | 117 | 118 | 122..=125 | 128..=130 | 133 | 134
        | 136..=138 | 153..=156 | 185 | 186 | 195 | 213..=222 | 225
    )
}

/// 71D550 and 71DDE0 override ordinary body admission in 738CF3.
fn mounted_full_body_behavior(behavior: u16) -> bool {
    matches!(
        behavior,
        1 | 6 | 57 | 58 | 118 | 131 | 132 | 466..=468 | 472
    )
}

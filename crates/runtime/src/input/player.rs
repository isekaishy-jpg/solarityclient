//! Held controls and active-axis arbitration from native InputControl.cpp.

use solarity_network::WorldMovementKind as Movement;
use solarity_ui::{UiMovementAction, UiMovementControl};

/// Unit/vehicle admission resolved by the movement owner for this command.
#[derive(Clone, Copy)]
pub(crate) struct PlayerInputAdmission {
    pub translation: bool,
    pub turning: bool,
    pub forced_forward: bool,
    pub yaw_during_mouselook: bool,
    pub movement_flags: u32,
    pub secondary_flags: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlayerInputEffect {
    Movement(Movement),
    Jump,
    ToggleRun,
    SitStand,
}

/// Held bits survive admission failures; active bits record emitted axis starts.
#[derive(Default)]
pub(crate) struct PlayerInputState {
    bits: u32,
}

impl PlayerInputState {
    pub(crate) fn mouse_free_look(&self) -> bool {
        self.bits & 3 != 0
    }

    pub(crate) fn mouse_turning(&self) -> bool {
        self.bits & 1 != 0
    }

    pub(crate) fn paired_mouse_buttons(&self) -> bool {
        self.bits & 3 == 3
    }

    pub(crate) fn clear_active_turn(&mut self) {
        // Camera steering supersedes the keyboard yaw axis (5FB260).
        self.bits &= !0x40000;
    }

    /// Ordinary held edges still enter InputControl when no mover is selected;
    /// native 5FBBC0 then returns without resolving active axes.
    pub(crate) fn record_without_mover(&mut self, action: UiMovementAction) {
        if let UiMovementAction::Hold { control, pressed } = action {
            self.hold(control_bit(control), pressed);
        } else if matches!(action, UiMovementAction::CameraOrbitStop { .. }) {
            self.hold(2, false);
        }
    }

    pub(crate) fn apply(
        &mut self,
        action: UiMovementAction,
        admission: PlayerInputAdmission,
        emit: &mut impl FnMut(PlayerInputEffect),
    ) {
        use UiMovementAction as Action;
        match action {
            Action::CameraZoom { .. } => {}
            Action::CameraOrbitStop { .. } => {
                if self.hold(2, false) {
                    self.resolve(admission, emit);
                }
            }
            Action::Hold { control, pressed } => {
                if !self.hold(control_bit(control), pressed) {
                    return;
                }
                self.resolve(admission, emit);
            }
            Action::ToggleAutoRun => {
                self.bits ^= 0x1000;
                self.resolve(admission, emit);
            }
            Action::JumpOrAscend => {
                if self.hold(0x2000, true) {
                    self.resolve(admission, emit);
                }
                // The wrapper requests jump on every invocation, independently
                // of whether its held-ascent bit changed. The unit owns gates.
                if admission.movement_flags & 0x0220_0000 == 0 {
                    emit(PlayerInputEffect::Jump);
                }
            }
            Action::SitStandOrDescend => {
                if self.hold(0x4000, true) {
                    self.resolve(admission, emit);
                }
                if admission.movement_flags & 0x0200_0000 == 0 {
                    emit(PlayerInputEffect::SitStand);
                }
            }
            Action::ToggleRun => emit(PlayerInputEffect::ToggleRun),
        }
    }

    fn hold(&mut self, bit: u32, pressed: bool) -> bool {
        if (self.bits & bit != 0) == pressed {
            return false;
        }
        if pressed {
            self.bits |= bit;
            if bit & 0x30 != 0 || (bit & 3 != 0 && self.bits & 3 == 3) {
                self.bits &= !0x1000;
            }
        } else {
            self.bits &= !bit;
        }
        true
    }

    pub(crate) fn resolve(
        &mut self,
        admission: PlayerInputAdmission,
        emit: &mut impl FnMut(PlayerInputEffect),
    ) {
        let held = self.bits;
        let has = |mask| i32::from(held & mask != 0);
        let mouselook = held & 0x0200_0001 != 0;
        let flags = admission.movement_flags;
        if admission.translation {
            let forward = if admission.forced_forward {
                1
            } else {
                has(0x1000) + has(0x10) + i32::from(held & 3 == 3) - has(0x20)
            };
            self.axis(
                forward,
                0x10000,
                (forward > 0 && flags & 2 != 0) || (forward < 0 && flags & 1 != 0),
                [
                    Movement::StartForward,
                    Movement::StartBackward,
                    Movement::Stop,
                ],
                emit,
            );
            let strafe = has(0x40) - has(0x80)
                + if mouselook {
                    has(0x100) - has(0x200)
                } else {
                    0
                };
            self.axis(
                strafe,
                0x20000,
                false,
                [
                    Movement::StartStrafeLeft,
                    Movement::StartStrafeRight,
                    Movement::StopStrafe,
                ],
                emit,
            );
            if flags & 0x0220_0000 == 0 {
                self.bits &= !0x100000;
            } else {
                let ascent = has(0x2000) - has(0x4000);
                let pitch = admission.secondary_flags & 0x10 != 0;
                let reverse = if pitch {
                    (ascent > 0 && flags & 0x80 != 0) || (ascent < 0 && flags & 0x40 != 0)
                } else {
                    (ascent > 0 && flags & 0x800000 != 0) || (ascent < 0 && flags & 0x400000 != 0)
                };
                let events = if pitch {
                    [
                        Movement::StartPitchUp,
                        Movement::StartPitchDown,
                        Movement::StopPitch,
                    ]
                } else {
                    [
                        Movement::StartAscend,
                        Movement::StartDescend,
                        Movement::StopAscend,
                    ]
                };
                self.axis(ascent, 0x100000, reverse, events, emit);
            }
        } else {
            self.axis(
                0,
                0x10000,
                false,
                [
                    Movement::StartForward,
                    Movement::StartBackward,
                    Movement::Stop,
                ],
                emit,
            );
            self.axis(
                0,
                0x20000,
                false,
                [
                    Movement::StartStrafeLeft,
                    Movement::StartStrafeRight,
                    Movement::StopStrafe,
                ],
                emit,
            );
            self.bits &= !0x1000;
            self.axis(
                0,
                0x100000,
                false,
                [
                    Movement::StartAscend,
                    Movement::StartDescend,
                    Movement::StopAscend,
                ],
                emit,
            );
        }
        if admission.turning {
            let turn = if !mouselook || admission.yaw_during_mouselook {
                has(0x100) - has(0x200)
            } else {
                0
            };
            self.axis(
                turn,
                0x40000,
                false,
                [
                    Movement::StartTurnLeft,
                    Movement::StartTurnRight,
                    Movement::StopTurn,
                ],
                emit,
            );
            if (flags & 0x0220_0000 != 0 || admission.secondary_flags & 0x20 != 0) && !mouselook {
                self.axis(
                    has(0x400) - has(0x800),
                    0x80000,
                    false,
                    [
                        Movement::StartPitchUp,
                        Movement::StartPitchDown,
                        Movement::StopPitch,
                    ],
                    emit,
                );
            }
        } else {
            self.axis(
                0,
                0x40000,
                false,
                [
                    Movement::StartTurnLeft,
                    Movement::StartTurnRight,
                    Movement::StopTurn,
                ],
                emit,
            );
            if flags & 0x0220_0000 != 0 {
                self.axis(
                    0,
                    0x80000,
                    false,
                    [
                        Movement::StartPitchUp,
                        Movement::StartPitchDown,
                        Movement::StopPitch,
                    ],
                    emit,
                );
            }
            self.bits &= !0x80000;
        }
    }

    fn axis(
        &mut self,
        value: i32,
        active: u32,
        reverse: bool,
        events: [Movement; 3],
        emit: &mut impl FnMut(PlayerInputEffect),
    ) {
        if value == 0 {
            if self.bits & active != 0 {
                emit(PlayerInputEffect::Movement(events[2]));
                self.bits &= !active;
            }
        } else if self.bits & active == 0 || reverse {
            emit(PlayerInputEffect::Movement(events[usize::from(value < 0)]));
            self.bits |= active;
        }
    }
}

const fn control_bit(control: UiMovementControl) -> u32 {
    use UiMovementControl as Control;
    match control {
        Control::Forward => 0x10,
        Control::Backward => 0x20,
        Control::StrafeLeft => 0x40,
        Control::StrafeRight => 0x80,
        Control::TurnLeft => 0x100,
        Control::TurnRight => 0x200,
        Control::PitchUp => 0x400,
        Control::PitchDown => 0x800,
        Control::Ascend => 0x2000,
        Control::Descend => 0x4000,
        Control::TurnOrAction | Control::Mouselook => 1,
        Control::CameraOrSelectOrMove => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_axes_match_original_native_resolvers() -> Result<(), Box<dyn std::error::Error>> {
        for (index, line) in include_str!("../../tests/fixtures/player-input-native.txt")
            .lines()
            .enumerate()
            .skip(1)
        {
            let tokens: Vec<_> = line
                .split_whitespace()
                .map(|v| u32::from_str_radix(v, 16))
                .collect::<Result<_, _>>()?;
            let mut state = PlayerInputState { bits: tokens[0] };
            let mut events = Vec::new();
            state.resolve(
                PlayerInputAdmission {
                    translation: true,
                    turning: true,
                    forced_forward: tokens[2] != 0,
                    yaw_during_mouselook: tokens[3] != 0,
                    movement_flags: tokens[1],
                    secondary_flags: 0,
                },
                &mut |effect| {
                    let PlayerInputEffect::Movement(event) = effect else {
                        panic!("unexpected effect");
                    };
                    events.push(event as u32);
                },
            );
            assert_eq!(state.bits, tokens[4], "case {index}: {line}");
            assert_eq!(events, tokens[5..], "case {index}: {line}");
        }
        Ok(())
    }
}

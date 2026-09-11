//! Authored passenger animation choices and per-slot completion (747B20/7484E0).

use super::VehiclePassengerPhase as Phase;

/// Seat animation inputs retained independently of an individual model instance.
#[derive(Clone, Copy, Debug)]
pub struct VehiclePassengerAnimationInput {
    /// Current passenger controller phase.
    pub phase: Phase,
    /// VehicleSeat flags at row +4.
    pub flags: u32,
    /// Controller flags; bits 2 and 4 retain primary and secondary completion.
    pub completed: u32,
    /// Unit movement flag +7D0 bit 0x40 changes the exit-animation eligibility.
    pub special_exit: bool,
    /// Entry initial/loop identifiers at row +34/+38.
    pub enter: [i32; 2],
    /// Seated body initial/loop identifiers at row +3C/+40.
    pub seated: [i32; 2],
    /// Seated secondary initial/loop identifiers at row +44/+48.
    pub secondary: [i32; 2],
    /// Exit initial/loop identifiers at row +68/+6C.
    pub exit: [i32; 2],
}

impl VehiclePassengerAnimationInput {
    /// 747B20's primary choice; -1 means absent and 506 means no selection.
    #[must_use]
    pub fn primary(self) -> Option<i32> {
        let pair = match self.phase {
            Phase::EnterDelay | Phase::Entering if self.flags & 1 != 0 => self.enter,
            Phase::Seated if self.flags & 2 != 0 => self.seated,
            Phase::ExitDelay | Phase::Exiting
                if self.flags & if self.special_exit { 8 } else { 0x8000 } != 0 =>
            {
                self.exit
            }
            _ => return None,
        };
        select(pair, self.completed & 2 != 0)
    }

    /// 747BD0's independent seated secondary choice.
    #[must_use]
    pub fn secondary(self) -> Option<i32> {
        if self.phase != Phase::Seated || self.flags & 4 == 0 {
            return None;
        }
        select(self.secondary, self.completed & 4 != 0)
    }

    /// 748560's primary stage before ordinary movement and posture selection.
    #[must_use]
    pub fn before_movement(self, alive: bool) -> Option<i32> {
        if alive && matches!(self.phase, Phase::Detached | Phase::Seated) {
            None
        } else {
            self.primary()
        }
    }

    /// 7485B0's secondary stage after ordinary movement and posture selection.
    #[must_use]
    pub fn after_movement(self) -> Option<i32> {
        self.secondary()
    }

    /// 7484E0 marks the completed lane; model root and key 26 are primary.
    #[must_use]
    pub fn complete(self, key_bone: i32) -> u32 {
        self.completed
            | if self.phase == Phase::Seated
                && self.primary().is_some()
                && self.secondary().is_some()
            {
                if matches!(key_bone, -1 | 26) { 2 } else { 4 }
            } else {
                6
            }
    }
}

fn select(pair: [i32; 2], completed: bool) -> Option<i32> {
    let selected = if !completed && pair[0] != -1 {
        pair[0]
    } else {
        pair[1]
    };
    (!matches!(selected, -1 | 506)).then_some(selected)
}

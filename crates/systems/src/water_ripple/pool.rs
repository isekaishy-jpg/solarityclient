//! The 32 local and 96 other-unit ripple slots selected by 0x0079D180.

use std::collections::VecDeque;

use glam::Vec3;

use super::envelope::{WaterRippleEnvelope, WaterRippleEnvelopeError};

/// The native pool bank reserved for the emitting unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterRippleOwner {
    /// Local player requests cycle through slots 0..32.
    LocalPlayer,
    /// Every other unit shares slots 32..128.
    OtherUnit,
}

/// One live ripple and the world-space water triangles frozen at its emission.
#[derive(Debug)]
pub struct WaterRipple {
    envelope: WaterRippleEnvelope,
    triangles: Box<[[Vec3; 3]]>,
}

impl WaterRipple {
    /// Retains the water-only query's authored triangle order without clipping
    /// or rebuilding the geometry on subsequent frames (0x0079CF40).
    pub fn new(envelope: WaterRippleEnvelope, triangles: Box<[[Vec3; 3]]>) -> Self {
        Self {
            envelope,
            triangles,
        }
    }

    /// Current projection and alpha state for the frozen geometry.
    pub fn envelope(&self) -> &WaterRippleEnvelope {
        &self.envelope
    }

    /// Water triangles in their original world-space order.
    pub fn triangles(&self) -> &[[Vec3; 3]] {
        &self.triangles
    }
}

/// A slot keeps its ring identity when an earlier ripple retires naturally.
struct RippleSlot {
    index: u8,
    ripple: WaterRipple,
}

/// Scene-owned ripple bank with oldest-to-newest native active-list order.
pub struct WaterRipplePool {
    active: VecDeque<RippleSlot>,
    next_local: u8,
    next_other: u8,
}

impl Default for WaterRipplePool {
    fn default() -> Self {
        Self {
            active: VecDeque::with_capacity(128),
            next_local: 0,
            next_other: 32,
        }
    }
}

impl WaterRipplePool {
    /// Reuses the next slot in the selected bank even when a different slot is
    /// vacant. Native 0x006DED60 moves a reused slot to the active-list tail.
    pub fn insert(&mut self, ripple: WaterRipple, owner: WaterRippleOwner) {
        let index = match owner {
            WaterRippleOwner::LocalPlayer => {
                let index = self.next_local;
                self.next_local = (index + 1) % 32;
                index
            }
            WaterRippleOwner::OtherUnit => {
                let index = self.next_other;
                self.next_other = if index == 127 { 32 } else { index + 1 };
                index
            }
        };
        if let Some(previous) = self.active.iter().position(|slot| slot.index == index) {
            self.active.remove(previous);
        }
        self.active.push_back(RippleSlot { index, ripple });
    }

    /// Advances active records once and releases their frozen geometry on
    /// retirement. Invalid frame times preserve the entire pool.
    ///
    /// # Errors
    /// Rejects negative or nonfinite frame times.
    pub fn advance(
        &mut self,
        elapsed: f32,
        scene_time: f32,
    ) -> Result<(), WaterRippleEnvelopeError> {
        if [elapsed, scene_time]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.)
        {
            return Err(WaterRippleEnvelopeError);
        }
        let mut index = 0;
        while index < self.active.len() {
            if self.active[index]
                .ripple
                .envelope
                .advance(elapsed, scene_time)?
            {
                index += 1;
            } else {
                self.active.remove(index);
            }
        }
        Ok(())
    }

    /// Visits live ripples in the stock active-list order. Rendering filters
    /// this order into circular and directional texture passes.
    pub fn ripples(&self) -> impl Iterator<Item = &WaterRipple> {
        self.active.iter().map(|slot| &slot.ripple)
    }
}

//! Native `006EB730` timeline and `006E97D0` 32-sample delay history.

#[cfg(test)]
#[path = "../../../tests/movement/remote_clock.rs"]
mod tests;

/// Packet and world timing at the sole movement admission boundary.
#[derive(Clone, Copy, Debug)]
pub struct RemoteMovementReceipt {
    /// Wrapping timestamp supplied by MovementInfo.
    pub server_ms: u32,
    /// Local packet receipt timestamp.
    pub receipt_ms: u32,
    /// Current world frame clock; receipt is clamped forward to this clock.
    pub frame_ms: u32,
    /// Current subject primary movement flags before admitting the packet.
    pub flags: u32,
    /// Whether the subject already has pending movement commands.
    pub has_pending_commands: bool,
    /// Whether the native path owner is allocated, including a finished path.
    pub has_path: bool,
}

/// Native decision to apply now or retain a command for its adjusted timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteMovementAdmission {
    /// Local time at which the snapshot belongs.
    pub timeline_ms: u32,
    /// Signed correction added to the clamped receipt time.
    pub adjustment_ms: i32,
    /// A path bypasses the ordinary future-time gate, matching native admission.
    pub immediate: bool,
}

/// Delay history belongs to one movement subject for its complete lifetime.
#[derive(Clone, Debug)]
pub struct RemoteMovementClock {
    initialized: bool,
    server_ms: u32,
    local_ms: u32,
    delay_ms: i32,
    history: [i16; 32],
    next_sample: usize,
}

impl Default for RemoteMovementClock {
    fn default() -> Self {
        // 006EBD30 primes sixteen early and sixteen late samples. The native
        // initial delay is 50 ms; a zero-filled ring changes first admission.
        let mut history = [50; 32];
        history[..16].fill(-50);
        Self {
            initialized: false,
            server_ms: 0,
            local_ms: 0,
            delay_ms: 50,
            history,
            next_sample: 0,
        }
    }
}

impl RemoteMovementClock {
    /// Aligns a packet using wrapping signed clock differences. Reordered
    /// server timestamps never move the server anchor backwards.
    pub fn admit(&mut self, receipt: RemoteMovementReceipt) -> RemoteMovementAdmission {
        if !self.initialized {
            self.server_ms = receipt.server_ms;
            self.local_ms = receipt.receipt_ms;
            self.initialized = true;
        }
        let receipt_ms = if (receipt.receipt_ms.wrapping_sub(receipt.frame_ms) as i32) < 0 {
            receipt.frame_ms
        } else {
            receipt.receipt_ms
        };
        let server_delta = (receipt.server_ms.wrapping_sub(self.server_ms) as i32).max(0);
        if server_delta > 0 {
            self.server_ms = receipt.server_ms;
        }
        let local_delta = receipt_ms.wrapping_sub(self.local_ms) as i32;
        let mut adjustment_ms = server_delta.wrapping_sub(local_delta);
        let sample = self
            .delay_ms
            .wrapping_sub(server_delta)
            .wrapping_add(local_delta);
        self.history[self.next_sample] = sample.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        self.next_sample = (self.next_sample + 1) & 31;
        let maximum_delay = i32::from(self.history.iter().copied().max().unwrap_or(0));
        if !receipt.has_path {
            if receipt.flags & 0xc010ff == 0 && !receipt.has_pending_commands {
                adjustment_ms = adjustment_ms
                    .wrapping_add(maximum_delay.wrapping_sub(self.delay_ms))
                    .clamp(-500, 1000);
                if (receipt_ms
                    .wrapping_add(adjustment_ms as u32)
                    .wrapping_sub(self.local_ms) as i32)
                    < 0
                {
                    adjustment_ms = self.local_ms.wrapping_sub(receipt_ms) as i32;
                }
                self.delay_ms = maximum_delay;
            }
            adjustment_ms = adjustment_ms.clamp(-500, 1000);
        }
        self.local_ms = receipt_ms.wrapping_add(adjustment_ms as u32);
        RemoteMovementAdmission {
            timeline_ms: self.local_ms,
            adjustment_ms,
            immediate: receipt.has_path
                || (receipt.frame_ms.wrapping_sub(self.local_ms) as i32) >= 0,
        }
    }
}

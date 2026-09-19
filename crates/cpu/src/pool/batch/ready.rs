//! Intrusive cost bins use the already charged node storage, with no extra queue.

use super::state::{Node, Status};

/// Each node is enqueued at most once. Cancellation leaves a stale entry for the
/// ordinary status check; clearing a phase discards all three lists atomically.
#[derive(Default)]
pub(super) struct ReadyJobs {
    heads: [Option<usize>; 3],
    tails: [Option<usize>; 3],
    len: usize,
}

impl ReadyJobs {
    /// Appends behind equal-cost jobs in their readiness order.
    pub fn push(&mut self, nodes: &mut [Node], index: usize) {
        let bin = nodes[index].cost.bin();
        nodes[index].next_ready = None;
        if let Some(tail) = self.tails[bin] {
            nodes[tail].next_ready = Some(index);
        } else {
            self.heads[bin] = Some(index);
        }
        self.tails[bin] = Some(index);
        self.len += 1;
    }

    /// Visits exactly three bins, independent of the phase's admitted job count.
    pub fn pop(&mut self, nodes: &mut [Node]) -> Option<usize> {
        for bin in (0..3).rev() {
            if let Some(index) = self.heads[bin] {
                self.heads[bin] = nodes[index].next_ready.take();
                if self.heads[bin].is_none() {
                    self.tails[bin] = None;
                }
                self.len -= 1;
                return Some(index);
            }
        }
        None
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Cancelled heads must not keep a phase in a high global cost bucket. Their
    /// links stay valid until removal, and each node is removed only once.
    pub fn cost(&mut self, nodes: &mut [Node]) -> u8 {
        for bin in (0..3).rev() {
            while let Some(index) = self.heads[bin] {
                if !matches!(nodes[index].status, Status::Terminal(_)) {
                    return bin as u8;
                }
                self.heads[bin] = nodes[index].next_ready.take();
                if self.heads[bin].is_none() {
                    self.tails[bin] = None;
                }
                self.len -= 1;
            }
        }
        0
    }

    /// Old node links are unreachable after the epoch is cleared.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

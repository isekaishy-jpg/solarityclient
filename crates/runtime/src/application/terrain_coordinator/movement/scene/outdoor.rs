//! Reusable native 64-bin WMO entry lists before portal traversal.

/// One entry refers to the stable root array used throughout scene preparation.
#[derive(Clone, Copy)]
pub(super) struct OutdoorSceneGroup {
    pub(super) root: usize,
    pub(super) group: usize,
}

/// 792AD0 appends static entries; 792BD0 appends converted moving entries.
/// 79A790/79A160 consume increasing bins and retain insertion order within each.
pub(super) struct OutdoorSceneGroups {
    pub(super) bins: [Vec<OutdoorSceneGroup>; 64],
    cursor: [usize; 2],
}

impl Default for OutdoorSceneGroups {
    fn default() -> Self {
        Self {
            bins: std::array::from_fn(|_| Vec::new()),
            cursor: [0, 0],
        }
    }
}

impl OutdoorSceneGroups {
    /// Empties the frame lists while retaining each bin's allocation.
    pub(super) fn clear(&mut self) {
        for bin in &mut self.bins {
            bin.clear();
        }
        self.cursor = [0, 0];
    }

    /// Consumes native increasing-bin traversal without allocating a flat list.
    pub(super) fn next_group(&mut self) -> Option<OutdoorSceneGroup> {
        while self.cursor[0] < self.bins.len() {
            if let Some(&group) = self.bins[self.cursor[0]].get(self.cursor[1]) {
                self.cursor[1] += 1;
                return Some(group);
            }
            self.cursor[0] += 1;
            self.cursor[1] = 0;
        }
        None
    }
}

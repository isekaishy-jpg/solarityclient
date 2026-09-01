//! Live scroll offsets parallel to the retained UI object arena.

use crate::script::UiRuntimeObjectPlan;

/// Arena-aligned ScrollFrame offsets and ranges after authored Lua mutation.
#[derive(Clone, Debug, PartialEq)]
pub struct UiScrollFramePlan {
    states: Vec<Option<UiScrollFrameState>>,
}

impl UiScrollFramePlan {
    pub(crate) fn from_live(live: &UiRuntimeObjectPlan) -> Self {
        Self {
            states: live
                .objects()
                .iter()
                .map(|object| {
                    object
                        .scroll_offset
                        .zip(object.scroll_range)
                        .map(|(offset, range)| UiScrollFrameState { offset, range })
                })
                .collect(),
        }
    }

    /// Returns live offset/range state for one ScrollFrame arena index.
    #[must_use]
    pub fn state(&self, object_index: usize) -> Option<UiScrollFrameState> {
        self.states.get(object_index).copied().flatten()
    }
}

/// One ScrollFrame's clamped horizontal and vertical values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScrollFrameState {
    offset: (f64, f64),
    range: (f64, f64),
}

impl UiScrollFrameState {
    /// Returns the current horizontal and vertical scroll offset.
    #[must_use]
    pub const fn offset(self) -> (f64, f64) {
        self.offset
    }

    /// Returns the current horizontal and vertical scroll range.
    #[must_use]
    pub const fn range(self) -> (f64, f64) {
        self.range
    }
}

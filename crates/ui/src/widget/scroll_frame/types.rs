//! Live scroll offsets parallel to the retained UI object arena.

use crate::{UiObjectKind, script::UiRuntimeObjectPlan};

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
                        .map(|(offset, range)| UiScrollFrameState {
                            offset,
                            range,
                            child: object.scroll_child,
                        })
                })
                .collect(),
        }
    }

    /// Copies offsets only for objects changed in the current retained transaction.
    pub(crate) fn refresh_objects(
        &mut self,
        live: &UiRuntimeObjectPlan,
        indices: impl IntoIterator<Item = usize>,
    ) {
        for index in indices {
            let object = &live.objects()[index];
            self.states[index] =
                object
                    .scroll_offset
                    .zip(object.scroll_range)
                    .map(|(offset, range)| UiScrollFrameState {
                        offset,
                        range,
                        child: object.scroll_child,
                    });
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
    /// Arena index of the sole region tree translated and clipped by this frame.
    child: Option<usize>,
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

    pub(crate) const fn child(self) -> Option<usize> {
        self.child
    }
}

/// Finds the nearest ScrollFrame whose assigned child owns this object.
///
/// Scrollbar chrome is commonly parented directly to the ScrollFrame beside
/// its assigned scroll child. Merely finding a ScrollFrame ancestor would
/// incorrectly translate and clip that chrome with the scrolling content.
pub(crate) fn nearest_owning_scroll_frame(
    live: &UiRuntimeObjectPlan,
    object_index: usize,
) -> Option<usize> {
    let mut descendant = object_index;
    let mut parent = live.objects().get(object_index)?.parent;
    while let Some(index) = parent {
        let candidate = live.objects().get(index)?;
        if candidate.kind == UiObjectKind::ScrollFrame && candidate.scroll_child == Some(descendant)
        {
            return Some(index);
        }
        descendant = index;
        parent = candidate.parent;
    }
    None
}

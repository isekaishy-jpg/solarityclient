//! Typed scroll publication reads only the owners named by a mutation journal.

mod ranges;

pub(super) use ranges::{initialize, invalidate_ranges, refresh_scroll_ranges};

use super::{
    DIRTY_SCROLL, UiScriptRuntime, execution_error, finite_region_number, horizontal_scroll_key,
    horizontal_scroll_range_key, snapshot_slider, vertical_scroll_key, vertical_scroll_range_key,
};
use crate::script::{UiRuntimeObject, UiRuntimeObjectPlan};
use crate::{UiBundle, UiObjectKind, UiScriptError};

impl UiScriptRuntime {
    /// All other callback changes keep their ordinary typed or full publisher.
    pub(crate) fn is_scroll_journal(&self, dirty: &[(usize, u32)]) -> bool {
        !dirty.is_empty() && dirty.iter().all(|(_, flags)| *flags == DIRTY_SCROLL)
    }

    /// Retains the old state of changed owners for delta-based thumb positioning.
    /// No unchanged object or document text is cloned or read from Lua.
    pub(crate) fn refresh_scroll_objects(
        &self,
        bundle: &UiBundle,
        live: &mut UiRuntimeObjectPlan,
        dirty: &[(usize, u32)],
    ) -> Result<Vec<(usize, UiRuntimeObject)>, UiScriptError> {
        let mut previous = Vec::with_capacity(dirty.len());
        for &(index, _) in dirty {
            let old = live.objects()[index].clone();
            let object = self.runtime_object(bundle.lua(), index, "scroll publication")?;
            match old.kind {
                UiObjectKind::Slider => {
                    live.replace_slider(index, snapshot_slider(index + 1, &object)?)
                }
                UiObjectKind::ScrollFrame => {
                    let offset = (
                        finite_region_number(
                            &object,
                            horizontal_scroll_key(),
                            index + 1,
                            "horizontal scroll",
                        )?,
                        finite_region_number(
                            &object,
                            vertical_scroll_key(),
                            index + 1,
                            "vertical scroll",
                        )?,
                    );
                    let range = (
                        finite_region_number(
                            &object,
                            horizontal_scroll_range_key(),
                            index + 1,
                            "horizontal scroll range",
                        )?,
                        finite_region_number(
                            &object,
                            vertical_scroll_range_key(),
                            index + 1,
                            "vertical scroll range",
                        )?,
                    );
                    live.replace_scroll_state(index, offset, range);
                }
                _ => {
                    return Err(execution_error(
                        "scroll publication",
                        mlua::Error::runtime("scroll journal owner has no scroll state"),
                    ));
                }
            }
            previous.push((index, old));
        }
        Ok(previous)
    }
}

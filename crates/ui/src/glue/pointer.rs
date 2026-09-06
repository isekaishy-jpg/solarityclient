//! Stock-ordered pointer targeting for the retained Glue object arena.

use crate::script::UiRuntimeObjectPlan;
use crate::{UiFrameStrata, UiObjectKind, UiObjectRole, UiRegionGeometryPlan};

use super::UiPointerButton;

pub(super) struct UiPointerButtonScope {
    state: std::rc::Rc<std::cell::Cell<Option<UiPointerButton>>>,
    previous: Option<UiPointerButton>,
}

impl UiPointerButtonScope {
    pub(super) fn new(
        state: std::rc::Rc<std::cell::Cell<Option<UiPointerButton>>>,
        button: Option<UiPointerButton>,
    ) -> Self {
        let previous = state.replace(button);
        Self { state, previous }
    }
}

impl Drop for UiPointerButtonScope {
    fn drop(&mut self) {
        self.state.set(self.previous);
    }
}

/// Live interaction facts parallel to frame-capable objects.
pub(super) struct UiPointerPlan {
    targets: Vec<Option<UiPointerTarget>>,
}

impl UiPointerPlan {
    /// Copies only state required by main-thread hit testing and capture.
    pub(super) fn from_live(live: &UiRuntimeObjectPlan) -> Self {
        let mut thumb_indices = vec![None; live.objects().len()];
        for (index, object) in live.objects().iter().enumerate() {
            if object.role == UiObjectRole::ThumbTexture
                && let Some(parent) = object.parent
                && let Some(slot) = thumb_indices.get_mut(parent)
            {
                *slot = Some(index);
            }
        }
        let targets = live
            .objects()
            .iter()
            .enumerate()
            .map(|(index, object)| {
                Some(UiPointerTarget {
                    kind: object.kind,
                    parent: object.parent,
                    strata: object.frame_strata?,
                    level: object.frame_level?,
                    keyboard_enabled: object.keyboard_enabled?,
                    mouse_enabled: object.mouse_enabled?,
                    mouse_wheel_enabled: object.mouse_wheel_enabled?,
                    motion_scripts_while_disabled: object.motion_scripts_while_disabled?,
                    enabled: object.enabled.unwrap_or(true),
                    hit_rect_insets: object.hit_rect_insets?,
                    click_action: object.click_action.unwrap_or(0),
                    edit_focused: object.edit_focused.unwrap_or(false),
                    slider: object.slider.map(|slider| UiPointerSlider {
                        minimum: slider.minimum,
                        maximum: slider.maximum,
                        step: slider.step,
                        vertical: slider.vertical,
                        thumb_index: thumb_indices[index],
                    }),
                })
            })
            .collect();
        Self { targets }
    }

    /// Refreshes EditBox focus without recopied hit-test state for every frame.
    pub(super) fn refresh_edit_box_focus(
        &mut self,
        live: &UiRuntimeObjectPlan,
        object_indices: &[usize],
    ) {
        for &object_index in object_indices {
            let Some(object) = live.objects().get(object_index) else {
                continue;
            };
            let Some(target) = self.targets.get_mut(object_index).and_then(Option::as_mut) else {
                continue;
            };
            if target.kind == UiObjectKind::EditBox {
                target.edit_focused = object.edit_focused.unwrap_or(false);
            }
        }
    }

    /// Returns the frontmost enabled mouse target containing one UI point.
    pub(super) fn hit_test(
        &self,
        geometry: &UiRegionGeometryPlan,
        point: (f64, f64),
    ) -> Option<usize> {
        self.hit_test_internal(geometry, point, true)
    }

    /// Returns the frontmost mouse target eligible for pointer-motion scripts.
    ///
    /// Build 12340 lets authored controls opt into `OnEnter`/`OnLeave` while
    /// disabled through `motionScriptsWhileDisabled`.
    pub(super) fn hover_hit_test(
        &self,
        geometry: &UiRegionGeometryPlan,
        point: (f64, f64),
    ) -> Option<usize> {
        self.hit_test_internal(geometry, point, false)
    }

    fn hit_test_internal(
        &self,
        geometry: &UiRegionGeometryPlan,
        point: (f64, f64),
        require_enabled: bool,
    ) -> Option<usize> {
        self.targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| {
                let target = target.as_ref()?;
                let region = geometry.region(index)?;
                if !target.mouse_enabled
                    || (!target.enabled
                        && (require_enabled || !target.motion_scripts_while_disabled))
                    || !region.effectively_shown()
                    || region.effective_alpha() <= 0.0
                {
                    return None;
                }
                let bounds = region.presentation_bounds();
                let scale = region.effective_scale();
                let [left, right, top, bottom] = target.hit_rect_insets;
                let hit_left = bounds.left() + left * scale;
                let hit_right = bounds.right() - right * scale;
                let hit_top = bounds.top() - top * scale;
                let hit_bottom = bounds.bottom() + bottom * scale;
                (hit_left <= point.0
                    && point.0 <= hit_right
                    && hit_bottom <= point.1
                    && point.1 <= hit_top)
                    .then_some((target.strata, target.level, index))
            })
            .max()
            .map(|(_, _, index)| index)
    }

    /// Returns the focused, visible EditBox selected by native focus state.
    pub(super) fn focused_edit_box(&self, geometry: &UiRegionGeometryPlan) -> Option<usize> {
        let (focused_rank, focused_index) = self
            .targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| {
                let target = target.as_ref()?;
                let region = geometry.region(index)?;
                (target.kind == UiObjectKind::EditBox
                    && target.edit_focused
                    && region.effectively_shown())
                .then_some(((target.strata, target.level, index), index))
            })
            .max()?;
        if let Some(keyboard_index) = self.keyboard_target(geometry) {
            let keyboard = self.targets[keyboard_index].as_ref()?;
            let keyboard_rank = (keyboard.strata, keyboard.level, keyboard_index);
            if keyboard_rank > focused_rank && !self.is_ancestor(keyboard_index, focused_index) {
                return None;
            }
        }
        Some(focused_index)
    }

    fn is_ancestor(&self, ancestor: usize, mut object: usize) -> bool {
        while let Some(parent) = self
            .targets
            .get(object)
            .and_then(Option::as_ref)
            .and_then(|target| target.parent)
        {
            if parent == ancestor {
                return true;
            }
            object = parent;
        }
        false
    }

    /// Returns the frontmost visible frame accepting unconsumed keyboard input.
    pub(super) fn keyboard_target(&self, geometry: &UiRegionGeometryPlan) -> Option<usize> {
        self.targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| {
                let target = target.as_ref()?;
                let region = geometry.region(index)?;
                (target.keyboard_enabled
                    && region.effectively_shown()
                    && region.effective_alpha() > 0.0)
                    .then_some((target.strata, target.level, index))
            })
            .max()
            .map(|(_, _, index)| index)
    }

    /// Returns the concrete object family at one retained interaction slot.
    pub(super) fn kind(&self, object_index: usize) -> Option<UiObjectKind> {
        self.targets
            .get(object_index)
            .and_then(Option::as_ref)
            .map(|target| target.kind)
    }

    /// Maps one pointer coordinate to the stock slider range, accounting for
    /// the thumb extent at both ends of the usable track.
    pub(super) fn slider_value_at(
        &self,
        geometry: &UiRegionGeometryPlan,
        object_index: usize,
        point: (f64, f64),
    ) -> Option<f64> {
        let target = self.targets.get(object_index)?.as_ref()?;
        let slider = target.slider?;
        let track = geometry.region(object_index)?.presentation_bounds();
        let thumb = slider
            .thumb_index
            .and_then(|index| geometry.region(index))
            .map(|region| region.presentation_bounds());
        let fraction = if slider.vertical {
            let thumb_height = thumb.map_or(0.0, |bounds| bounds.height());
            let usable = (track.height() - thumb_height).max(0.0);
            if usable == 0.0 {
                0.0
            } else {
                ((track.top() - thumb_height * 0.5 - point.1) / usable).clamp(0.0, 1.0)
            }
        } else {
            let thumb_width = thumb.map_or(0.0, |bounds| bounds.width());
            let usable = (track.width() - thumb_width).max(0.0);
            if usable == 0.0 {
                0.0
            } else {
                ((point.0 - track.left() - thumb_width * 0.5) / usable).clamp(0.0, 1.0)
            }
        };
        let value = slider.minimum + fraction * (slider.maximum - slider.minimum);
        Some(if slider.step > 0.0 {
            (slider.minimum + ((value - slider.minimum) / slider.step).round() * slider.step)
                .clamp(slider.minimum, slider.maximum)
        } else {
            value.clamp(slider.minimum, slider.maximum)
        })
    }

    /// Returns the frontmost wheel-enabled ScrollFrame under one UI point.
    pub(super) fn wheel_hit_test(
        &self,
        geometry: &UiRegionGeometryPlan,
        point: (f64, f64),
    ) -> Option<usize> {
        self.targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| {
                let target = target.as_ref()?;
                let region = geometry.region(index)?;
                if target.kind != UiObjectKind::ScrollFrame
                    || !target.mouse_wheel_enabled
                    || !region.effectively_shown()
                    || region.effective_alpha() <= 0.0
                {
                    return None;
                }
                let bounds = region.presentation_bounds();
                (bounds.left() <= point.0
                    && point.0 <= bounds.right()
                    && bounds.bottom() <= point.1
                    && point.1 <= bounds.top())
                .then_some((target.strata, target.level, index))
            })
            .max()
            .map(|(_, _, index)| index)
    }

    /// Returns whether this target registered the exact pointer transition.
    pub(super) fn activates(
        &self,
        object_index: usize,
        button: UiPointerButton,
        pressed: bool,
    ) -> bool {
        self.targets
            .get(object_index)
            .and_then(Option::as_ref)
            .is_some_and(|target| target.click_action & button.action_mask(pressed) != 0)
    }
}

/// One live button's stock interaction ordering and hit rectangle.
struct UiPointerTarget {
    kind: UiObjectKind,
    parent: Option<usize>,
    strata: UiFrameStrata,
    level: i32,
    keyboard_enabled: bool,
    mouse_enabled: bool,
    mouse_wheel_enabled: bool,
    motion_scripts_while_disabled: bool,
    enabled: bool,
    hit_rect_insets: [f64; 4],
    click_action: u64,
    edit_focused: bool,
    slider: Option<UiPointerSlider>,
}

#[derive(Clone, Copy)]
struct UiPointerSlider {
    minimum: f64,
    maximum: f64,
    step: f64,
    vertical: bool,
    thumb_index: Option<usize>,
}

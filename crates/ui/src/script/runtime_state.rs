//! Owned region and identity state copied from live Lua object tables.

use mlua::{Lua, Table};

use super::simple_script::{
    DIRTY_FRAME, DIRTY_LAYOUT, DIRTY_MODEL, DIRTY_TEXT, DIRTY_TEXTURE, DIRTY_TEXTURE_VERTEX_COLOR,
    DIRTY_WIDGET, OBJECT_REGISTRY, alpha_key, anchors_key, backdrop_border_color_key,
    backdrop_color_key, button_pressed_key, checked_key, click_action_key, desaturated_key,
    disabled_font_key, disabled_text_color_key, draw_layer_key, draw_sub_level_key,
    edit_caret_visible_key, edit_cursor_key, edit_focused_key, edit_highlight_color_key,
    edit_max_letters_key, edit_multi_line_key, edit_password_key, edit_selection_end_key,
    edit_selection_start_key, edit_text_insets_key, enabled_key, font_face_key, font_flags_key,
    font_height_key, font_object_key, font_set_key, font_shadow_color_key, font_shadow_offset_key,
    frame_level_key, frame_strata_key, height_key, highlight_font_key, highlight_locked_key,
    hit_rect_insets_key, horizontal_scroll_key, horizontal_scroll_range_key, horizontal_tiling_key,
    hovered_key, index_key, justify_h_key, justify_v_key, keyboard_enabled_key, max_text_lines_key,
    model_background_light_ghost_key, model_background_light_live_key, model_camera_key,
    model_character_light_ghost_key, model_character_light_live_key, model_file_key,
    model_fog_color_key, model_fog_far_key, model_fog_near_key, model_glow_key,
    model_instance_generation_key, model_pet_light_ghost_key, model_pet_light_live_key,
    model_rotation_key, model_scale_key, model_sequence_key, model_sequence_time_key,
    model_sequence_time_sequence_key, model_unit_key, motion_scripts_while_disabled_key,
    mouse_enabled_key, mouse_wheel_enabled_key, name_key, non_blocking_key, non_space_wrap_key,
    normal_font_key, parent_key, parse_point, portrait_unit_key, role_key, scale_key,
    scroll_child_key, shown_key, slider_max_key, slider_min_key, slider_orientation_key,
    slider_step_key, slider_value_key, spacing_key, tex_coord_key, text_color_key, text_key,
    texture_blend_mode_key, texture_color_key, texture_file_key, texture_solid_color_key, type_key,
    vertex_color_set_key, vertical_scroll_key, vertical_scroll_range_key, vertical_tiling_key,
    width_key, word_wrap_key,
};
use crate::animation::owner_animation_transforms;
use crate::{
    FontRasterization, HorizontalJustification, UiBlendMode, UiDrawLayer, UiFrameStrata,
    UiObjectKind, UiObjectRole, UiPoint, UiScriptError, VerticalJustification,
};
use solarity_asset::AssetPath;

/// One live anchor after XML construction and startup Lua mutation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRuntimeAnchor {
    pub(crate) point: UiPoint,
    pub(crate) target: Option<usize>,
    pub(crate) relative_point: UiPoint,
    pub(crate) offset: (f64, f64),
}

/// Identity and mutable region properties for one live Lua object.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeObject {
    pub(crate) name: Option<String>,
    pub(crate) kind: UiObjectKind,
    pub(crate) role: UiObjectRole,
    pub(crate) parent: Option<usize>,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) shown: bool,
    pub(crate) alpha: f64,
    pub(crate) scale: f64,
    pub(crate) animation_alpha_delta: f64,
    pub(crate) animation_offset: (f64, f64),
    pub(crate) animation_active: bool,
    pub(crate) first_anchor: usize,
    pub(crate) anchor_count: usize,
    pub(crate) texture: Option<UiRuntimeTexture>,
    pub(crate) text: Option<UiRuntimeText>,
    pub(crate) simple_html_text: Option<String>,
    pub(crate) model: Option<UiRuntimeModel>,
    pub(crate) backdrop_color: Option<[f64; 4]>,
    pub(crate) backdrop_border_color: Option<[f64; 4]>,
    pub(crate) frame_level: Option<i32>,
    pub(crate) frame_strata: Option<UiFrameStrata>,
    pub(crate) keyboard_enabled: Option<bool>,
    pub(crate) mouse_enabled: Option<bool>,
    pub(crate) motion_scripts_while_disabled: Option<bool>,
    pub(crate) mouse_wheel_enabled: Option<bool>,
    pub(crate) hit_rect_insets: Option<[f64; 4]>,
    pub(crate) scroll_offset: Option<(f64, f64)>,
    pub(crate) scroll_range: Option<(f64, f64)>,
    pub(crate) scroll_child: Option<usize>,
    pub(crate) slider: Option<UiRuntimeSlider>,
    pub(crate) status_bar: Option<(crate::widget::StatusBarState, Option<usize>)>,
    pub(crate) enabled: Option<bool>,
    pub(crate) click_action: Option<u64>,
    pub(crate) checked: Option<bool>,
    pub(crate) highlighted: Option<bool>,
    pub(crate) pushed: Option<bool>,
    pub(crate) edit_focused: Option<bool>,
}

/// Native value and axis state for one live Slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRuntimeSlider {
    pub(crate) minimum: f64,
    pub(crate) maximum: f64,
    pub(crate) value: f64,
    pub(crate) step: f64,
    pub(crate) vertical: bool,
}

/// Post-Lua text, font, alignment, and EditBox presentation state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeText {
    pub(crate) content: String,
    pub(crate) face: AssetPath,
    pub(crate) height: f64,
    pub(crate) rasterization: FontRasterization,
    pub(crate) outline_width: f64,
    pub(crate) color: [f64; 4],
    pub(crate) shadow_offset: [f64; 2],
    pub(crate) shadow_color: [f64; 4],
    pub(crate) spacing: f64,
    pub(crate) word_wrap: bool,
    pub(crate) non_space_wrap: bool,
    pub(crate) max_lines: u32,
    pub(crate) max_letters: u32,
    pub(crate) horizontal: HorizontalJustification,
    pub(crate) vertical: VerticalJustification,
    pub(crate) draw_layer: UiDrawLayer,
    pub(crate) draw_sub_level: i16,
    pub(crate) password: bool,
    pub(crate) multiline: bool,
    pub(crate) text_insets: [f64; 4],
    pub(crate) cursor: usize,
    pub(crate) selection: [usize; 2],
    pub(crate) caret_visible: bool,
    pub(crate) highlight_color: [f64; 4],
}

/// A button-label font transition whose glyph metrics and atlas coverage are unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRuntimeTextColorChange {
    pub(crate) object_index: usize,
    pub(crate) previous_color: [f64; 4],
    pub(crate) color: [f64; 4],
    pub(crate) previous_shadow_color: [f64; 4],
    pub(crate) shadow_color: [f64; 4],
}

/// Post-Lua model source and animation-selection properties.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeModel {
    pub(crate) file: Option<AssetPath>,
    /// Explicit instance replacements, distinct from shared model-file identity.
    pub(crate) instance_generation: u32,
    pub(crate) unit: Option<String>,
    pub(crate) rotation_radians: f64,
    pub(crate) camera: i32,
    pub(crate) sequence: u32,
    pub(crate) sequence_time_sequence: u32,
    pub(crate) sequence_time_ms: i32,
    pub(crate) scale: f64,
    pub(crate) fog_color: Option<[f64; 3]>,
    pub(crate) fog_near: f64,
    pub(crate) fog_far: f64,
    pub(crate) glow: f64,
    pub(crate) background_lights: UiRuntimeModelLightSets,
    pub(crate) character_lights: UiRuntimeModelLightSets,
    pub(crate) pet_lights: UiRuntimeModelLightSets,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRuntimeModelLight {
    pub(crate) direction: [f64; 3],
    pub(crate) ambient: [f64; 3],
    pub(crate) diffuse: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UiRuntimeModelLightSets {
    pub(crate) live: [Option<UiRuntimeModelLight>; 4],
    pub(crate) ghost: [Option<UiRuntimeModelLight>; 4],
}

/// Post-Lua texture source and presentation properties.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeTexture {
    pub(crate) portrait_unit: Option<String>,
    pub(crate) file: Option<AssetPath>,
    pub(crate) solid_color: Option<[f64; 4]>,
    pub(crate) blend_mode: UiBlendMode,
    pub(crate) tex_coords: [f64; 8],
    pub(crate) vertex_colors: [[f64; 4]; 4],
    pub(crate) horizontal_tiling: bool,
    pub(crate) vertical_tiling: bool,
    pub(crate) non_blocking: bool,
    pub(crate) desaturated: bool,
    pub(crate) draw_layer: UiDrawLayer,
    pub(crate) draw_sub_level: i16,
}

/// Flat post-bootstrap object arena and its anchor records.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeObjectPlan {
    objects: Vec<UiRuntimeObject>,
    anchors: Vec<UiRuntimeAnchor>,
}

impl UiRuntimeObjectPlan {
    pub(crate) fn objects(&self) -> &[UiRuntimeObject] {
        &self.objects
    }

    pub(crate) fn anchors_for(&self, object: &UiRuntimeObject) -> &[UiRuntimeAnchor] {
        &self.anchors[object.first_anchor..object.first_anchor + object.anchor_count]
    }

    pub(crate) fn animation_transform_matches(
        &self,
        object_index: usize,
        transform: crate::animation::UiAnimationTransform,
    ) -> bool {
        self.objects.get(object_index).is_some_and(|object| {
            object.animation_alpha_delta == transform.alpha_delta
                && object.animation_offset == transform.offset
                && object.animation_active == transform.active
        })
    }

    pub(crate) fn replace_shown(&mut self, index: usize, shown: bool) {
        if let Some(object) = self.objects.get_mut(index) {
            object.shown = shown;
        }
    }

    pub(crate) fn replace_slider(&mut self, object_index: usize, slider: UiRuntimeSlider) {
        if let Some(object) = self.objects.get_mut(object_index) {
            object.slider = Some(slider);
        }
    }

    /// Keeps the retained copy aligned with dimensions published back to Lua.
    pub(crate) fn replace_resolved_dimensions(
        &mut self,
        object_index: usize,
        width: f64,
        height: f64,
    ) {
        if let Some(object) = self.objects.get_mut(object_index) {
            object.width = width;
            object.height = height;
        }
    }

    /// Replaces one object's compact anchor slice and repairs later offsets.
    fn replace_anchors(&mut self, object_index: usize, anchors: Vec<UiRuntimeAnchor>) {
        let Some(object) = self.objects.get(object_index) else {
            return;
        };
        let first = object.first_anchor;
        let old_count = object.anchor_count;
        let new_count = anchors.len();
        self.anchors.splice(first..first + old_count, anchors);
        self.objects[object_index].anchor_count = new_count;
        if new_count >= old_count {
            let delta = new_count - old_count;
            for object in &mut self.objects[object_index + 1..] {
                object.first_anchor += delta;
            }
        } else {
            let delta = old_count - new_count;
            for object in &mut self.objects[object_index + 1..] {
                object.first_anchor -= delta;
            }
        }
    }

    /// Returns changed live-text owners when local layout and packet ancestry
    /// remain compatible with an object-scoped glyph refresh.
    pub(crate) fn text_layout_changes_from(&self, previous: &Self) -> Option<Vec<usize>> {
        if self.anchors != previous.anchors || self.objects.len() != previous.objects.len() {
            return None;
        }
        let mut changed = Vec::new();
        for (object_index, (current, previous)) in
            self.objects.iter().zip(&previous.objects).enumerate()
        {
            let text_changed = current.text != previous.text
                || current.edit_focused != previous.edit_focused
                || current.width != previous.width
                || current.height != previous.height;
            let text_owns_dimension_change = current.text.is_some() || previous.text.is_some();
            if (current.width != previous.width || current.height != previous.height)
                && !text_owns_dimension_change
                || current.parent != previous.parent
                || current.frame_level != previous.frame_level
                || current.frame_strata != previous.frame_strata
                || current.scroll_child != previous.scroll_child
            {
                return None;
            }
            if text_changed {
                changed.push(object_index);
            }
        }
        Some(changed)
    }

    pub(crate) fn replace_scroll_state(
        &mut self,
        object_index: usize,
        offset: (f64, f64),
        range: (f64, f64),
    ) {
        if let Some(object) = self.objects.get_mut(object_index) {
            object.scroll_offset = Some(offset);
            object.scroll_range = Some(range);
        }
    }

    pub(crate) fn replace_button_highlight(&mut self, object_index: usize, highlighted: bool) {
        if let Some(object) = self.objects.get_mut(object_index)
            && matches!(
                object.kind,
                UiObjectKind::Button | UiObjectKind::CheckButton
            )
        {
            object.highlighted = Some(highlighted);
        }
    }

    pub(crate) fn replace_animation_transform(
        &mut self,
        object_index: usize,
        transform: crate::animation::UiAnimationTransform,
    ) {
        if let Some(object) = self.objects.get_mut(object_index) {
            object.animation_alpha_delta = transform.alpha_delta;
            object.animation_offset = transform.offset;
            object.animation_active = transform.active;
        }
    }

    /// Returns whether the only live presentation mutations are scroll offsets
    /// and Slider values. Those values move clipped content and native thumb
    /// geometry, but cannot invalidate text layout, object ownership, or input
    /// admission. Keeping that distinction here beside the complete snapshot
    /// prevents the interactive scroll path from accidentally overlooking a
    /// newly added runtime field.
    pub(crate) fn is_scroll_only_update_from(&self, previous: &Self) -> bool {
        if self.anchors != previous.anchors || self.objects.len() != previous.objects.len() {
            return false;
        }

        let mut changed = false;
        for (current, previous) in self.objects.iter().zip(&previous.objects) {
            if current == previous {
                continue;
            }

            let mut normalized = current.clone();
            normalized.scroll_offset = previous.scroll_offset;
            if let (Some(current_slider), Some(previous_slider)) =
                (normalized.slider.as_mut(), previous.slider)
            {
                current_slider.value = previous_slider.value;
            }
            if normalized != *previous {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// Returns whether only direct alpha or temporary animation contributions changed.
    ///
    /// Alpha and translation still require fresh resolved geometry and draw
    /// vertices, but they cannot alter text rasterization, object hierarchy,
    /// input topology, texture residency, or authored scroll ownership.
    pub(crate) fn is_visual_transform_only_update_from(&self, previous: &Self) -> bool {
        if self.anchors != previous.anchors || self.objects.len() != previous.objects.len() {
            return false;
        }

        let mut changed = false;
        for (current, previous) in self.objects.iter().zip(&previous.objects) {
            if current == previous {
                continue;
            }

            let mut normalized = current.clone();
            normalized.alpha = previous.alpha;
            normalized.animation_alpha_delta = previous.animation_alpha_delta;
            normalized.animation_offset = previous.animation_offset;
            normalized.animation_active = previous.animation_active;
            if normalized != *previous {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// Returns whether only explicit region visibility changed.
    ///
    /// The renderer can retain already-materialized object slots across a
    /// hide/show cycle. A newly revealed object which has never been resident
    /// is detected by the Glue owner and takes the ordinary preparation path.
    pub(crate) fn is_visibility_only_update_from(&self, previous: &Self) -> bool {
        if self.anchors != previous.anchors || self.objects.len() != previous.objects.len() {
            return false;
        }

        let mut changed = false;
        for (current, previous) in self.objects.iter().zip(&previous.objects) {
            if current == previous {
                continue;
            }

            let mut normalized = current.clone();
            normalized.shown = previous.shown;
            if normalized != *previous {
                return false;
            }
            changed = true;
        }
        changed
    }

    /// Returns whether only mutually exclusive Button presentation state changed.
    ///
    /// These fields select already-authored state textures. They do not alter
    /// geometry, text layout, hierarchy, texture residency, or pointer hit
    /// admission, so the renderer can retain its complete mesh topology.
    pub(crate) fn is_button_state_only_update_from(&self, previous: &Self) -> bool {
        if self.anchors != previous.anchors || self.objects.len() != previous.objects.len() {
            return false;
        }

        let mut changed = false;
        for (current, previous) in self.objects.iter().zip(&previous.objects) {
            if current == previous {
                continue;
            }

            let mut normalized = current.clone();
            normalized.checked = previous.checked;
            normalized.highlighted = previous.highlighted;
            normalized.pushed = previous.pushed;
            if normalized != *previous {
                return false;
            }
            changed = true;
        }
        changed
    }
}

pub(super) fn snapshot_runtime_objects(
    lua: &Lua,
    object_count: usize,
) -> Result<UiRuntimeObjectPlan, UiScriptError> {
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    let mut objects = Vec::with_capacity(object_count);
    let mut anchors = Vec::new();
    let animation_transforms = owner_animation_transforms(lua, object_count)
        .map_err(|error| snapshot_error("object animations", error))?;

    for lua_index in 1..=object_count {
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        let stored_index = table
            .raw_get::<usize>(index_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} index"), error))?;
        if stored_index != lua_index {
            return Err(UiScriptError::Plan {
                message: format!(
                    "live UI object {lua_index} stores mismatched index {stored_index}"
                ),
            });
        }

        let kind_name = table
            .raw_get::<String>(type_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} type"), error))?;
        let kind =
            UiObjectKind::from_element_name(&kind_name).ok_or_else(|| UiScriptError::Plan {
                message: format!("live UI object {lua_index} has unknown type {kind_name}"),
            })?;
        let role_name = table
            .raw_get::<String>(role_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} role"), error))?;
        let role = parse_role(&role_name).ok_or_else(|| UiScriptError::Plan {
            message: format!("live UI object {lua_index} has unknown role {role_name}"),
        })?;
        let parent = table
            .raw_get::<Option<usize>>(parent_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} parent"), error))?
            .map(|index| index.saturating_sub(1));
        if parent.is_some_and(|index| index >= object_count || index + 1 == lua_index) {
            return Err(UiScriptError::Plan {
                message: format!("live UI object {lua_index} has invalid parent"),
            });
        }

        let first_anchor = anchors.len();
        snapshot_anchors(lua_index, &table, object_count, &mut anchors)?;
        let texture = (kind == UiObjectKind::Texture)
            .then(|| snapshot_texture(lua_index, &table))
            .transpose()?;
        let text = if matches!(
            kind,
            UiObjectKind::FontString | UiObjectKind::EditBox | UiObjectKind::ScrollingMessageFrame
        ) {
            let (presentation_font, presentation_color) =
                button_presentation_font(&registry, role, parent)?;
            snapshot_text(
                lua_index,
                kind,
                &table,
                presentation_font.as_ref(),
                presentation_color,
            )?
        } else {
            None
        };
        let model = matches!(
            kind,
            UiObjectKind::Model
                | UiObjectKind::PlayerModel
                | UiObjectKind::DressUpModel
                | UiObjectKind::TabardModel
                | UiObjectKind::ModelFfx
        )
        .then(|| snapshot_model(lua_index, &table))
        .transpose()?;
        let is_frame = !matches!(kind, UiObjectKind::Texture | UiObjectKind::FontString);
        let is_button = matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton);
        let click_action = is_button
            .then(|| table.raw_get::<u64>(click_action_key()))
            .transpose()
            .map_err(|error| snapshot_error(format!("object {lua_index} click action"), error))?;
        // Snapshot the assigned child identity rather than inferring ownership
        // from ancestry; ScrollFrame chrome shares the same direct parent.
        let scroll_child = (kind == UiObjectKind::ScrollFrame)
            .then(|| {
                table
                    .raw_get::<Option<Table>>(scroll_child_key())
                    .map_err(|error| {
                        snapshot_error(format!("object {lua_index} scroll child"), error)
                    })?
                    .map(|child| {
                        let stored = child.raw_get::<usize>(index_key()).map_err(|error| {
                            snapshot_error(
                                format!("object {lua_index} scroll child index"),
                                error,
                            )
                        })?;
                        let index = stored.checked_sub(1).ok_or_else(|| UiScriptError::Plan {
                            message: format!(
                                "live UI object {lua_index} has invalid scroll child index {stored}"
                            ),
                        })?;
                        if index >= object_count || stored == lua_index {
                            return Err(UiScriptError::Plan {
                                message: format!(
                                    "live UI object {lua_index} has invalid scroll child index {stored}"
                                ),
                            });
                        }
                        Ok(index)
                    })
                    .transpose()
            })
            .transpose()?
            .flatten();
        let animation = animation_transforms[stored_index - 1];
        objects.push(UiRuntimeObject {
            name: table
                .raw_get(name_key())
                .map_err(|error| snapshot_error(format!("object {lua_index} name"), error))?,
            kind,
            role,
            parent,
            width: finite_region_number(&table, width_key(), lua_index, "width")?,
            height: finite_region_number(&table, height_key(), lua_index, "height")?,
            shown: table
                .raw_get(shown_key())
                .map_err(|error| snapshot_error(format!("object {lua_index} visibility"), error))?,
            alpha: finite_region_number(&table, alpha_key(), lua_index, "alpha")?,
            scale: positive_region_number(&table, scale_key(), lua_index, "scale")?,
            animation_alpha_delta: animation.alpha_delta,
            animation_offset: animation.offset,
            animation_active: animation.active,
            first_anchor,
            anchor_count: anchors.len() - first_anchor,
            texture,
            text,
            simple_html_text: (kind == UiObjectKind::SimpleHtml)
                .then(|| table.raw_get::<Option<String>>(text_key()))
                .transpose()
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} SimpleHTML text"), error)
                })?
                .flatten(),
            model,
            backdrop_color: is_frame
                .then(|| {
                    snapshot_optional_color(
                        lua_index,
                        &table,
                        backdrop_color_key(),
                        "backdrop color",
                    )
                })
                .transpose()?
                .flatten(),
            backdrop_border_color: is_frame
                .then(|| {
                    snapshot_optional_color(
                        lua_index,
                        &table,
                        backdrop_border_color_key(),
                        "backdrop border color",
                    )
                })
                .transpose()?
                .flatten(),
            frame_level: is_frame
                .then(|| {
                    table.raw_get(frame_level_key()).map_err(|error| {
                        snapshot_error(format!("object {lua_index} frame level"), error)
                    })
                })
                .transpose()?,
            frame_strata: is_frame
                .then(|| snapshot_frame_strata(lua_index, &table))
                .transpose()?,
            keyboard_enabled: is_frame
                .then(|| table.raw_get(keyboard_enabled_key()))
                .transpose()
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} keyboard input"), error)
                })?,
            mouse_enabled: is_frame
                .then(|| table.raw_get(mouse_enabled_key()))
                .transpose()
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} mouse input"), error)
                })?,
            motion_scripts_while_disabled: is_frame
                .then(|| table.raw_get(motion_scripts_while_disabled_key()))
                .transpose()
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} disabled motion scripts"), error)
                })?,
            mouse_wheel_enabled: is_frame
                .then(|| table.raw_get(mouse_wheel_enabled_key()))
                .transpose()
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} mouse wheel input"), error)
                })?,
            hit_rect_insets: is_frame
                .then(|| {
                    table
                        .raw_get::<Table>(hit_rect_insets_key())
                        .map_err(|error| {
                            snapshot_error(format!("object {lua_index} hit rect insets"), error)
                        })
                        .and_then(|values| {
                            numeric_array::<4>(&values, lua_index, "hit rect insets")
                        })
                })
                .transpose()?,
            scroll_offset: (kind == UiObjectKind::ScrollFrame)
                .then(|| {
                    Ok::<(f64, f64), UiScriptError>((
                        finite_region_number(
                            &table,
                            horizontal_scroll_key(),
                            lua_index,
                            "horizontal scroll",
                        )?,
                        finite_region_number(
                            &table,
                            vertical_scroll_key(),
                            lua_index,
                            "vertical scroll",
                        )?,
                    ))
                })
                .transpose()?,
            scroll_range: (kind == UiObjectKind::ScrollFrame)
                .then(|| {
                    Ok::<(f64, f64), UiScriptError>((
                        finite_region_number(
                            &table,
                            horizontal_scroll_range_key(),
                            lua_index,
                            "horizontal scroll range",
                        )?,
                        finite_region_number(
                            &table,
                            vertical_scroll_range_key(),
                            lua_index,
                            "vertical scroll range",
                        )?,
                    ))
                })
                .transpose()?,
            scroll_child,
            status_bar: (kind == UiObjectKind::StatusBar)
                .then(|| super::simple_script::status_bars::snapshot(&table))
                .transpose()
                .map_err(|error| snapshot_error(format!("object {lua_index} status bar"), error))?,
            slider: (kind == UiObjectKind::Slider)
                .then(|| snapshot_slider(lua_index, &table))
                .transpose()?,
            enabled: matches!(
                kind,
                UiObjectKind::Button | UiObjectKind::CheckButton | UiObjectKind::Slider
            )
            .then(|| table.raw_get(enabled_key()))
            .transpose()
            .map_err(|error| snapshot_error(format!("object {lua_index} enabled"), error))?,
            click_action,
            checked: (kind == UiObjectKind::CheckButton)
                .then(|| table.raw_get(checked_key()))
                .transpose()
                .map_err(|error| snapshot_error(format!("object {lua_index} checked"), error))?,
            highlighted: matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton)
                .then(|| {
                    Ok::<bool, mlua::Error>(
                        table.raw_get::<bool>(highlight_locked_key())?
                            || table.raw_get::<bool>(hovered_key())?,
                    )
                })
                .transpose()
                .map_err(|error| snapshot_error(format!("object {lua_index} highlight"), error))?,
            pushed: is_button
                .then(|| table.raw_get::<bool>(button_pressed_key()))
                .transpose()
                .map_err(|error| snapshot_error(format!("object {lua_index} pushed"), error))?,
            edit_focused: (kind == UiObjectKind::EditBox)
                .then(|| table.raw_get::<bool>(edit_focused_key()))
                .transpose()
                .map_err(|error| snapshot_error(format!("object {lua_index} focus"), error))?,
        });
    }

    Ok(UiRuntimeObjectPlan { objects, anchors })
}

/// Updates only values that can change through a classified visual transform tick.
pub(super) fn refresh_runtime_visual_transforms(
    lua: &Lua,
    live: &mut UiRuntimeObjectPlan,
) -> Result<(), UiScriptError> {
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    let animation_transforms = owner_animation_transforms(lua, live.objects.len())
        .map_err(|error| snapshot_error("object animations", error))?;
    for (object_index, (object, animation)) in live
        .objects
        .iter_mut()
        .zip(animation_transforms)
        .enumerate()
    {
        let lua_index = object_index + 1;
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        object.alpha = finite_region_number(&table, alpha_key(), lua_index, "alpha")?;
        object.animation_alpha_delta = animation.alpha_delta;
        object.animation_offset = animation.offset;
        object.animation_active = animation.active;
    }
    Ok(())
}

/// Copies presentation-only region state for objects named by the mutation journal.
pub(super) fn refresh_runtime_visual_objects(
    lua: &Lua,
    live: &mut UiRuntimeObjectPlan,
    object_indices: &[usize],
) -> Result<(), UiScriptError> {
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    for &object_index in object_indices {
        let lua_index = object_index + 1;
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        let alpha = finite_region_number(&table, alpha_key(), lua_index, "alpha")?;
        let shown = table
            .raw_get::<bool>(shown_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} visibility"), error))?;
        if let Some(object) = live.objects.get_mut(object_index) {
            object.alpha = alpha;
            object.shown = shown;
            if let Some(text) = object.text.as_mut()
                && object.kind == UiObjectKind::EditBox
            {
                text.caret_visible =
                    table
                        .raw_get::<bool>(edit_caret_visible_key())
                        .map_err(|error| {
                            snapshot_error(format!("object {lua_index} caret visibility"), error)
                        })?;
            }
        }
    }
    Ok(())
}

/// Tests final layout state after the complete authored callback transaction.
/// Clearing and restoring anchors must remain visible to Lua, but does not
/// invalidate retained geometry when every layout input ends unchanged.
pub(super) fn runtime_layout_journal_is_unchanged(
    lua: &Lua,
    live: &UiRuntimeObjectPlan,
    dirty_objects: &[(usize, u32)],
) -> Result<bool, UiScriptError> {
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    for &(object_index, _) in dirty_objects {
        let lua_index = object_index + 1;
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        let current = live
            .objects
            .get(object_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("dirty UI object {lua_index} is outside the retained arena"),
            })?;
        let layout = snapshot_layout(lua_index, &table, live.objects.len())?;
        if current.width != layout.0
            || current.height != layout.1
            || current.scale != layout.2
            || live.anchors_for(current) != layout.3
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn snapshot_layout(
    lua_index: usize,
    table: &Table,
    object_count: usize,
) -> Result<(f64, f64, f64, Vec<UiRuntimeAnchor>), UiScriptError> {
    let width = finite_region_number(table, width_key(), lua_index, "width")?;
    let height = finite_region_number(table, height_key(), lua_index, "height")?;
    let scale = positive_region_number(table, scale_key(), lua_index, "scale")?;
    let mut anchors = Vec::new();
    snapshot_anchors(lua_index, table, object_count, &mut anchors)?;
    Ok((width, height, scale, anchors))
}

/// Copies only fields named by the per-object mutation journal.
///
/// Object identity and arena topology are validated by the dispatcher before
/// this path is selected. Layout still resolves globally because anchors may
/// target any region, while Lua-to-native copying remains proportional to the
/// actual mutation set.
pub(super) fn refresh_runtime_dirty_objects(
    lua: &Lua,
    live: &mut UiRuntimeObjectPlan,
    dirty_objects: &[(usize, u32)],
) -> Result<Vec<usize>, UiScriptError> {
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    let object_count = live.objects.len();
    let mut text_objects = Vec::new();
    let mut text_button_owners = Vec::new();

    for &(object_index, flags) in dirty_objects {
        let lua_index = object_index + 1;
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        let Some(current) = live.objects.get(object_index) else {
            return Err(UiScriptError::Plan {
                message: format!("dirty UI object {lua_index} is outside the retained arena"),
            });
        };
        let (kind, role, parent) = (current.kind, current.role, current.parent);

        if flags & DIRTY_LAYOUT != 0 {
            let (width, height, scale, anchors) = snapshot_layout(lua_index, &table, object_count)?;
            live.objects[object_index].width = width;
            live.objects[object_index].height = height;
            live.objects[object_index].scale = scale;
            live.replace_anchors(object_index, anchors);
        }
        if flags & (DIRTY_TEXTURE | DIRTY_TEXTURE_VERTEX_COLOR) != 0
            && kind == UiObjectKind::Texture
        {
            live.objects[object_index].texture = Some(snapshot_texture(lua_index, &table)?);
        }
        if flags & DIRTY_MODEL != 0
            && matches!(
                kind,
                UiObjectKind::Model
                    | UiObjectKind::PlayerModel
                    | UiObjectKind::DressUpModel
                    | UiObjectKind::TabardModel
                    | UiObjectKind::ModelFfx
            )
        {
            live.objects[object_index].model = Some(snapshot_model(lua_index, &table)?);
        }
        if flags & DIRTY_FRAME != 0 {
            live.objects[object_index].backdrop_color =
                snapshot_optional_color(lua_index, &table, backdrop_color_key(), "backdrop color")?;
            live.objects[object_index].backdrop_border_color = snapshot_optional_color(
                lua_index,
                &table,
                backdrop_border_color_key(),
                "backdrop border color",
            )?;
        }
        if flags & DIRTY_WIDGET != 0 {
            let is_button = matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton);
            if kind == UiObjectKind::ScrollFrame {
                let offset = (
                    finite_region_number(
                        &table,
                        horizontal_scroll_key(),
                        lua_index,
                        "horizontal scroll",
                    )?,
                    finite_region_number(
                        &table,
                        vertical_scroll_key(),
                        lua_index,
                        "vertical scroll",
                    )?,
                );
                let range = (
                    finite_region_number(
                        &table,
                        horizontal_scroll_range_key(),
                        lua_index,
                        "horizontal scroll range",
                    )?,
                    finite_region_number(
                        &table,
                        vertical_scroll_range_key(),
                        lua_index,
                        "vertical scroll range",
                    )?,
                );
                live.replace_scroll_state(object_index, offset, range);
            }
            if matches!(
                kind,
                UiObjectKind::Button | UiObjectKind::CheckButton | UiObjectKind::Slider
            ) {
                live.objects[object_index].enabled =
                    Some(table.raw_get(enabled_key()).map_err(|error| {
                        snapshot_error(format!("object {lua_index} enabled"), error)
                    })?);
            }
            if kind == UiObjectKind::StatusBar {
                live.objects[object_index].status_bar = Some(
                    super::simple_script::status_bars::snapshot(&table).map_err(|error| {
                        snapshot_error(format!("object {lua_index} status bar"), error)
                    })?,
                );
            }
            if kind == UiObjectKind::Slider {
                live.objects[object_index].slider = Some(snapshot_slider(lua_index, &table)?);
            }
            if is_button {
                live.objects[object_index].click_action =
                    Some(table.raw_get(click_action_key()).map_err(|error| {
                        snapshot_error(format!("object {lua_index} click action"), error)
                    })?);
                live.objects[object_index].highlighted = Some(
                    table
                        .raw_get::<bool>(highlight_locked_key())
                        .and_then(|locked| {
                            table
                                .raw_get::<bool>(hovered_key())
                                .map(|hovered| locked || hovered)
                        })
                        .map_err(|error| {
                            snapshot_error(format!("object {lua_index} highlight"), error)
                        })?,
                );
                live.objects[object_index].pushed =
                    Some(table.raw_get(button_pressed_key()).map_err(|error| {
                        snapshot_error(format!("object {lua_index} pushed"), error)
                    })?);
                if kind == UiObjectKind::CheckButton {
                    live.objects[object_index].checked =
                        Some(table.raw_get(checked_key()).map_err(|error| {
                            snapshot_error(format!("object {lua_index} checked"), error)
                        })?);
                }
                text_button_owners.push(object_index);
            }
        }
        if flags & DIRTY_TEXT != 0 {
            if matches!(
                kind,
                UiObjectKind::FontString
                    | UiObjectKind::EditBox
                    | UiObjectKind::ScrollingMessageFrame
            ) {
                let (presentation_font, presentation_color) =
                    button_presentation_font(&registry, role, parent)?;
                live.objects[object_index].width =
                    finite_region_number(&table, width_key(), lua_index, "width")?;
                live.objects[object_index].height =
                    finite_region_number(&table, height_key(), lua_index, "height")?;
                live.objects[object_index].text = snapshot_text(
                    lua_index,
                    kind,
                    &table,
                    presentation_font.as_ref(),
                    presentation_color,
                )?;
                if kind == UiObjectKind::EditBox {
                    live.objects[object_index].edit_focused =
                        Some(table.raw_get(edit_focused_key()).map_err(|error| {
                            snapshot_error(format!("object {lua_index} focus"), error)
                        })?);
                }
                text_objects.push(object_index);
            } else if matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton) {
                text_button_owners.push(object_index);
            }
        }
    }

    text_button_owners.sort_unstable();
    text_button_owners.dedup();
    if !text_button_owners.is_empty() {
        for object_index in 0..live.objects.len() {
            let (kind, role, parent) = {
                let object = &live.objects[object_index];
                (object.kind, object.role, object.parent)
            };
            if role != UiObjectRole::ButtonText
                || parent.is_none_or(|owner| text_button_owners.binary_search(&owner).is_err())
            {
                continue;
            }
            let lua_index = object_index + 1;
            let table: Table = registry
                .raw_get(lua_index)
                .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
            let (presentation_font, presentation_color) =
                button_presentation_font(&registry, role, parent)?;
            live.objects[object_index].text = snapshot_text(
                lua_index,
                kind,
                &table,
                presentation_font.as_ref(),
                presentation_color,
            )?;
            text_objects.push(object_index);
        }
    }
    text_objects.sort_unstable();
    text_objects.dedup();
    Ok(text_objects)
}

/// Copies only the native hover/locked highlight selector for named buttons.
pub(super) fn refresh_runtime_button_highlights(
    lua: &Lua,
    live: &mut UiRuntimeObjectPlan,
    object_indices: impl IntoIterator<Item = usize>,
) -> Result<(), UiScriptError> {
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    for object_index in object_indices {
        let lua_index = object_index + 1;
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        let highlighted = table
            .raw_get::<bool>(highlight_locked_key())
            .and_then(|locked| {
                table
                    .raw_get::<bool>(hovered_key())
                    .map(|hovered| locked || hovered)
            })
            .map_err(|error| snapshot_error(format!("object {lua_index} highlight"), error))?;
        live.replace_button_highlight(object_index, highlighted);
    }
    Ok(())
}

/// Copies only the active presentation font for labels owned by named buttons.
pub(super) fn refresh_runtime_button_texts(
    lua: &Lua,
    live: &mut UiRuntimeObjectPlan,
    button_indices: impl IntoIterator<Item = usize>,
) -> Result<(Vec<usize>, Vec<UiRuntimeTextColorChange>), UiScriptError> {
    let mut button_indices = button_indices.into_iter().collect::<Vec<_>>();
    if button_indices.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    button_indices.sort_unstable();
    button_indices.dedup();
    let registry: Table = lua
        .named_registry_value(OBJECT_REGISTRY)
        .map_err(|error| snapshot_error("object registry", error))?;
    let mut layout_changes = Vec::new();
    let mut color_changes = Vec::new();
    for object_index in 0..live.objects.len() {
        let (kind, role, parent) = {
            let object = &live.objects[object_index];
            (object.kind, object.role, object.parent)
        };
        if role != UiObjectRole::ButtonText
            || parent.is_none_or(|parent| button_indices.binary_search(&parent).is_err())
        {
            continue;
        }
        let lua_index = object_index + 1;
        let table: Table = registry
            .raw_get(lua_index)
            .map_err(|error| snapshot_error(format!("object {lua_index}"), error))?;
        let (presentation_font, presentation_color) =
            button_presentation_font(&registry, role, parent)?;
        let text = snapshot_text(
            lua_index,
            kind,
            &table,
            presentation_font.as_ref(),
            presentation_color,
        )?;
        if live.objects[object_index].text != text {
            match (&live.objects[object_index].text, &text) {
                (Some(previous), Some(current)) => {
                    let mut normalized = current.clone();
                    normalized.color = previous.color;
                    normalized.shadow_color = previous.shadow_color;
                    if normalized == *previous {
                        color_changes.push(UiRuntimeTextColorChange {
                            object_index,
                            previous_color: previous.color,
                            color: current.color,
                            previous_shadow_color: previous.shadow_color,
                            shadow_color: current.shadow_color,
                        });
                    } else {
                        layout_changes.push(object_index);
                    }
                }
                _ => layout_changes.push(object_index),
            }
        }
        live.objects[object_index].text = text;
    }
    Ok((layout_changes, color_changes))
}

pub(super) fn snapshot_slider(
    lua_index: usize,
    table: &Table,
) -> Result<UiRuntimeSlider, UiScriptError> {
    let minimum = finite_region_number(table, slider_min_key(), lua_index, "slider minimum")?;
    let maximum = finite_region_number(table, slider_max_key(), lua_index, "slider maximum")?;
    let value = finite_region_number(table, slider_value_key(), lua_index, "slider value")?;
    let step = finite_region_number(table, slider_step_key(), lua_index, "slider step")?;
    let orientation = table
        .raw_get::<String>(slider_orientation_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} slider orientation"), error))?;
    let vertical = match orientation.as_str() {
        "VERTICAL" => true,
        "HORIZONTAL" => false,
        _ => {
            return Err(UiScriptError::Plan {
                message: format!(
                    "live Slider object {lua_index} has invalid orientation {orientation}"
                ),
            });
        }
    };
    if minimum > maximum || value < minimum || value > maximum || step < 0.0 {
        return Err(UiScriptError::Plan {
            message: format!("live Slider object {lua_index} has invalid range state"),
        });
    }
    Ok(UiRuntimeSlider {
        minimum,
        maximum,
        value,
        step,
        vertical,
    })
}

fn snapshot_optional_color(
    lua_index: usize,
    table: &Table,
    key: mlua::LightUserData,
    label: &str,
) -> Result<Option<[f64; 4]>, UiScriptError> {
    table
        .raw_get::<Option<Table>>(key)
        .map_err(|error| snapshot_error(format!("object {lua_index} {label}"), error))?
        .map(|values| numeric_array::<4>(&values, lua_index, label))
        .transpose()
}

/// Selects the stock Button font role used to present its owned label. The
/// FontString keeps its layout policy, while this object supplies the face,
/// size, color, shadow, rasterization flags, and spacing.
fn button_presentation_font(
    registry: &Table,
    role: UiObjectRole,
    parent: Option<usize>,
) -> Result<(Option<Table>, Option<[f64; 4]>), UiScriptError> {
    if role != UiObjectRole::ButtonText {
        return Ok((None, None));
    }
    let Some(parent) = parent else {
        return Ok((None, None));
    };
    let lua_index = parent + 1;
    let button: Table = registry
        .raw_get(lua_index)
        .map_err(|error| snapshot_error(format!("button owner {lua_index}"), error))?;
    let kind = button
        .raw_get::<String>(type_key())
        .map_err(|error| snapshot_error(format!("button owner {lua_index} type"), error))?;
    if !matches!(kind.as_str(), "Button" | "CheckButton") {
        return Ok((None, None));
    }
    let enabled = button
        .raw_get::<bool>(enabled_key())
        .map_err(|error| snapshot_error(format!("button owner {lua_index} enabled"), error))?;
    let highlighted = button
        .raw_get::<bool>(hovered_key())
        .and_then(|hovered| {
            button
                .raw_get::<bool>(highlight_locked_key())
                .map(|locked| hovered || locked)
        })
        .map_err(|error| snapshot_error(format!("button owner {lua_index} highlight"), error))?;
    let disabled_color = if !enabled {
        button
            .raw_get::<Option<Table>>(disabled_text_color_key())
            .map_err(|error| {
                snapshot_error(
                    format!("button owner {lua_index} disabled text color"),
                    error,
                )
            })?
            .map(|values| numeric_array::<4>(&values, lua_index, "disabled text color"))
            .transpose()?
    } else {
        None
    };
    if !enabled {
        let disabled = button
            .raw_get::<Option<Table>>(disabled_font_key())
            .map_err(|error| snapshot_error(format!("button owner {lua_index} font"), error))?;
        if disabled.is_some() {
            return Ok((disabled, disabled_color));
        }
    }
    if highlighted {
        let highlight = button
            .raw_get::<Option<Table>>(highlight_font_key())
            .map_err(|error| snapshot_error(format!("button owner {lua_index} font"), error))?;
        if highlight.is_some() {
            return Ok((highlight, None));
        }
    }
    let font = button
        .raw_get::<Option<Table>>(normal_font_key())
        .map_err(|error| snapshot_error(format!("button owner {lua_index} font"), error))?;
    Ok((font, disabled_color))
}

fn snapshot_text(
    lua_index: usize,
    kind: UiObjectKind,
    table: &Table,
    presentation_font: Option<&Table>,
    presentation_color: Option<[f64; 4]>,
) -> Result<Option<UiRuntimeText>, UiScriptError> {
    let font_set = table
        .raw_get::<bool>(font_set_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font assignment"), error))?;
    if !font_set {
        return Ok(None);
    }
    let Some(local_font) = table
        .raw_get::<Option<Table>>(font_object_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font object"), error))?
    else {
        return Ok(None);
    };
    let font = presentation_font.unwrap_or(&local_font);
    let Some(face) = font
        .raw_get::<Option<String>>(font_face_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font face"), error))?
    else {
        return Ok(None);
    };
    let face = AssetPath::new(&face).map_err(|error| UiScriptError::Plan {
        message: format!("live UI text {lua_index} has invalid font face: {error}"),
    })?;
    let Some(height) = font
        .raw_get::<Option<f64>>(font_height_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font height"), error))?
    else {
        return Ok(None);
    };
    if !height.is_finite() || height <= 0.0 {
        return Err(UiScriptError::Plan {
            message: format!("live UI text {lua_index} has invalid font height {height}"),
        });
    }
    let flags = font
        .raw_get::<String>(font_flags_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font flags"), error))?;
    let content = table
        .raw_get::<Option<String>>(text_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} text"), error))?
        .unwrap_or_default();
    let content_len = content.len();
    let horizontal = table.raw_get::<String>(justify_h_key()).map_err(|error| {
        snapshot_error(format!("object {lua_index} horizontal alignment"), error)
    })?;
    let vertical = table
        .raw_get::<String>(justify_v_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} vertical alignment"), error))?;
    let draw_layer = table
        .raw_get::<String>(draw_layer_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} text draw layer"), error))?;
    let style = presentation_font.unwrap_or(table);
    let color: Table = style
        .raw_get(text_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} text color"), error))?;
    let shadow_offset: Table = style
        .raw_get(font_shadow_offset_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} shadow offset"), error))?;
    let shadow_color: Table = style
        .raw_get(font_shadow_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} shadow color"), error))?;
    // Build 12340's CSimpleFontString presentation callback applies the
    // Region's explicit vertex color when its presence flag is set and uses
    // opaque white when it is absent. Stock options code relies on this to
    // replace an inherited gold font with white High/Low slider labels.
    let vertex_color = table
        .raw_get::<bool>(vertex_color_set_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} vertex color flag"), error))?
        .then(|| {
            table
                .raw_get::<Table>(texture_color_key())
                .map_err(|error| snapshot_error(format!("object {lua_index} vertex color"), error))
                .and_then(|values| numeric_array::<4>(&values, lua_index, "vertex color"))
        })
        .transpose()?;
    let is_edit_box = kind == UiObjectKind::EditBox;
    let text_insets = if is_edit_box {
        let values: Table = table
            .raw_get(edit_text_insets_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} text insets"), error))?;
        numeric_array::<4>(&values, lua_index, "text insets")?
    } else {
        [0.0; 4]
    };
    let cursor = if is_edit_box {
        table
            .raw_get::<u32>(edit_cursor_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} cursor"), error))?
            as usize
    } else {
        0
    };
    let selection = if is_edit_box {
        [
            table
                .raw_get::<u32>(edit_selection_start_key())
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} selection start"), error)
                })? as usize,
            table
                .raw_get::<u32>(edit_selection_end_key())
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} selection end"), error)
                })? as usize,
        ]
    } else {
        [0, 0]
    };
    Ok(Some(UiRuntimeText {
        content,
        face,
        height,
        rasterization: if flags
            .split(',')
            .any(|flag| flag.trim().eq_ignore_ascii_case("MONOCHROME"))
        {
            FontRasterization::Monochrome
        } else {
            FontRasterization::Antialiased
        },
        outline_width: if flags
            .split(',')
            .any(|flag| flag.trim().eq_ignore_ascii_case("THICKOUTLINE"))
        {
            2.0
        } else if flags
            .split(',')
            .any(|flag| flag.trim().eq_ignore_ascii_case("OUTLINE"))
        {
            1.0
        } else {
            0.0
        },
        color: vertex_color.unwrap_or(
            presentation_color
                .map_or_else(|| numeric_array::<4>(&color, lua_index, "text color"), Ok)?,
        ),
        shadow_offset: numeric_array::<2>(&shadow_offset, lua_index, "shadow offset")?,
        shadow_color: numeric_array::<4>(&shadow_color, lua_index, "shadow color")?,
        spacing: finite_region_number(table, spacing_key(), lua_index, "font spacing")?,
        word_wrap: table
            .raw_get(word_wrap_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} word wrap"), error))?,
        non_space_wrap: table
            .raw_get(non_space_wrap_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} non-space wrap"), error))?,
        max_lines: table
            .raw_get(max_text_lines_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} maximum lines"), error))?,
        max_letters: if is_edit_box {
            table.raw_get(edit_max_letters_key()).map_err(|error| {
                snapshot_error(format!("object {lua_index} maximum letters"), error)
            })?
        } else {
            0
        },
        horizontal: parse_horizontal_justification(&horizontal).ok_or_else(|| {
            UiScriptError::Plan {
                message: format!(
                    "live UI text {lua_index} has unknown horizontal alignment {horizontal}"
                ),
            }
        })?,
        vertical: parse_vertical_justification(&vertical).ok_or_else(|| UiScriptError::Plan {
            message: format!("live UI text {lua_index} has unknown vertical alignment {vertical}"),
        })?,
        draw_layer: parse_draw_layer(&draw_layer).ok_or_else(|| UiScriptError::Plan {
            message: format!("live UI text {lua_index} has unknown draw layer {draw_layer}"),
        })?,
        draw_sub_level: table
            .raw_get(draw_sub_level_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} text sublevel"), error))?,
        password: is_edit_box
            && table
                .raw_get::<bool>(edit_password_key())
                .map_err(|error| snapshot_error(format!("object {lua_index} password"), error))?,
        multiline: is_edit_box
            && table
                .raw_get::<bool>(edit_multi_line_key())
                .map_err(|error| snapshot_error(format!("object {lua_index} multiline"), error))?,
        text_insets,
        cursor: cursor.min(content_len),
        selection: selection.map(|offset| offset.min(content_len)),
        caret_visible: !is_edit_box
            || table
                .raw_get::<bool>(edit_caret_visible_key())
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} caret visibility"), error)
                })?,
        highlight_color: if is_edit_box {
            let values = table
                .raw_get::<Table>(edit_highlight_color_key())
                .map_err(|error| {
                    snapshot_error(format!("object {lua_index} highlight color"), error)
                })?;
            numeric_array::<4>(&values, lua_index, "highlight color")?
        } else {
            [0.0; 4]
        },
    }))
}

fn snapshot_model(lua_index: usize, table: &Table) -> Result<UiRuntimeModel, UiScriptError> {
    let file = table
        .raw_get::<Option<String>>(model_file_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} model file"), error))?
        .map(|value| AssetPath::new(&value))
        .transpose()
        .map_err(|error| UiScriptError::Plan {
            message: format!("live UI model {lua_index} has invalid file: {error}"),
        })?;
    let scale = finite_region_number(table, model_scale_key(), lua_index, "model scale")?;
    if scale <= 0.0 {
        return Err(UiScriptError::Plan {
            message: format!("live UI model {lua_index} has nonpositive model scale"),
        });
    }
    let fog_color = table
        .raw_get::<Option<Table>>(model_fog_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} model fog color"), error))?
        .map(|color| numeric_array::<3>(&color, lua_index, "model fog color"))
        .transpose()?;
    let fog_near = finite_region_number(table, model_fog_near_key(), lua_index, "model fog near")?;
    let fog_far = finite_region_number(table, model_fog_far_key(), lua_index, "model fog far")?;
    if fog_color.is_some() && fog_far < fog_near {
        return Err(UiScriptError::Plan {
            message: format!(
                "live UI model {lua_index} has fog far {fog_far} before near {fog_near}"
            ),
        });
    }
    Ok(UiRuntimeModel {
        file,
        instance_generation: table
            .raw_get(model_instance_generation_key())
            .map_err(|error| {
                snapshot_error(
                    format!("object {lua_index} model instance generation"),
                    error,
                )
            })?,
        unit: table
            .raw_get(model_unit_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} model unit"), error))?,
        rotation_radians: finite_region_number(
            table,
            model_rotation_key(),
            lua_index,
            "model rotation",
        )?,
        camera: table
            .raw_get(model_camera_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} model camera"), error))?,
        sequence: table
            .raw_get(model_sequence_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} model sequence"), error))?,
        sequence_time_sequence: table.raw_get(model_sequence_time_sequence_key()).map_err(
            |error| {
                snapshot_error(
                    format!("object {lua_index} model sequence-time sequence"),
                    error,
                )
            },
        )?,
        sequence_time_ms: table.raw_get(model_sequence_time_key()).map_err(|error| {
            snapshot_error(format!("object {lua_index} model sequence time"), error)
        })?,
        scale,
        fog_color,
        fog_near,
        fog_far,
        glow: finite_region_number(table, model_glow_key(), lua_index, "model glow")?,
        background_lights: snapshot_model_light_sets(
            table,
            model_background_light_live_key(),
            model_background_light_ghost_key(),
            lua_index,
            "background",
        )?,
        character_lights: snapshot_model_light_sets(
            table,
            model_character_light_live_key(),
            model_character_light_ghost_key(),
            lua_index,
            "character",
        )?,
        pet_lights: snapshot_model_light_sets(
            table,
            model_pet_light_live_key(),
            model_pet_light_ghost_key(),
            lua_index,
            "pet",
        )?,
    })
}

fn snapshot_model_light_sets(
    table: &Table,
    live_key: mlua::LightUserData,
    ghost_key: mlua::LightUserData,
    lua_index: usize,
    label: &str,
) -> Result<UiRuntimeModelLightSets, UiScriptError> {
    Ok(UiRuntimeModelLightSets {
        live: snapshot_model_lights(table, live_key, lua_index, label, "live")?,
        ghost: snapshot_model_lights(table, ghost_key, lua_index, label, "ghost")?,
    })
}

fn snapshot_model_lights(
    table: &Table,
    key: mlua::LightUserData,
    lua_index: usize,
    label: &str,
    state: &str,
) -> Result<[Option<UiRuntimeModelLight>; 4], UiScriptError> {
    let Some(lights) = table.raw_get::<Option<Table>>(key).map_err(|error| {
        snapshot_error(format!("object {lua_index} {label} {state} lights"), error)
    })?
    else {
        return Ok([None; 4]);
    };
    let count = lights.raw_len();
    if count > 4 {
        return Err(UiScriptError::Plan {
            message: format!(
                "live UI model {lua_index} {label} {state} light count {count} exceeds four"
            ),
        });
    }
    let mut result = [None; 4];
    for (index, slot) in result.iter_mut().take(count).enumerate() {
        let light = lights.raw_get::<Table>(index + 1).map_err(|error| {
            snapshot_error(
                format!("object {lua_index} {label} {state} light {index}"),
                error,
            )
        })?;
        let values = numeric_array::<9>(&light, lua_index, "model light")?;
        *slot = Some(UiRuntimeModelLight {
            direction: [values[0], values[1], values[2]],
            ambient: [values[3], values[4], values[5]],
            diffuse: [values[6], values[7], values[8]],
        });
    }
    Ok(result)
}

fn snapshot_texture(lua_index: usize, table: &Table) -> Result<UiRuntimeTexture, UiScriptError> {
    let file = table
        .raw_get::<Option<String>>(texture_file_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} texture file"), error))?
        .map(|value| crate::widget::canonical_texture_asset(&value))
        .transpose()
        .map_err(|error| UiScriptError::Plan {
            message: format!("live UI texture {lua_index} has invalid file: {error}"),
        })?;
    let solid_color = table
        .raw_get::<Option<Table>>(texture_solid_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} solid color"), error))?
        .map(|values| numeric_array::<4>(&values, lua_index, "solid color"))
        .transpose()?;
    if file.is_some() && solid_color.is_some() {
        return Err(UiScriptError::Plan {
            message: format!("live UI texture {lua_index} has two material sources"),
        });
    }
    let blend_mode = table
        .raw_get::<String>(texture_blend_mode_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} blend mode"), error))?;
    let draw_layer = table
        .raw_get::<String>(draw_layer_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} draw layer"), error))?;
    let coords: Table = table.raw_get(tex_coord_key()).map_err(|error| {
        snapshot_error(format!("object {lua_index} texture coordinates"), error)
    })?;
    let colors: Table = table
        .raw_get(texture_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} vertex colors"), error))?;
    let flat_colors = numeric_array::<16>(&colors, lua_index, "vertex colors")?;
    Ok(UiRuntimeTexture {
        portrait_unit: table
            .raw_get(portrait_unit_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} portrait unit"), error))?,
        file,
        solid_color,
        blend_mode: parse_blend_mode(&blend_mode).ok_or_else(|| UiScriptError::Plan {
            message: format!("live UI texture {lua_index} has unknown blend mode {blend_mode}"),
        })?,
        tex_coords: numeric_array::<8>(&coords, lua_index, "texture coordinates")?,
        vertex_colors: std::array::from_fn(|corner| {
            std::array::from_fn(|channel| flat_colors[corner * 4 + channel])
        }),
        horizontal_tiling: table.raw_get(horizontal_tiling_key()).map_err(|error| {
            snapshot_error(format!("object {lua_index} horizontal tiling"), error)
        })?,
        vertical_tiling: table.raw_get(vertical_tiling_key()).map_err(|error| {
            snapshot_error(format!("object {lua_index} vertical tiling"), error)
        })?,
        non_blocking: table
            .raw_get(non_blocking_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} nonblocking"), error))?,
        desaturated: table
            .raw_get(desaturated_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} desaturated"), error))?,
        draw_layer: parse_draw_layer(&draw_layer).ok_or_else(|| UiScriptError::Plan {
            message: format!("live UI texture {lua_index} has unknown draw layer {draw_layer}"),
        })?,
        draw_sub_level: table
            .raw_get(draw_sub_level_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} draw sublevel"), error))?,
    })
}

fn numeric_array<const LENGTH: usize>(
    table: &Table,
    lua_index: usize,
    field: &str,
) -> Result<[f64; LENGTH], UiScriptError> {
    let values: [f64; LENGTH] = (0..LENGTH)
        .map(|index| {
            table.raw_get::<f64>(index + 1).map_err(|error| {
                snapshot_error(format!("object {lua_index} {field} component"), error)
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| UiScriptError::Plan {
            message: format!("live UI object {lua_index} has malformed {field}"),
        })?;
    if values.iter().any(|value| !value.is_finite()) {
        return Err(UiScriptError::Plan {
            message: format!("live UI object {lua_index} has non-finite {field}"),
        });
    }
    Ok(values)
}

fn parse_blend_mode(value: &str) -> Option<UiBlendMode> {
    match value {
        "BLEND" => Some(UiBlendMode::Blend),
        "ADD" => Some(UiBlendMode::Add),
        _ => None,
    }
}

fn parse_draw_layer(value: &str) -> Option<UiDrawLayer> {
    match value {
        "BACKGROUND" => Some(UiDrawLayer::Background),
        "BORDER" => Some(UiDrawLayer::Border),
        "ARTWORK" => Some(UiDrawLayer::Artwork),
        "OVERLAY" => Some(UiDrawLayer::Overlay),
        "HIGHLIGHT" => Some(UiDrawLayer::Highlight),
        _ => None,
    }
}

fn parse_horizontal_justification(value: &str) -> Option<HorizontalJustification> {
    match value {
        "LEFT" => Some(HorizontalJustification::Left),
        "CENTER" => Some(HorizontalJustification::Center),
        "RIGHT" => Some(HorizontalJustification::Right),
        _ => None,
    }
}

fn parse_vertical_justification(value: &str) -> Option<VerticalJustification> {
    match value {
        "TOP" => Some(VerticalJustification::Top),
        "MIDDLE" => Some(VerticalJustification::Middle),
        "BOTTOM" => Some(VerticalJustification::Bottom),
        _ => None,
    }
}

fn snapshot_frame_strata(lua_index: usize, table: &Table) -> Result<UiFrameStrata, UiScriptError> {
    let value = table
        .raw_get::<String>(frame_strata_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} frame strata"), error))?;
    match value.as_str() {
        "BACKGROUND" => Ok(UiFrameStrata::Background),
        "LOW" => Ok(UiFrameStrata::Low),
        "MEDIUM" => Ok(UiFrameStrata::Medium),
        "HIGH" => Ok(UiFrameStrata::High),
        "DIALOG" => Ok(UiFrameStrata::Dialog),
        "FULLSCREEN" => Ok(UiFrameStrata::Fullscreen),
        "FULLSCREEN_DIALOG" => Ok(UiFrameStrata::FullscreenDialog),
        "TOOLTIP" => Ok(UiFrameStrata::Tooltip),
        _ => Err(UiScriptError::Plan {
            message: format!("live UI object {lua_index} has unknown frame strata {value}"),
        }),
    }
}

fn positive_region_number(
    table: &Table,
    key: mlua::LightUserData,
    lua_index: usize,
    field: &str,
) -> Result<f64, UiScriptError> {
    let value = finite_region_number(table, key, lua_index, field)?;
    if value <= 0.0 {
        return Err(UiScriptError::Plan {
            message: format!("live UI object {lua_index} has nonpositive {field}"),
        });
    }
    Ok(value)
}

fn snapshot_anchors(
    lua_index: usize,
    object: &Table,
    object_count: usize,
    output: &mut Vec<UiRuntimeAnchor>,
) -> Result<(), UiScriptError> {
    let records: Table = object
        .raw_get(anchors_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} anchors"), error))?;
    for point_index in 1..=9 {
        let Some(record) = records
            .raw_get::<Option<Table>>(point_index)
            .map_err(|error| snapshot_error(format!("object {lua_index} anchor"), error))?
        else {
            continue;
        };
        let point_name = record
            .raw_get::<String>(1)
            .map_err(|error| snapshot_error(format!("object {lua_index} anchor point"), error))?;
        let point = parse_point(&point_name).ok_or_else(|| UiScriptError::Plan {
            message: format!("live UI object {lua_index} has unknown anchor point {point_name}"),
        })?;
        let target = record
            .raw_get::<Option<usize>>(2)
            .map_err(|error| snapshot_error(format!("object {lua_index} anchor target"), error))?
            .map(|index| index.saturating_sub(1));
        if target.is_some_and(|index| index >= object_count || index + 1 == lua_index) {
            return Err(UiScriptError::Plan {
                message: format!("live UI object {lua_index} has invalid anchor target"),
            });
        }
        let relative_name = record.raw_get::<String>(3).map_err(|error| {
            snapshot_error(format!("object {lua_index} relative anchor point"), error)
        })?;
        let relative_point = parse_point(&relative_name).ok_or_else(|| UiScriptError::Plan {
            message: format!(
                "live UI object {lua_index} has unknown relative point {relative_name}"
            ),
        })?;
        let offset = (
            record
                .raw_get::<f64>(4)
                .map_err(|error| snapshot_error(format!("object {lua_index} anchor x"), error))?,
            record
                .raw_get::<f64>(5)
                .map_err(|error| snapshot_error(format!("object {lua_index} anchor y"), error))?,
        );
        if !offset.0.is_finite() || !offset.1.is_finite() {
            return Err(UiScriptError::Plan {
                message: format!("live UI object {lua_index} has non-finite anchor offset"),
            });
        }
        output.push(UiRuntimeAnchor {
            point,
            target,
            relative_point,
            offset,
        });
    }
    Ok(())
}

pub(super) fn finite_region_number(
    table: &Table,
    key: mlua::LightUserData,
    lua_index: usize,
    field: &str,
) -> Result<f64, UiScriptError> {
    let value = table
        .raw_get::<f64>(key)
        .map_err(|error| snapshot_error(format!("object {lua_index} {field}"), error))?;
    if !value.is_finite() {
        let object_name = table
            .raw_get::<Option<String>>(name_key())
            .map_err(|error| snapshot_error(format!("object {lua_index} name"), error))?
            .unwrap_or_else(|| "<unnamed>".to_owned());
        return Err(UiScriptError::Plan {
            message: format!("live UI object {lua_index} ({object_name}) has non-finite {field}"),
        });
    }
    Ok(value)
}

fn parse_role(value: &str) -> Option<UiObjectRole> {
    match value {
        "object" => Some(UiObjectRole::Object),
        "scroll_child" => Some(UiObjectRole::ScrollChild),
        "button_text" => Some(UiObjectRole::ButtonText),
        "normal_texture" => Some(UiObjectRole::NormalTexture),
        "pushed_texture" => Some(UiObjectRole::PushedTexture),
        "disabled_texture" => Some(UiObjectRole::DisabledTexture),
        "highlight_texture" => Some(UiObjectRole::HighlightTexture),
        "checked_texture" => Some(UiObjectRole::CheckedTexture),
        "disabled_checked_texture" => Some(UiObjectRole::DisabledCheckedTexture),
        "thumb_texture" => Some(UiObjectRole::ThumbTexture),
        "bar_texture" => Some(UiObjectRole::BarTexture),
        _ => None,
    }
}

fn snapshot_error(label: impl Into<String>, error: mlua::Error) -> UiScriptError {
    UiScriptError::Execution {
        label: format!("{} snapshot", label.into()),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn font_string_snapshot_uses_live_and_explicit_region_colors() -> Result<(), Box<dyn Error>> {
        let lua = Lua::new();
        let font = lua.create_table()?;
        font.raw_set(font_face_key(), "Fonts\\FRIZQT__.TTF")?;
        font.raw_set(font_height_key(), 10.0)?;
        font.raw_set(font_flags_key(), "")?;
        font.raw_set(
            text_color_key(),
            lua.create_sequence_from([1.0, 0.82, 0.0, 1.0])?,
        )?;
        font.raw_set(
            font_shadow_offset_key(),
            lua.create_sequence_from([0.0, 0.0])?,
        )?;
        font.raw_set(
            font_shadow_color_key(),
            lua.create_sequence_from([0.0, 0.0, 0.0, 1.0])?,
        )?;

        let object = lua.create_table()?;
        object.raw_set(font_set_key(), true)?;
        object.raw_set(font_object_key(), font)?;
        object.raw_set(text_key(), "High")?;
        object.raw_set(justify_h_key(), "CENTER")?;
        object.raw_set(justify_v_key(), "MIDDLE")?;
        object.raw_set(draw_layer_key(), "ARTWORK")?;
        object.raw_set(draw_sub_level_key(), 0_i16)?;
        object.raw_set(spacing_key(), 0.0)?;
        object.raw_set(word_wrap_key(), true)?;
        object.raw_set(non_space_wrap_key(), false)?;
        object.raw_set(max_text_lines_key(), 0_u32)?;
        object.raw_set(
            text_color_key(),
            lua.create_sequence_from([0.25, 0.5, 0.75, 0.8])?,
        )?;
        object.raw_set(
            font_shadow_offset_key(),
            lua.create_sequence_from([1.0, -1.0])?,
        )?;
        object.raw_set(
            font_shadow_color_key(),
            lua.create_sequence_from([0.1, 0.2, 0.3, 0.4])?,
        )?;
        object.raw_set(
            texture_color_key(),
            lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
        )?;
        object.raw_set(vertex_color_set_key(), false)?;

        let live = snapshot_text(1, UiObjectKind::FontString, &object, None, None)?
            .ok_or("test font string did not snapshot")?;
        assert_eq!(live.color, [0.25, 0.5, 0.75, 0.8]);
        assert_eq!(live.shadow_offset, [1.0, -1.0]);
        assert_eq!(live.shadow_color, [0.1, 0.2, 0.3, 0.4]);

        object.raw_set(vertex_color_set_key(), true)?;
        let live = snapshot_text(1, UiObjectKind::FontString, &object, None, None)?
            .ok_or("test font string did not snapshot")?;
        assert_eq!(live.color, [1.0; 4]);
        Ok(())
    }
}

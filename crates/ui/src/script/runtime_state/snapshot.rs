//! Full Lua-arena snapshots shared by synchronous refresh and covered startup.

use super::{
    UiRuntimeObject, UiRuntimeObjectPlan, button_presentation_font, finite_region_number,
    numeric_array, parse_role, positive_region_number, snapshot_anchors, snapshot_clamp_insets,
    snapshot_error, snapshot_frame_strata, snapshot_model, snapshot_optional_color,
    snapshot_slider, snapshot_text, snapshot_texture,
};
use crate::animation::owner_animation_transforms;
use crate::script::simple_script::{
    OBJECT_REGISTRY, alpha_key, backdrop_border_color_key, backdrop_color_key, button_pressed_key,
    checked_key, click_action_key, edit_focused_key, enabled_key, frame_level_key, height_key,
    highlight_locked_key, hit_rect_insets_key, horizontal_scroll_key, horizontal_scroll_range_key,
    hovered_key, index_key, keyboard_enabled_key, motion_scripts_while_disabled_key,
    mouse_enabled_key, mouse_wheel_enabled_key, name_key, parent_key, role_key, scale_key,
    scroll_child_key, shown_key, text_key, type_key, vertical_scroll_key,
    vertical_scroll_range_key, width_key,
};
use crate::startup::{StartupBudget, complete_unyielding};
use crate::{UiObjectKind, UiScriptError};
use mlua::{Lua, Table};

/// Preserves the synchronous live-refresh API without allocating a task owner.
pub(in crate::script) fn snapshot_runtime_objects(
    lua: &Lua,
    object_count: usize,
) -> Result<UiRuntimeObjectPlan, UiScriptError> {
    complete_unyielding(snapshot_runtime_objects_cooperatively(
        lua,
        object_count,
        None,
    ))
}

/// Copies the same ordered object arena in bounded groups during covered startup.
/// No UI callback or object mutation may interleave this snapshot.
pub(in crate::script) async fn snapshot_runtime_objects_cooperatively(
    lua: &Lua,
    object_count: usize,
    budget: Option<&StartupBudget>,
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
            clamp_insets: snapshot_clamp_insets(&table, lua_index)?,
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
            minimap: (kind == UiObjectKind::Minimap)
                .then(|| crate::script::simple_script::minimap::snapshot(&table))
                .transpose()
                .map_err(|error| snapshot_error(format!("object {lua_index} minimap"), error))?,
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
                .then(|| crate::script::simple_script::status_bars::snapshot(&table))
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
        // Avoid per-field clocks while keeping large arenas interruptible.
        if lua_index % 64 == 0
            && let Some(budget) = budget
        {
            budget.checkpoint().await;
        }
    }

    Ok(UiRuntimeObjectPlan { objects, anchors })
}

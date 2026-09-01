//! Owned region and identity state copied from live Lua object tables.

use mlua::{Lua, Table};

use super::simple_script::{
    OBJECT_REGISTRY, alpha_key, anchors_key, backdrop_border_color_key, backdrop_color_key,
    button_pressed_key, checked_key, click_action_key, desaturated_key, draw_layer_key,
    draw_sub_level_key, edit_cursor_key, edit_focused_key, edit_multi_line_key, edit_password_key,
    edit_selection_end_key, edit_selection_start_key, edit_text_insets_key, enabled_key,
    font_face_key, font_flags_key, font_height_key, font_object_key, font_set_key,
    font_shadow_color_key, font_shadow_offset_key, frame_level_key, frame_strata_key, height_key,
    highlight_locked_key, hit_rect_insets_key, horizontal_scroll_key, horizontal_scroll_range_key,
    horizontal_tiling_key, index_key, justify_h_key, justify_v_key, keyboard_enabled_key,
    model_background_light_ghost_key, model_background_light_live_key, model_camera_key,
    model_character_light_ghost_key, model_character_light_live_key, model_file_key,
    model_fog_color_key, model_fog_far_key, model_fog_near_key, model_glow_key,
    model_pet_light_ghost_key, model_pet_light_live_key, model_scale_key, model_sequence_key,
    model_sequence_time_key, model_sequence_time_sequence_key, mouse_enabled_key,
    mouse_wheel_enabled_key, name_key, non_blocking_key, parent_key, parse_point, role_key,
    scale_key, shown_key, spacing_key, tex_coord_key, text_color_key, text_key,
    texture_blend_mode_key, texture_color_key, texture_file_key, texture_solid_color_key, type_key,
    vertical_scroll_key, vertical_scroll_range_key, vertical_tiling_key, width_key,
};
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
    pub(crate) first_anchor: usize,
    pub(crate) anchor_count: usize,
    pub(crate) texture: Option<UiRuntimeTexture>,
    pub(crate) text: Option<UiRuntimeText>,
    pub(crate) model: Option<UiRuntimeModel>,
    pub(crate) backdrop_color: Option<[f64; 4]>,
    pub(crate) backdrop_border_color: Option<[f64; 4]>,
    pub(crate) frame_level: Option<i32>,
    pub(crate) frame_strata: Option<UiFrameStrata>,
    pub(crate) keyboard_enabled: Option<bool>,
    pub(crate) mouse_enabled: Option<bool>,
    pub(crate) mouse_wheel_enabled: Option<bool>,
    pub(crate) hit_rect_insets: Option<[f64; 4]>,
    pub(crate) scroll_offset: Option<(f64, f64)>,
    pub(crate) scroll_range: Option<(f64, f64)>,
    pub(crate) enabled: Option<bool>,
    pub(crate) click_action: Option<u64>,
    pub(crate) checked: Option<bool>,
    pub(crate) highlighted: Option<bool>,
    pub(crate) pushed: Option<bool>,
    pub(crate) edit_focused: Option<bool>,
}

/// Post-Lua text, font, alignment, and EditBox presentation state.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeText {
    pub(crate) content: String,
    pub(crate) face: AssetPath,
    pub(crate) height: f64,
    pub(crate) rasterization: FontRasterization,
    pub(crate) color: [f64; 4],
    pub(crate) shadow_offset: [f64; 2],
    pub(crate) shadow_color: [f64; 4],
    pub(crate) spacing: f64,
    pub(crate) horizontal: HorizontalJustification,
    pub(crate) vertical: VerticalJustification,
    pub(crate) draw_layer: UiDrawLayer,
    pub(crate) draw_sub_level: i16,
    pub(crate) password: bool,
    pub(crate) multiline: bool,
    pub(crate) text_insets: [f64; 4],
    pub(crate) cursor: usize,
    pub(crate) selection: [usize; 2],
}

/// Post-Lua model source and animation-selection properties.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UiRuntimeModel {
    pub(crate) file: Option<AssetPath>,
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
        let text = if matches!(kind, UiObjectKind::FontString | UiObjectKind::EditBox) {
            snapshot_text(lua_index, kind, &table)?
        } else {
            None
        };
        let model = matches!(kind, UiObjectKind::Model | UiObjectKind::ModelFfx)
            .then(|| snapshot_model(lua_index, &table))
            .transpose()?;
        let is_frame = !matches!(kind, UiObjectKind::Texture | UiObjectKind::FontString);
        let is_button = matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton);
        let click_action = is_button
            .then(|| table.raw_get::<u64>(click_action_key()))
            .transpose()
            .map_err(|error| snapshot_error(format!("object {lua_index} click action"), error))?;
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
            first_anchor,
            anchor_count: anchors.len() - first_anchor,
            texture,
            text,
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
                .then(|| table.raw_get(highlight_locked_key()))
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

fn snapshot_text(
    lua_index: usize,
    kind: UiObjectKind,
    table: &Table,
) -> Result<Option<UiRuntimeText>, UiScriptError> {
    let font_set = table
        .raw_get::<bool>(font_set_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font assignment"), error))?;
    if !font_set {
        return Ok(None);
    }
    let Some(font) = table
        .raw_get::<Option<Table>>(font_object_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} font object"), error))?
    else {
        return Ok(None);
    };
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
    let color: Table = table
        .raw_get(text_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} text color"), error))?;
    let shadow_offset: Table = table
        .raw_get(font_shadow_offset_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} shadow offset"), error))?;
    let shadow_color: Table = table
        .raw_get(font_shadow_color_key())
        .map_err(|error| snapshot_error(format!("object {lua_index} shadow color"), error))?;
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
        color: numeric_array::<4>(&color, lua_index, "text color")?,
        shadow_offset: numeric_array::<2>(&shadow_offset, lua_index, "shadow offset")?,
        shadow_color: numeric_array::<4>(&shadow_color, lua_index, "shadow color")?,
        spacing: finite_region_number(table, spacing_key(), lua_index, "font spacing")?,
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

fn finite_region_number(
    table: &Table,
    key: mlua::LightUserData,
    lua_index: usize,
    field: &str,
) -> Result<f64, UiScriptError> {
    let value = table
        .raw_get::<f64>(key)
        .map_err(|error| snapshot_error(format!("object {lua_index} {field}"), error))?;
    if !value.is_finite() {
        return Err(UiScriptError::Plan {
            message: format!("live UI object {lua_index} has non-finite {field}"),
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
        _ => None,
    }
}

fn snapshot_error(label: impl Into<String>, error: mlua::Error) -> UiScriptError {
    UiScriptError::Execution {
        label: format!("{} snapshot", label.into()),
        message: error.to_string(),
    }
}

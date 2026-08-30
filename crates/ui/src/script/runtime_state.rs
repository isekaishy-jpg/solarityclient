//! Owned region and identity state copied from live Lua object tables.

use mlua::{Lua, Table};

use super::simple_script::{
    OBJECT_REGISTRY, alpha_key, anchors_key, height_key, index_key, name_key, parent_key,
    parse_point, role_key, scale_key, shown_key, type_key, width_key,
};
use crate::{UiObjectKind, UiObjectRole, UiPoint, UiScriptError};

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
        });
    }

    Ok(UiRuntimeObjectPlan { objects, anchors })
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

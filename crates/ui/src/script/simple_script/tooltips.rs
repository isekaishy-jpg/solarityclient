//! Retained stock game-tooltip ownership methods.

mod content;

use mlua::{Lua, ObjectLike, Table, Value};

use super::{
    DIRTY_LAYOUT, OBJECT_REGISTRY, anchors_key, create_anchor_record, index_key, is_object_type,
    live_region_scale, mark_object_state_changed, object_is_visible, point_index,
    tooltip_anchor_key, tooltip_offset_x_key, tooltip_offset_y_key, tooltip_owner_key,
    tooltip_padding_key, type_key,
};
use crate::UiPoint;

const TOOLTIP_OBJECTS: &str = "solarity.tooltip.objects";

pub(super) fn enroll(lua: &Lua, tooltip: &Table) -> mlua::Result<()> {
    let objects = match lua.named_registry_value::<Option<Table>>(TOOLTIP_OBJECTS)? {
        Some(objects) => objects,
        None => {
            let objects = lua.create_table()?;
            lua.set_named_registry_value(TOOLTIP_OBJECTS, objects.clone())?;
            objects
        }
    };
    objects.raw_set(objects.raw_len() + 1, tooltip.clone())
}

/// CGameTooltip::OnUpdate (0x0061B2E0) uses screen-root coordinates and
/// the tooltip's effective scale, independently of its owner or parent.
pub(super) fn update_cursor_anchors(lua: &Lua, cursor: (f64, f64)) -> mlua::Result<()> {
    let Some(tooltips) = lua.named_registry_value::<Option<Table>>(TOOLTIP_OBJECTS)? else {
        return Ok(());
    };
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    for slot in 1..=tooltips.raw_len() {
        let tooltip: Table = tooltips.raw_get(slot)?;
        let anchor = tooltip.raw_get::<String>(tooltip_anchor_key())?;
        let point = match anchor.as_str() {
            "ANCHOR_CURSOR" => UiPoint::Bottom,
            "ANCHOR_CURSOR_RIGHT" => UiPoint::BottomLeft,
            _ => continue,
        };
        if !object_is_visible(lua, tooltip.clone())? {
            continue;
        }
        let scale = live_region_scale(&objects, &tooltip)?;
        let mut offset = (cursor.0 / scale, cursor.1 / scale);
        if point == UiPoint::BottomLeft {
            offset.0 += tooltip.raw_get::<f64>(tooltip_offset_x_key())?;
            offset.1 += tooltip.raw_get::<f64>(tooltip_offset_y_key())?;
        }
        let anchors: Table = tooltip.raw_get(anchors_key())?;
        if let Some(old) = anchors.raw_get::<Option<Table>>(point_index(point))?
            && old.raw_get::<Option<usize>>(2)?.is_none()
            && old.raw_get::<String>(3)? == "BOTTOMLEFT"
            && old.raw_get::<f64>(4)? == offset.0
            && old.raw_get::<f64>(5)? == offset.1
        {
            continue;
        }
        anchors.raw_set(
            point_index(point),
            create_anchor_record(lua, point, None, UiPoint::BottomLeft, offset)?,
        )?;
        mark_object_state_changed(lua, &tooltip, DIRTY_LAYOUT)?;
    }
    Ok(())
}

pub(super) fn load_xml(lua: &Lua, tooltip: &Table) -> mlua::Result<()> {
    content::load_xml(lua, tooltip)
}

/// Registers the ownership relationship that stock tooltips keep separately
/// from their structural frame parent.
pub(super) fn register_game_tooltip_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    content::register(lua, methods)?;
    methods.raw_set(
        "SetPadding",
        lua.create_function(|_, (tooltip, padding): (Table, f64)| {
            tooltip.raw_set(tooltip_padding_key(), padding)
        })?,
    )?;
    methods.raw_set(
        "GetPadding",
        lua.create_function(|_, tooltip: Table| tooltip.raw_get::<f64>(tooltip_padding_key()))?,
    )?;
    methods.raw_set(
        "GetAnchorType",
        lua.create_function(|_, tooltip: Table| tooltip.raw_get::<String>(tooltip_anchor_key()))?,
    )?;
    methods.raw_set(
        "SetOwner",
        lua.create_function(
            |lua,
             (tooltip, owner, anchor, offset_x, offset_y): (
                Table,
                Value,
                Option<String>,
                Option<f64>,
                Option<f64>,
            )| {
                content::hide(lua, &tooltip)?;
                let Value::Table(owner) = owner else {
                    return Err(mlua::Error::runtime(
                        "GameTooltip:SetOwner(): frame required",
                    ));
                };
                if !is_object_type(&owner.raw_get::<String>(type_key())?, "Frame")
                    || owner == tooltip
                {
                    return Err(mlua::Error::runtime(
                        "GameTooltip:SetOwner(): another frame required",
                    ));
                }
                let anchor = anchor
                    .as_deref()
                    .and_then(canonical_anchor)
                    .unwrap_or("ANCHOR_LEFT");
                tooltip.raw_set(tooltip_owner_key(), owner.raw_get::<usize>(index_key())?)?;
                tooltip.raw_set(tooltip_anchor_key(), anchor)?;
                tooltip.raw_set(tooltip_offset_x_key(), offset_x.unwrap_or(0.0))?;
                tooltip.raw_set(tooltip_offset_y_key(), offset_y.unwrap_or(0.0))?;
                if anchor != "ANCHOR_PRESERVE" {
                    tooltip.call_method::<()>("ClearAllPoints", ())?;
                }
                content::anchor(
                    &tooltip,
                    &owner,
                    anchor,
                    offset_x.unwrap_or(0.0),
                    offset_y.unwrap_or(0.0),
                )
            },
        )?,
    )?;
    methods.raw_set(
        "GetOwner",
        lua.create_function(|lua, tooltip: Table| {
            let Some(owner) = tooltip.raw_get::<Option<usize>>(tooltip_owner_key())? else {
                return Ok(None::<Table>);
            };
            let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
            objects.raw_get::<Option<Table>>(owner)
        })?,
    )?;
    methods.raw_set(
        "IsOwned",
        lua.create_function(|_, (tooltip, candidate): (Table, Table)| {
            let owner = tooltip.raw_get::<Option<usize>>(tooltip_owner_key())?;
            let candidate = candidate.raw_get::<usize>(index_key())?;
            Ok((owner == Some(candidate)).then_some(Value::Number(1.0)))
        })?,
    )?;
    // Hiding a stock tooltip releases its temporary owner. This intentionally
    // replaces the generic Region method installed earlier in the metatable.
    methods.raw_set(
        "Hide",
        lua.create_function(|lua, tooltip: Table| content::hide(lua, &tooltip))?,
    )
}

/// Accepts only the anchor tokens exposed by the build-12340 tooltip API.
fn canonical_anchor(anchor: &str) -> Option<&'static str> {
    const ANCHORS: [&str; 12] = [
        "ANCHOR_TOP",
        "ANCHOR_RIGHT",
        "ANCHOR_BOTTOM",
        "ANCHOR_LEFT",
        "ANCHOR_TOPRIGHT",
        "ANCHOR_BOTTOMRIGHT",
        "ANCHOR_TOPLEFT",
        "ANCHOR_BOTTOMLEFT",
        "ANCHOR_CURSOR",
        "ANCHOR_CURSOR_RIGHT",
        "ANCHOR_PRESERVE",
        "ANCHOR_NONE",
    ];
    ANCHORS
        .into_iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(anchor))
}

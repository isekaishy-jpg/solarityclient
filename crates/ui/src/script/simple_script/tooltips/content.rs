//! Build-12340 GameTooltip line storage and its show-time text measurement.

use mlua::{LightUserData, Lua, ObjectLike, Table, Value, Variadic};

use super::super::{
    OBJECT_REGISTRY, auto_text_width_key, call_object_handler, font_object_key, lua_bool, lua_text,
    name_key, object_script_function, set_object_shown, shown_key, tooltip_anchor_key,
    tooltip_offset_x_key, tooltip_offset_y_key, tooltip_owner_key, tooltip_padding_key, type_key,
};
use crate::UiScriptHandler;

// A distinct allocation keeps private tooltip storage out of the public Lua namespace.
static CONTENT_TOKEN: [u8; 16] = *b"tooltip-content!";

fn state(lua: &Lua, tooltip: &Table) -> mlua::Result<Table> {
    let key = LightUserData(std::ptr::from_ref(&CONTENT_TOKEN).cast_mut().cast());
    if let Some(state) = tooltip.raw_get::<Option<Table>>(key)? {
        return Ok(state);
    }
    let state = lua.create_table()?;
    state.raw_set("left", lua.create_table()?)?;
    state.raw_set("right", lua.create_table()?)?;
    state.raw_set("wrap", lua.create_table()?)?;
    state.raw_set("count", 0_usize)?;
    state.raw_set("minimum", 0.0)?;
    state.raw_set("fixed", false)?;
    tooltip.raw_set(key, state.clone())?;
    Ok(state)
}

pub(super) fn load_xml(lua: &Lua, tooltip: &Table) -> mlua::Result<()> {
    let state = state(lua, tooltip)?;
    let left = lua.create_table()?;
    let right = lua.create_table()?;
    // CGameTooltip::LoadXML (0x0061FB30) enrolls consecutive named pairs.
    if let Some(name) = tooltip.raw_get::<Option<String>>(name_key())? {
        for index in 1.. {
            let l = lua
                .globals()
                .raw_get::<Option<Table>>(format!("{name}TextLeft{index}"))?;
            let r = lua
                .globals()
                .raw_get::<Option<Table>>(format!("{name}TextRight{index}"))?;
            let (Some(l), Some(r)) = (l, r) else {
                break;
            };
            if l.raw_get::<String>(type_key())? != "FontString"
                || r.raw_get::<String>(type_key())? != "FontString"
            {
                break;
            }
            left.raw_set(index, l)?;
            right.raw_set(index, r)?;
        }
    }
    state.raw_set("left", left)?;
    state.raw_set("right", right)?;
    Ok(())
}

pub(super) fn register(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "NumLines",
        lua.create_function(|lua, tooltip: Table| state(lua, &tooltip)?.raw_get::<usize>("count"))?,
    )?;
    methods.raw_set(
        "ClearLines",
        lua.create_function(|lua, tooltip: Table| clear(lua, &tooltip))?,
    )?;
    methods.raw_set(
        "SetMinimumWidth",
        lua.create_function(
            |lua, (tooltip, width, fixed): (Table, f64, Option<Value>)| {
                let state = state(lua, &tooltip)?;
                state.raw_set("minimum", width)?;
                state.raw_set("fixed", truth(fixed))
            },
        )?,
    )?;
    methods.raw_set(
        "GetMinimumWidth",
        lua.create_function(|lua, tooltip: Table| {
            let state = state(lua, &tooltip)?;
            Ok((
                state.raw_get::<f64>("minimum")?,
                state.raw_get::<bool>("fixed")?.then_some(1),
            ))
        })?,
    )?;
    methods.raw_set(
        "AddFontStrings",
        lua.create_function(|lua, (tooltip, left, right): (Table, Table, Table)| {
            if left.raw_get::<String>(type_key())? != "FontString"
                || right.raw_get::<String>(type_key())? != "FontString"
            {
                return Err(mlua::Error::runtime(
                    "GameTooltip:AddFontStrings(): font strings required",
                ));
            }
            let state = state(lua, &tooltip)?;
            let l: Table = state.raw_get("left")?;
            let r: Table = state.raw_get("right")?;
            let index = l.raw_len() + 1;
            l.raw_set(index, left)?;
            r.raw_set(index, right)
        })?,
    )?;
    methods.raw_set(
        "SetText",
        lua.create_function(|lua, (tooltip, args): (Table, Variadic<Value>)| {
            let text = lua_text(lua, argument(&args, 0))?;
            if text.is_none() {
                return Err(mlua::Error::runtime("GameTooltip:SetText(): text required"));
            }
            clear(lua, &tooltip)?;
            let color = color(lua, &args, 1, true)?;
            append(
                lua,
                &tooltip,
                text,
                None,
                color,
                color,
                lua_bool(&argument(&args, 5), false),
            )?;
            show(lua, &tooltip)
        })?,
    )?;
    methods.raw_set(
        "AddLine",
        lua.create_function(|lua, (tooltip, args): (Table, Variadic<Value>)| {
            let color = color(lua, &args, 1, false)?;
            append(
                lua,
                &tooltip,
                lua_text(lua, argument(&args, 0))?,
                None,
                color,
                color,
                lua_bool(&argument(&args, 4), false),
            )
        })?,
    )?;
    methods.raw_set(
        "AddDoubleLine",
        lua.create_function(|lua, (tooltip, args): (Table, Variadic<Value>)| {
            append(
                lua,
                &tooltip,
                lua_text(lua, argument(&args, 0))?,
                lua_text(lua, argument(&args, 1))?,
                color(lua, &args, 2, false)?,
                color(lua, &args, 5, false)?,
                lua_bool(&argument(&args, 8), false),
            )
        })?,
    )?;
    methods.raw_set(
        "Show",
        lua.create_function(|lua, tooltip: Table| show(lua, &tooltip))?,
    )
}

fn truth(value: Option<Value>) -> bool {
    value.is_some_and(|value| lua_bool(&value, false))
}

fn argument(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or(Value::Nil)
}

fn color(lua: &Lua, args: &[Value], start: usize, alpha: bool) -> mlua::Result<[f64; 4]> {
    // Native CImVector at 0x00AD2D2C: ARGB 0xFFFFD200.
    let number = |index| lua.coerce_number(argument(args, index));
    Ok(if let Some(r) = number(start)? {
        [
            r.clamp(0., 1.),
            number(start + 1)?.unwrap_or(0.).clamp(0., 1.),
            number(start + 2)?.unwrap_or(0.).clamp(0., 1.),
            if alpha {
                number(start + 3)?.unwrap_or(1.).clamp(0., 1.)
            } else {
                1.
            },
        ]
    } else {
        [1., 210. / 255., 0., 1.]
    })
}

pub(super) fn clear(lua: &Lua, tooltip: &Table) -> mlua::Result<()> {
    let state = state(lua, tooltip)?;
    let count: usize = state.raw_get("count")?;
    let left: Table = state.raw_get("left")?;
    let right: Table = state.raw_get("right")?;
    let wraps: Table = state.raw_get("wrap")?;
    for index in 1..=count {
        let l: Table = left.raw_get(index)?;
        l.call_method::<()>("SetWidth", 0.)?;
        l.raw_set(auto_text_width_key(), true)?;
        for line in [l, right.raw_get::<Table>(index)?] {
            line.call_method::<()>("SetText", "")?;
            set_object_shown(lua, &line, false)?;
        }
        wraps.raw_set(index, false)?;
    }
    state.raw_set("count", 0_usize)?;
    if !state.raw_get::<bool>("fixed")? {
        state.raw_set("minimum", 0.)?;
    }
    if count != 0
        && let Some(callback) =
            object_script_function(lua, tooltip, UiScriptHandler::TooltipCleared)?
    {
        call_object_handler(lua, &callback, tooltip.clone())?;
    }
    Ok(())
}

pub(super) fn hide(lua: &Lua, tooltip: &Table) -> mlua::Result<()> {
    clear(lua, tooltip)?;
    tooltip.raw_set(tooltip_owner_key(), Option::<usize>::None)?;
    tooltip.raw_set(tooltip_anchor_key(), "ANCHOR_NONE")?;
    tooltip.call_method::<()>("SetAlpha", 1.)?;
    set_object_shown(lua, tooltip, false)
}

#[allow(clippy::too_many_arguments)]
fn append(
    lua: &Lua,
    tooltip: &Table,
    ltext: Option<String>,
    rtext: Option<String>,
    lc: [f64; 4],
    rc: [f64; 4],
    wrap: bool,
) -> mlua::Result<()> {
    let ltext = ltext.filter(|text| !text.is_empty());
    let rtext = rtext.filter(|text| !text.is_empty());
    if ltext.is_none() && rtext.is_none() {
        return Ok(());
    }
    let state = state(lua, tooltip)?;
    let left: Table = state.raw_get("left")?;
    let right: Table = state.raw_get("right")?;
    let index = state.raw_get::<usize>("count")? + 1;
    if left.raw_len() == 0 {
        return Ok(());
    }
    // 0x0061FEC0 retains one spare pair, copying the last row's font objects.
    if index == left.raw_len() {
        let next = index + 1;
        let name = tooltip
            .raw_get::<Option<String>>(name_key())?
            .unwrap_or_default();
        let previous_left: Table = left.raw_get(index)?;
        let previous_right: Table = right.raw_get(index)?;
        let l: Table = tooltip.call_method(
            "CreateFontString",
            (format!("{name}TextLeft{next}"), "ARTWORK"),
        )?;
        let r: Table = tooltip.call_method(
            "CreateFontString",
            (format!("{name}TextRight{next}"), "ARTWORK"),
        )?;
        for (line, previous) in [(&l, &previous_left), (&r, &previous_right)] {
            let font: Table = previous.raw_get(font_object_key())?;
            line.call_method::<()>("SetFontObject", font)?;
            set_object_shown(lua, line, false)?;
        }
        l.call_method::<()>(
            "SetPoint",
            ("TOPLEFT", previous_left, "BOTTOMLEFT", 0., -2.),
        )?;
        r.call_method::<()>("SetPoint", ("RIGHT", l.clone(), "LEFT", 40., 0.))?;
        left.raw_set(next, l)?;
        right.raw_set(next, r)?;
    }
    let has_right = rtext.is_some();
    for (line, text, color) in [
        (left.raw_get::<Table>(index)?, ltext, lc),
        (right.raw_get::<Table>(index)?, rtext, rc),
    ] {
        if let Some(text) = text {
            line.call_method::<()>("SetTextColor", (color[0], color[1], color[2], color[3]))?;
            line.call_method::<()>("SetText", text)?;
            set_object_shown(lua, &line, true)?;
        }
    }
    state
        .raw_get::<Table>("wrap")?
        .raw_set(index, wrap && !has_right)?;
    state.raw_set("count", index)
}

fn show(lua: &Lua, tooltip: &Table) -> mlua::Result<()> {
    let state = state(lua, tooltip)?;
    let count: usize = state.raw_get("count")?;
    let Some(owner) = tooltip
        .raw_get::<Option<usize>>(tooltip_owner_key())?
        .filter(|_| count != 0)
    else {
        return hide(lua, tooltip);
    };
    let left: Table = state.raw_get("left")?;
    let right: Table = state.raw_get("right")?;
    let wraps: Table = state.raw_get("wrap")?;
    let fixed = state.raw_get::<bool>("fixed")?;
    let mut width = state.raw_get::<f64>("minimum")?;
    // 0x0061CAF0 first finds the widest unwrapped row, including both columns.
    if !fixed {
        for index in 1..=count {
            if wraps.raw_get::<Option<bool>>(index)?.unwrap_or(false) {
                continue;
            }
            let l: Table = left.raw_get(index)?;
            let r: Table = right.raw_get(index)?;
            let ls = l.raw_get::<bool>(shown_key())?;
            let rs = r.raw_get::<bool>(shown_key())?;
            let row = if ls {
                l.call_method::<f64>("GetStringWidth", ())?
            } else {
                0.
            } + if rs {
                r.call_method::<f64>("GetStringWidth", ())?
            } else {
                0.
            } + if ls && rs {
                f64::from(f32::from_bits(0x4219_999a))
            } else {
                0.
            };
            width = width.max(row);
        }
    }
    for index in 1..=count {
        if !fixed && !wraps.raw_get::<Option<bool>>(index)?.unwrap_or(false) {
            continue;
        }
        let l: Table = left.raw_get(index)?;
        // The native natural-width query ignores the previous Show's constraint.
        l.raw_set(auto_text_width_key(), true)?;
        let natural = l.call_method::<f64>("GetStringWidth", ())?;
        let candidate = natural.min(f64::from(f32::from_bits(0x4366_6666)));
        l.call_method::<()>("SetWordWrap", true)?;
        if width < candidate {
            l.call_method::<()>("SetWidth", candidate)?;
            width = width.max(l.call_method::<f64>("GetStringWidth", ())?);
        }
        l.call_method::<()>("SetWidth", width)?;
    }
    let mut height = 0.;
    for index in 1..=count {
        let l: Table = left.raw_get(index)?;
        let r: Table = right.raw_get(index)?;
        if r.raw_get::<bool>(shown_key())? {
            r.call_method::<()>("SetPoint", ("RIGHT", l.clone(), "LEFT", width, 0.))?;
        }
        if l.raw_get::<bool>(shown_key())? {
            if height != 0. {
                height += 2.;
            }
            height += l.call_method::<f64>("GetStringHeight", ())?;
        }
    }
    let border = f64::from(f32::from_bits(0x4123_d70a)) * 2.;
    tooltip.call_method::<()>(
        "SetSize",
        (
            width + border + tooltip.raw_get::<f64>(tooltip_padding_key())?,
            height + border,
        ),
    )?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let owner: Table = objects.raw_get(owner)?;
    anchor(
        tooltip,
        &owner,
        &tooltip.raw_get::<String>(tooltip_anchor_key())?,
        tooltip.raw_get(tooltip_offset_x_key())?,
        tooltip.raw_get(tooltip_offset_y_key())?,
    )?;
    tooltip.call_method::<()>("SetAlpha", 1.)?;
    tooltip.call_method::<()>("SetClampedToScreen", true)?;
    set_object_shown(lua, tooltip, true)
}

pub(super) fn anchor(
    tooltip: &Table,
    owner: &Table,
    anchor: &str,
    x: f64,
    y: f64,
) -> mlua::Result<()> {
    let points = match anchor {
        "ANCHOR_LEFT" => Some(("BOTTOMRIGHT", "TOPLEFT")),
        "ANCHOR_RIGHT" => Some(("BOTTOMLEFT", "TOPRIGHT")),
        "ANCHOR_BOTTOMLEFT" => Some(("TOPRIGHT", "BOTTOMLEFT")),
        "ANCHOR_BOTTOM" => Some(("TOP", "BOTTOM")),
        "ANCHOR_BOTTOMRIGHT" => Some(("TOPLEFT", "BOTTOMRIGHT")),
        "ANCHOR_TOPLEFT" => Some(("BOTTOMLEFT", "TOPLEFT")),
        "ANCHOR_TOP" => Some(("BOTTOM", "TOP")),
        "ANCHOR_TOPRIGHT" => Some(("BOTTOMRIGHT", "TOPRIGHT")),
        _ => None,
    };
    if let Some((point, relative)) = points {
        tooltip.call_method::<()>("ClearAllPoints", ())?;
        tooltip.call_method::<()>("SetPoint", (point, owner.clone(), relative, x, y))?;
    }
    Ok(())
}

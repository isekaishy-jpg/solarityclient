//! Native FrameXML UIParent scale, separate from the fixed GlueXML canvas.

use mlua::{Lua, Table};

use super::{OBJECT_REGISTRY, UiScriptEnvironment, cvars::UiCVarRegistry};

const ROOT: &str = "solarity.ui.world_root";
const ASPECT: &str = "solarity.ui.world_root_aspect";
const MINIMUM: f32 = 0.64;

#[derive(Clone, Copy)]
enum Update {
    Initial,
    Enabled,
    Value,
}

fn capped_request(value: f32, aspect: f32) -> f32 {
    if aspect * 3.0 < 4.0 {
        value.min(aspect * 0.75)
    } else {
        value
    }
}

fn automatic_scale(height: u32, aspect: f32) -> f32 {
    let scale = if height <= 768 {
        1.0
    } else {
        768.0 / height as f32
    };
    capped_request(scale, aspect).max(0.9).max(MINIMUM)
}

fn selected_scale(
    update: Update,
    height: u32,
    aspect: f32,
    enabled: bool,
    value: f32,
) -> Option<f32> {
    let request = capped_request(value, aspect);
    match update {
        Update::Initial if enabled && request >= MINIMUM => Some(request),
        Update::Initial => Some(automatic_scale(height, aspect)),
        Update::Enabled if !enabled => Some(automatic_scale(height, aspect)),
        Update::Value if !enabled => None,
        Update::Enabled | Update::Value => Some(request.max(MINIMUM)),
    }
}

fn configured_aspect(cvars: &UiCVarRegistry) -> f32 {
    if cvars.integer("widescreen").unwrap_or(1) == 0 {
        return 4.0 / 3.0;
    }
    cvars
        .get("gxResolution")
        .and_then(|resolution| {
            let (width, height) = resolution.split_once('x')?;
            let width = width.parse::<u32>().ok()?;
            let height = height.parse::<u32>().ok()?;
            (width != 0 && height != 0).then_some(width as f32 / height as f32)
        })
        .unwrap_or(4.0 / 3.0)
}

pub(super) fn initialize(lua: &Lua, environment: &UiScriptEnvironment) -> mlua::Result<()> {
    let root = lua.globals().raw_get::<Option<Table>>("UIParent")?;
    lua.set_named_registry_value(ROOT, root)?;
    lua.set_named_registry_value(ASPECT, configured_aspect(&environment.cvars()))?;
    apply(
        lua,
        environment.logical_extent().1,
        &environment.cvars(),
        Update::Initial,
        None,
    )
}

/// World GetScreenWidth/Height divide by the retained UIParent effective scale.
/// The native root is bound after FrameXML construction, independent of later
/// writes to the global named UIParent.
pub(super) fn root_scale(lua: &Lua) -> mlua::Result<f64> {
    let Some(root) = lua.named_registry_value::<Option<Table>>(ROOT)? else {
        return Ok(1.0);
    };
    let objects = lua.named_registry_value::<Table>(OBJECT_REGISTRY)?;
    super::live_region_scale(&objects, &root)
}

pub(super) fn cvar_changing(
    lua: &Lua,
    height: u32,
    cvars: &UiCVarRegistry,
    name: &str,
    requested: &str,
) -> mlua::Result<()> {
    let update = if name.eq_ignore_ascii_case("useUiScale") {
        Update::Enabled
    } else if name.eq_ignore_ascii_case("uiScale") {
        Update::Value
    } else {
        return Ok(());
    };
    apply(lua, height, cvars, update, Some(requested))
}

fn apply(
    lua: &Lua,
    height: u32,
    cvars: &UiCVarRegistry,
    update: Update,
    requested: Option<&str>,
) -> mlua::Result<()> {
    let Some(root) = lua.named_registry_value::<Option<Table>>(ROOT)? else {
        return Ok(());
    };
    let aspect = lua.named_registry_value::<f32>(ASPECT)?;
    let mut enabled = cvars.integer("useUiScale").unwrap_or(0) != 0;
    let mut value = cvars.number("uiScale").unwrap_or(0.0);
    if let Some(requested) = requested {
        match update {
            Update::Enabled => enabled = UiCVarRegistry::parse_integer(requested) != 0,
            Update::Value => {
                value = requested
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .unwrap_or(0.0)
            }
            Update::Initial => {}
        }
    }
    let Some(scale) = selected_scale(update, height, aspect, enabled, value) else {
        return Ok(());
    };
    // 513240 forces root propagation and DISPLAY_SIZE_CHANGED even when the
    // requested effective scale is unchanged. Use the normal layout mutation
    // path so child anchors, fonts, hit testing and model viewports agree.
    root.raw_set(super::scale_key(), f64::from(scale))?;
    super::mark_live_state_changed(lua)?;
    let objects = lua.named_registry_value::<Table>(OBJECT_REGISTRY)?;
    super::dispatch_event_callbacks(lua, objects.raw_len(), "DISPLAY_SIZE_CHANGED", &[])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_scale_matches_original_client_callbacks() -> Result<(), Box<dyn std::error::Error>> {
        let native = include_str!("../../../tests/fixtures/ui_scale_native.txt");
        let mut count = 0;
        for line in native.lines().filter_map(|line| line.strip_prefix("case ")) {
            let (input, output) = line.split_once(';').ok_or("native case delimiter")?;
            let values: Vec<_> = input.split_whitespace().collect();
            let mode = values[0].parse::<u32>()?;
            let width = values[1].parse::<u32>()?;
            let height = values[2].parse::<u32>()?;
            let aspect = if values[3] == "1" {
                width as f32 / height as f32
            } else {
                4.0 / 3.0
            };
            let update = match mode {
                0 => Update::Initial,
                1 => Update::Enabled,
                _ => Update::Value,
            };
            let actual =
                selected_scale(update, height, aspect, values[4] == "1", values[5].parse()?);
            let expected = output
                .split_whitespace()
                .next()
                .ok_or("native scale result")?;
            let expected = if expected == "-" {
                None
            } else {
                Some(expected.parse::<u32>()?)
            };
            assert_eq!(actual.map(f32::to_bits), expected, "{input}");
            count += 1;
        }
        assert_eq!(count, 504);
        Ok(())
    }
}

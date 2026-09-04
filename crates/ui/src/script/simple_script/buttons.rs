//! Button and check-button methods shared by static and dynamic objects.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{LightUserData, Lua, Table, Value, Variadic};
use solarity_asset::AssetStoreHandle;

use crate::UiScriptHandler;
use crate::font::wrap_line;
use crate::{FontDefinition, FontRasterization, FontSystem};

use super::{
    DIRTY_TEXT, DIRTY_TEXTURE, DIRTY_WIDGET, DynamicArenaState, OBJECT_REGISTRY,
    auto_text_height_key, auto_text_width_key, button_pressed_key, button_state_locked_key,
    button_text_key, checked_key, clamped_color, click_action_key, create_dynamic_region,
    disabled_font_key, disabled_text_color_key, disabled_texture_key, drag_button_key, enabled_key,
    font_object_key, font_set_key, font_shadow_offset_key, height_key, highlight_font_key,
    highlight_locked_key, highlight_texture_key, lua_bool, lua_text, mark_live_state_changed,
    mark_object_state_changed, max_text_lines_key, name_key, non_space_wrap_key, normal_font_key,
    normal_texture_key, object_script_function, pushed_texture_key, resolve_font_object,
    spacing_key, text_key, texture_file_key, texture_solid_color_key, type_key, width_key,
    word_wrap_key,
};

#[derive(Clone, Copy)]
struct MeasuredCharacter {
    character: char,
    advance: f64,
}

/// Per-button native recursion guard; never exposed as a script property.
static CLICK_ACTIVE_TOKEN: u8 = 0;

/// Build 12340's script Click (0x00978260) enters the same virtual click as
/// pointer input. Checkbox toggling (0x009623C0) precedes Button's enabled and
/// recursion guards (0x0096FD70), including disabled or recursively clicked
/// checkboxes. The guarded base path dispatches PreClick, OnClick, PostClick.
pub(super) fn dispatch_click(
    lua: &Lua,
    button: &Table,
    mouse_button: &str,
    down: bool,
) -> mlua::Result<()> {
    if button.raw_get::<String>(type_key())? == "CheckButton" {
        super::apply_check_button_click(lua, button)?;
    }
    let active_key = super::hidden_key(&CLICK_ACTIVE_TOKEN);
    if !button.raw_get::<bool>(enabled_key())?
        || button.raw_get::<Option<bool>>(active_key)?.unwrap_or(false)
    {
        return Ok(());
    }
    button.raw_set(active_key, true)?;
    let result = (|| {
        for handler in [
            UiScriptHandler::PreClick,
            UiScriptHandler::Click,
            UiScriptHandler::PostClick,
        ] {
            if let Some(function) = object_script_function(lua, button, handler)? {
                call_click_handler(lua, &function, button.clone(), mouse_button, down)?;
            }
        }
        Ok(())
    })();
    let restore = button.raw_set(active_key, Value::Nil);
    result.and(restore)
}

/// Archive-backed state required by the stock text-extent methods.
#[derive(Clone)]
pub(super) struct TextMeasurement {
    assets: Option<AssetStoreHandle>,
    fonts: Rc<HashMap<String, FontDefinition>>,
    system: Rc<RefCell<FontSystem>>,
    pixels_per_ui_unit: f64,
}

impl TextMeasurement {
    pub(super) fn new(
        assets: Option<AssetStoreHandle>,
        fonts: Rc<HashMap<String, FontDefinition>>,
        logical_height: u32,
    ) -> Result<Self, crate::FontError> {
        Ok(Self {
            assets,
            fonts,
            system: Rc::new(RefCell::new(FontSystem::new()?)),
            pixels_per_ui_unit: f64::from(logical_height) / 768.0,
        })
    }

    fn width(&self, button: &Table) -> mlua::Result<f64> {
        self.width_with_font(button, normal_font_key(), "Button:GetTextWidth")
    }

    pub(super) fn font_string_width(&self, font_string: &Table) -> mlua::Result<f64> {
        self.font_string_dimensions(font_string)
            .map(|dimensions| dimensions.0)
    }

    pub(super) fn font_string_height(&self, font_string: &Table) -> mlua::Result<f64> {
        self.font_string_dimensions(font_string)
            .map(|dimensions| dimensions.1)
    }

    pub(super) fn update_auto_font_string_size(&self, font_string: &Table) -> mlua::Result<()> {
        // Asset-less script fixtures validate Lua behavior without constructing
        // a presentation. Exact automatic extents require the mounted stock face.
        if self.assets.is_none() {
            return Ok(());
        }
        let auto_width = font_string.raw_get::<bool>(auto_text_width_key())?;
        let auto_height = font_string.raw_get::<bool>(auto_text_height_key())?;
        if !auto_width && !auto_height {
            return Ok(());
        }
        let dimensions = self.font_string_dimensions(font_string)?;
        if auto_width {
            font_string.raw_set(width_key(), dimensions.0)?;
        }
        if auto_height {
            font_string.raw_set(height_key(), dimensions.1)?;
        }
        Ok(())
    }

    pub(super) fn synchronize_auto_font_strings(
        &self,
        lua: &Lua,
        object_count: usize,
    ) -> mlua::Result<()> {
        let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
        for index in 1..=object_count {
            let object: Table = objects.raw_get(index)?;
            if object.raw_get::<String>(type_key())? == "FontString" {
                self.update_auto_font_string_size(&object)?;
            }
        }
        Ok(())
    }

    fn width_with_font(
        &self,
        object: &Table,
        font_key: LightUserData,
        operation: &str,
    ) -> mlua::Result<f64> {
        let Some(text) = object.raw_get::<Option<String>>(text_key())? else {
            return Ok(0.0);
        };
        let text = visible_text(&text);
        if text.is_empty() {
            return Ok(0.0);
        }
        let Some(font) = object.raw_get::<Option<Table>>(font_key)? else {
            return Ok(self.one_pixel());
        };
        let name = font.raw_get::<String>(name_key())?;
        let Some(definition) = self.fonts.get(&name) else {
            return Err(mlua::Error::runtime(format!(
                "{operation} font object {name} has no stock definition"
            )));
        };
        let Some(path) = definition.face() else {
            return Ok(self.one_pixel());
        };
        let Some(height) = definition.height() else {
            return Ok(self.one_pixel());
        };
        let pixel_height = (f64::from(height) * self.pixels_per_ui_unit)
            .round()
            .max(1.0) as u32;
        let rasterization = if definition.monochrome().unwrap_or(false) {
            FontRasterization::Monochrome
        } else {
            FontRasterization::Antialiased
        };
        let spacing = f64::from(definition.spacing().unwrap_or(0.0));
        let shadow = definition
            .shadow()
            .and_then(|shadow| shadow.offset())
            .map_or(0.0, |offset| f64::from(offset.0).max(0.0));
        let mut widest = 0.0_f64;
        let Some(assets) = &self.assets else {
            return Err(mlua::Error::runtime(format!(
                "{operation} requires a mounted stock asset store"
            )));
        };
        let mut assets = assets.borrow_mut();
        let mut system = self.system.borrow_mut();
        for line in text.lines() {
            let width = system
                .measure_line_width_26_6(&mut assets, path, pixel_height, line, rasterization)
                .map_err(|error| mlua::Error::runtime(error.to_string()))?;
            let glyph_spacing = line.chars().count().saturating_sub(1) as f64 * spacing;
            widest = widest.max(width as f64 / 64.0 / self.pixels_per_ui_unit + glyph_spacing);
        }
        Ok((widest + shadow).max(self.one_pixel()))
    }

    fn height(&self, button: &Table) -> mlua::Result<f64> {
        let Some(text) = button.raw_get::<Option<String>>(text_key())? else {
            return Ok(0.0);
        };
        if text.is_empty() {
            return Ok(0.0);
        }
        let Some(font) = button.raw_get::<Option<Table>>(normal_font_key())? else {
            return Ok(self.one_pixel());
        };
        let name = font.raw_get::<String>(name_key())?;
        let Some(definition) = self.fonts.get(&name) else {
            return Err(mlua::Error::runtime(format!(
                "Button:GetTextHeight font object {name} has no stock definition"
            )));
        };
        let Some(line_height) = definition.height() else {
            return Ok(self.one_pixel());
        };
        let lines = text.lines().count().max(1) as f64;
        let shadow = definition
            .shadow()
            .and_then(|shadow| shadow.offset())
            .map_or(0.0, |offset| f64::from(offset.1).max(0.0));
        Ok((f64::from(line_height) * lines + shadow).max(self.one_pixel()))
    }

    fn one_pixel(&self) -> f64 {
        self.pixels_per_ui_unit.recip()
    }

    pub(super) fn font_string_dimensions(&self, font_string: &Table) -> mlua::Result<(f64, f64)> {
        let Some(text) = font_string.raw_get::<Option<String>>(text_key())? else {
            return Ok((0.0, 0.0));
        };
        let text = visible_text(&text);
        if text.is_empty() {
            return Ok((0.0, 0.0));
        }
        let Some(font) = font_string.raw_get::<Option<Table>>(font_object_key())? else {
            return Ok((self.one_pixel(), self.one_pixel()));
        };
        let name = font.raw_get::<String>(name_key())?;
        let Some(definition) = self.fonts.get(&name) else {
            return Err(mlua::Error::runtime(format!(
                "FontString extent font object {name} has no stock definition"
            )));
        };
        let (Some(path), Some(line_height)) = (definition.face(), definition.height()) else {
            return Ok((self.one_pixel(), self.one_pixel()));
        };
        let Some(assets) = &self.assets else {
            return Err(mlua::Error::runtime(
                "FontString extent requires a mounted stock asset store",
            ));
        };
        let pixel_height = (f64::from(line_height) * self.pixels_per_ui_unit)
            .round()
            .max(1.0) as u32;
        let rasterization = if definition.monochrome().unwrap_or(false) {
            FontRasterization::Monochrome
        } else {
            FontRasterization::Antialiased
        };
        let spacing = font_string.raw_get::<f64>(spacing_key())?;
        let shadow: Table = font_string.raw_get(font_shadow_offset_key())?;
        let shadow_x = shadow.raw_get::<f64>(1)?.max(0.0);
        let shadow_y = shadow.raw_get::<f64>(2)?.max(0.0);
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let auto_width = font_string.raw_get::<bool>(auto_text_width_key())?;
        let word_wrap = font_string.raw_get::<bool>(word_wrap_key())?;
        let non_space_wrap = font_string.raw_get::<bool>(non_space_wrap_key())?;
        let max_lines = font_string.raw_get::<u32>(max_text_lines_key())? as usize;
        let wrap_width = (!auto_width && word_wrap)
            .then(|| font_string.raw_get::<f64>(width_key()))
            .transpose()?
            .filter(|width| width.is_finite() && *width > 0.0);
        let mut widest = 0.0_f64;
        let mut line_count = 0_usize;
        let mut assets = assets.borrow_mut();
        let mut system = self.system.borrow_mut();
        for line in normalized.split('\n') {
            let advances = system
                .measure_character_advances_26_6(
                    &mut assets,
                    path,
                    pixel_height,
                    line,
                    rasterization,
                )
                .map_err(|error| mlua::Error::runtime(error.to_string()))?;
            let measured = line
                .chars()
                .zip(advances)
                .map(|(character, advance)| MeasuredCharacter {
                    character,
                    advance: advance as f64 / 64.0 / self.pixels_per_ui_unit,
                })
                .collect::<Vec<_>>();
            let wrapped = match wrap_width {
                Some(width) => wrap_line(
                    &measured,
                    width,
                    non_space_wrap,
                    |item| item.character,
                    |item| item.advance,
                ),
                None => vec![measured],
            };
            for wrapped_line in wrapped {
                if max_lines > 0 && line_count == max_lines {
                    break;
                }
                widest = widest.max(wrapped_line.iter().map(|item| item.advance).sum());
                line_count += 1;
            }
            if max_lines > 0 && line_count == max_lines {
                break;
            }
        }
        let rendered_line_height = f64::from(pixel_height) / self.pixels_per_ui_unit;
        let height = rendered_line_height * line_count as f64
            + spacing * line_count.saturating_sub(1) as f64;
        Ok((
            (widest + shadow_x).max(self.one_pixel()),
            (height + shadow_y).max(self.one_pixel()),
        ))
    }
}

/// Removes build-12340 inline color controls before native text measurement.
fn visible_text(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut cursor = 0_usize;
    while cursor < source.len() {
        let remaining = &source[cursor..];
        if let Some(hex) = remaining.strip_prefix("|c").and_then(|tail| tail.get(..8))
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            cursor += 10;
            continue;
        }
        if remaining.starts_with("|r") {
            cursor += 2;
            continue;
        }
        if remaining.starts_with("|n") {
            output.push('\n');
            cursor += 2;
            continue;
        }
        if remaining.starts_with("||") {
            output.push('|');
            cursor += 2;
            continue;
        }
        let Some(character) = remaining.chars().next() else {
            break;
        };
        output.push(character);
        cursor += character.len_utf8();
    }
    output
}

pub(super) fn register_button_methods(
    lua: &Lua,
    methods: &Table,
    measurement: Option<TextMeasurement>,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<()> {
    let width_measurement = measurement.clone();
    register_font_pair(
        lua,
        methods,
        "SetNormalFontObject",
        "GetNormalFontObject",
        normal_font_key(),
        true,
    )?;
    methods.raw_set(
        "SetDisabledTextColor",
        lua.create_function(
            |lua, (button, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                button.raw_set(
                    disabled_text_color_key(),
                    lua.create_sequence_from(clamped_color(red, green, blue, alpha))?,
                )?;
                mark_object_state_changed(lua, &button, DIRTY_TEXT)
            },
        )?,
    )?;
    methods.raw_set(
        "GetFontString",
        lua.create_function(|_, button: Table| button.raw_get::<Option<Table>>(button_text_key()))?,
    )?;
    let font_string_measurement = measurement.clone();
    methods.raw_set(
        "SetFontString",
        lua.create_function(move |lua, (button, font_string): (Table, Table)| {
            if font_string.raw_get::<String>(type_key())? != "FontString" {
                return Err(mlua::Error::runtime(
                    "Usage: Button:SetFontString(fontString)",
                ));
            }
            if let Some(font) = button.raw_get::<Option<Table>>(normal_font_key())? {
                font_string.raw_set(font_object_key(), font)?;
                font_string.raw_set(font_set_key(), true)?;
            }
            font_string.raw_set(text_key(), button.raw_get::<Option<String>>(text_key())?)?;
            if let Some(measurement) = &font_string_measurement {
                measurement.update_auto_font_string_size(&font_string)?;
            }
            button.raw_set(button_text_key(), font_string.clone())?;
            mark_object_state_changed(lua, &font_string, DIRTY_TEXT)
        })?,
    )?;
    register_texture_pair(
        lua,
        methods,
        "SetNormalTexture",
        "GetNormalTexture",
        normal_texture_key(),
        dynamic_arena.clone(),
    )?;
    register_texture_pair(
        lua,
        methods,
        "SetPushedTexture",
        "GetPushedTexture",
        pushed_texture_key(),
        dynamic_arena.clone(),
    )?;
    register_texture_pair(
        lua,
        methods,
        "SetDisabledTexture",
        "GetDisabledTexture",
        disabled_texture_key(),
        dynamic_arena.clone(),
    )?;
    register_texture_pair(
        lua,
        methods,
        "SetHighlightTexture",
        "GetHighlightTexture",
        highlight_texture_key(),
        dynamic_arena,
    )?;
    register_font_pair(
        lua,
        methods,
        "SetDisabledFontObject",
        "GetDisabledFontObject",
        disabled_font_key(),
        false,
    )?;
    register_font_pair(
        lua,
        methods,
        "SetHighlightFontObject",
        "GetHighlightFontObject",
        highlight_font_key(),
        false,
    )?;
    let set_text_measurement = measurement.clone();
    methods.raw_set(
        "SetText",
        lua.create_function(move |lua, (button, value): (Table, Value)| {
            set_button_text(
                lua,
                &button,
                lua_text(lua, value)?,
                set_text_measurement.as_ref(),
            )
        })?,
    )?;
    let formatted_text_measurement = measurement.clone();
    methods.raw_set(
        "SetFormattedText",
        lua.create_function(move |lua, (button, arguments): (Table, Variadic<Value>)| {
            let library: Table = lua.globals().raw_get("string")?;
            let format: mlua::Function = library.raw_get("format")?;
            let text = format.call::<String>(arguments)?;
            set_button_text(
                lua,
                &button,
                Some(text),
                formatted_text_measurement.as_ref(),
            )
        })?,
    )?;
    methods.raw_set(
        "GetText",
        lua.create_function(|_, button: Table| {
            let text = button.raw_get::<Option<String>>(text_key())?;
            Ok(text.filter(|text| !text.is_empty()))
        })?,
    )?;
    methods.raw_set(
        "GetButtonState",
        lua.create_function(|_, button: Table| {
            Ok(if button.raw_get::<bool>(button_pressed_key())? {
                "PUSHED"
            } else {
                "NORMAL"
            })
        })?,
    )?;
    methods.raw_set(
        "SetButtonState",
        lua.create_function(
            |lua, (button, state, locked): (Table, String, Option<Value>)| {
                let pushed = match state.as_str() {
                    "NORMAL" => false,
                    "PUSHED" => true,
                    _ => {
                        return Err(mlua::Error::runtime(
                            "Usage: Button:SetButtonState(\"NORMAL\" or \"PUSHED\" [, lock])",
                        ));
                    }
                };
                let state_locked = locked.as_ref().is_some_and(|value| lua_bool(value, false));
                let changed = button.raw_get::<bool>(button_pressed_key())? != pushed
                    || button.raw_get::<bool>(button_state_locked_key())? != state_locked;
                button.raw_set(button_pressed_key(), pushed)?;
                button.raw_set(button_state_locked_key(), state_locked)?;
                if changed {
                    mark_object_state_changed(lua, &button, DIRTY_WIDGET)?;
                }
                Ok(())
            },
        )?,
    )?;
    methods.raw_set(
        "LockHighlight",
        lua.create_function(|lua, button: Table| {
            if !button.raw_get::<bool>(highlight_locked_key())? {
                button.raw_set(highlight_locked_key(), true)?;
                mark_object_state_changed(lua, &button, DIRTY_WIDGET)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "UnlockHighlight",
        lua.create_function(|lua, button: Table| {
            if button.raw_get::<bool>(highlight_locked_key())? {
                button.raw_set(highlight_locked_key(), false)?;
                mark_object_state_changed(lua, &button, DIRTY_WIDGET)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "RegisterForClicks",
        lua.create_function(|lua, (button, arguments): (Table, Variadic<Value>)| {
            let mut action = 0_u64;
            for value in arguments {
                let Some(value) = lua.coerce_string(value)? else {
                    break;
                };
                action |= click_action(value.to_string_lossy().as_str());
            }
            button.raw_set(click_action_key(), action)?;
            mark_object_state_changed(lua, &button, DIRTY_WIDGET)
        })?,
    )?;
    methods.raw_set(
        "Click",
        lua.create_function(
            |lua, (button, mouse_button, down): (Table, Option<String>, Option<bool>)| {
                let mouse_button = mouse_button.unwrap_or_else(|| "LeftButton".to_owned());
                let down = down.unwrap_or(false);
                dispatch_click(lua, &button, &mouse_button, down)
            },
        )?,
    )?;
    methods.raw_set(
        "GetTextWidth",
        lua.create_function(move |_, button: Table| {
            let Some(measurement) = &width_measurement else {
                return Err(mlua::Error::runtime(
                    "Button:GetTextWidth requires a mounted stock asset store",
                ));
            };
            measurement.width(&button)
        })?,
    )?;
    methods.raw_set(
        "GetTextHeight",
        lua.create_function(move |_, button: Table| {
            let Some(measurement) = &measurement else {
                return Err(mlua::Error::runtime(
                    "Button:GetTextHeight requires a mounted stock asset store",
                ));
            };
            measurement.height(&button)
        })?,
    )
}

fn set_button_text(
    lua: &Lua,
    button: &Table,
    text: Option<String>,
    measurement: Option<&TextMeasurement>,
) -> mlua::Result<()> {
    if button.raw_get::<Option<String>>(text_key())? == text {
        return Ok(());
    }
    button.raw_set(text_key(), text.as_deref())?;
    if let Some(font_string) = button.raw_get::<Option<Table>>(button_text_key())? {
        font_string.raw_set(text_key(), text.as_deref())?;
        if let Some(measurement) = measurement {
            measurement.update_auto_font_string_size(&font_string)?;
        }
        return mark_object_state_changed(lua, &font_string, DIRTY_TEXT);
    }
    mark_live_state_changed(lua)
}

fn call_click_handler(
    lua: &Lua,
    function: &mlua::Function,
    button: Table,
    mouse_button: &str,
    down: bool,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    let previous_arg1 = globals.raw_get::<Value>("arg1")?;
    let previous_arg2 = globals.raw_get::<Value>("arg2")?;
    globals.raw_set("this", button.clone())?;
    globals.raw_set("arg1", mouse_button)?;
    globals.raw_set("arg2", down)?;
    let result = function.call::<()>((button, mouse_button, down));
    let restore_this = globals.raw_set("this", previous_this);
    let restore_arg1 = globals.raw_set("arg1", previous_arg1);
    let restore_arg2 = globals.raw_set("arg2", previous_arg2);
    match result {
        Ok(()) => {
            restore_this?;
            restore_arg1?;
            restore_arg2
        }
        Err(error) => {
            let _ = restore_this;
            let _ = restore_arg1;
            let _ = restore_arg2;
            Err(error)
        }
    }
}

/// Registers frame-wide drag initiation buttons.
pub(super) fn register_drag_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "RegisterForDrag",
        lua.create_function(|lua, (frame, arguments): (Table, Variadic<Value>)| {
            let mut buttons = 0_u8;
            for value in arguments {
                let Some(value) = lua.coerce_string(value)? else {
                    break;
                };
                buttons |= drag_button(value.to_string_lossy().as_str());
            }
            frame.raw_set(drag_button_key(), buttons)
        })?,
    )
}

pub(super) fn register_check_button_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetChecked",
        lua.create_function(|lua, (button, arguments): (Table, Variadic<Value>)| {
            // Build 12340 clears the state for an omitted value, explicit nil,
            // false, or any numeric value coercing to zero. Every other Lua
            // value selects the checked state.
            let checked = match arguments.first() {
                None | Some(Value::Nil | Value::Boolean(false)) => false,
                Some(value) => !lua
                    .coerce_number(value.clone())?
                    .is_some_and(|number| number == 0.0),
            };
            if button.raw_get::<bool>(checked_key())? != checked {
                button.raw_set(checked_key(), checked)?;
                mark_object_state_changed(lua, &button, DIRTY_WIDGET)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "GetChecked",
        lua.create_function(|_, button: Table| button.raw_get::<bool>(checked_key()))?,
    )
}

fn register_font_pair(
    lua: &Lua,
    methods: &Table,
    setter: &'static str,
    getter: &'static str,
    key: LightUserData,
    propagate_button_text: bool,
) -> mlua::Result<()> {
    methods.raw_set(
        setter,
        lua.create_function(move |lua, (button, font): (Table, Value)| {
            let font = resolve_font_object(lua, font).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "Usage: {}:{setter}(\"fontname\" or fontObject)",
                    button
                        .raw_get::<Option<String>>(name_key())
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "<unnamed>".to_owned())
                ))
            })?;
            button.raw_set(key, font.clone())?;
            if let Some(text) = button.raw_get::<Option<Table>>(button_text_key())? {
                if propagate_button_text {
                    text.raw_set(font_object_key(), font)?;
                    text.raw_set(font_set_key(), true)?;
                }
                return mark_object_state_changed(lua, &text, DIRTY_TEXT);
            }
            mark_live_state_changed(lua)
        })?,
    )?;
    methods.raw_set(
        getter,
        lua.create_function(move |_, button: Table| button.raw_get::<Option<Table>>(key))?,
    )
}

/// Registers one button texture slot with stock string/object overloads.
fn register_texture_pair(
    lua: &Lua,
    methods: &Table,
    setter: &'static str,
    getter: &'static str,
    key: LightUserData,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<()> {
    methods.raw_set(
        setter,
        lua.create_function(move |lua, (button, value): (Table, Value)| {
            let texture = match value {
                Value::Nil => {
                    button.raw_set(key, Option::<Table>::None)?;
                    mark_live_state_changed(lua)?;
                    return Ok(());
                }
                Value::Table(texture) => {
                    if texture.raw_get::<String>(type_key())? != "Texture" {
                        return Err(button_texture_usage(&button, setter));
                    }
                    texture
                }
                Value::String(path) => {
                    let texture = match button.raw_get::<Option<Table>>(key)? {
                        Some(texture) => texture,
                        None => create_dynamic_region(
                            lua,
                            "Texture",
                            button.clone(),
                            None,
                            None,
                            None,
                            None,
                            &dynamic_arena.counters(),
                        )?,
                    };
                    let path = path.to_string_lossy();
                    texture.raw_set(texture_file_key(), (!path.is_empty()).then_some(path))?;
                    texture.raw_set(texture_solid_color_key(), Option::<Table>::None)?;
                    texture
                }
                _ => return Err(button_texture_usage(&button, setter)),
            };
            button.raw_set(key, texture.clone())?;
            mark_object_state_changed(lua, &texture, DIRTY_TEXTURE)
        })?,
    )?;
    methods.raw_set(
        getter,
        lua.create_function(move |_, button: Table| button.raw_get::<Option<Table>>(key))?,
    )
}

/// Produces the native-style method contract without accepting other objects.
fn button_texture_usage(button: &Table, setter: &str) -> mlua::Error {
    let name = button
        .raw_get::<Option<String>>(name_key())
        .ok()
        .flatten()
        .unwrap_or_else(|| "<unnamed>".to_owned());
    mlua::Error::runtime(format!(
        "Usage: {name}:{setter}(\"filename\" or textureObject)"
    ))
}

/// Reproduces build 12340's recognized `StringToClickAction` names.
fn click_action(value: &str) -> u64 {
    if value.eq_ignore_ascii_case("LeftButtonDown") {
        1
    } else if value.eq_ignore_ascii_case("LeftButtonUp") {
        0x8000_0000
    } else if value.eq_ignore_ascii_case("MiddleButtonDown") {
        2
    } else if value.eq_ignore_ascii_case("RightButtonDown") {
        4
    } else {
        0
    }
}

/// Reproduces the five pointer-button names accepted by frame drag routing.
fn drag_button(value: &str) -> u8 {
    if value.eq_ignore_ascii_case("LeftButton") {
        1
    } else if value.eq_ignore_ascii_case("RightButton") {
        2
    } else if value.eq_ignore_ascii_case("MiddleButton") {
        4
    } else if value.eq_ignore_ascii_case("Button4") {
        8
    } else if value.eq_ignore_ascii_case("Button5") {
        16
    } else {
        0
    }
}

//! Button and check-button methods shared by static and dynamic objects.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{LightUserData, Lua, Table, Value, Variadic};
use solarity_asset::AssetStoreHandle;

use crate::{FontDefinition, FontRasterization, FontSystem};

use super::{
    DynamicArenaState, button_text_key, checked_key, click_action_key, create_dynamic_region,
    disabled_font_key, disabled_texture_key, drag_button_key, font_object_key, font_set_key,
    highlight_font_key, highlight_locked_key, highlight_texture_key, lua_bool, lua_text, name_key,
    normal_font_key, normal_texture_key, pushed_texture_key, resolve_font_object, text_key,
    texture_file_key, texture_solid_color_key, type_key,
};

/// Archive-backed state required by the stock text-extent methods.
#[derive(Clone)]
pub(super) struct ButtonTextMeasurement {
    assets: AssetStoreHandle,
    fonts: Rc<HashMap<String, FontDefinition>>,
    system: Rc<RefCell<FontSystem>>,
    pixels_per_ui_unit: f64,
}

impl ButtonTextMeasurement {
    pub(super) fn new(
        assets: AssetStoreHandle,
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
                "Button:GetTextWidth font object {name} has no stock definition"
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
        let mut assets = self.assets.borrow_mut();
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

    fn one_pixel(&self) -> f64 {
        self.pixels_per_ui_unit.recip()
    }
}

pub(super) fn register_button_methods(
    lua: &Lua,
    methods: &Table,
    measurement: Option<ButtonTextMeasurement>,
    dynamic_arena: DynamicArenaState,
) -> mlua::Result<()> {
    register_font_pair(
        lua,
        methods,
        "SetNormalFontObject",
        "GetNormalFontObject",
        normal_font_key(),
        true,
    )?;
    methods.raw_set(
        "GetFontString",
        lua.create_function(|_, button: Table| button.raw_get::<Option<Table>>(button_text_key()))?,
    )?;
    methods.raw_set(
        "SetFontString",
        lua.create_function(|_, (button, font_string): (Table, Table)| {
            if font_string.raw_get::<String>(type_key())? != "FontString" {
                return Err(mlua::Error::runtime(
                    "Usage: Button:SetFontString(fontString)",
                ));
            }
            if let Some(font) = button.raw_get::<Option<Table>>(normal_font_key())? {
                font_string.raw_set(font_object_key(), font)?;
                font_string.raw_set(font_set_key(), true)?;
            }
            button.raw_set(button_text_key(), font_string)
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
    methods.raw_set(
        "SetText",
        lua.create_function(|lua, (button, value): (Table, Value)| {
            button.raw_set(text_key(), lua_text(lua, value)?)
        })?,
    )?;
    methods.raw_set(
        "SetFormattedText",
        lua.create_function(|lua, (button, arguments): (Table, Variadic<Value>)| {
            let library: Table = lua.globals().raw_get("string")?;
            let format: mlua::Function = library.raw_get("format")?;
            let text = format.call::<String>(arguments)?;
            button.raw_set(text_key(), text)
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
        "LockHighlight",
        lua.create_function(|_, button: Table| button.raw_set(highlight_locked_key(), true))?,
    )?;
    methods.raw_set(
        "UnlockHighlight",
        lua.create_function(|_, button: Table| button.raw_set(highlight_locked_key(), false))?,
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
            button.raw_set(click_action_key(), action)
        })?,
    )?;
    methods.raw_set(
        "RegisterForDrag",
        lua.create_function(|lua, (button, arguments): (Table, Variadic<Value>)| {
            let mut buttons = 0_u8;
            for value in arguments {
                let Some(value) = lua.coerce_string(value)? else {
                    break;
                };
                buttons |= drag_button(value.to_string_lossy().as_str());
            }
            button.raw_set(drag_button_key(), buttons)
        })?,
    )?;
    methods.raw_set(
        "GetTextWidth",
        lua.create_function(move |_, button: Table| {
            let Some(measurement) = &measurement else {
                return Err(mlua::Error::runtime(
                    "Button:GetTextWidth requires a mounted stock asset store",
                ));
            };
            measurement.width(&button)
        })?,
    )
}

pub(super) fn register_check_button_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetChecked",
        lua.create_function(|_, (button, arguments): (Table, Variadic<Value>)| {
            let checked = arguments.first().is_none_or(|value| lua_bool(value, true));
            button.raw_set(checked_key(), checked)
        })?,
    )?;
    methods.raw_set(
        "GetChecked",
        lua.create_function(|_, button: Table| {
            Ok(button
                .raw_get::<bool>(checked_key())?
                .then_some(Value::Number(1.0)))
        })?,
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
            if propagate_button_text
                && let Some(text) = button.raw_get::<Option<Table>>(button_text_key())?
            {
                text.raw_set(font_object_key(), font)?;
                text.raw_set(font_set_key(), true)?;
            }
            Ok(())
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
            button.raw_set(key, texture)
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

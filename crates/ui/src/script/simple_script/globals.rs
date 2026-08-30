//! Process-independent globals installed before built-in UI execution.

use mlua::{Function, Lua, MultiValue, Table, Value, Variadic};

use crate::UiManifestKind;

use super::UiScriptEnvironment;
use super::cvars::UiCVarSetError;
use super::type_key;

const ERROR_HANDLER_REGISTRY: &str = "solarity.ui.error_handler";
const CHARACTER_SELECT_MODEL_REGISTRY: &str = "solarity.ui.character_select_model";
const CHARACTER_CUSTOMIZE_MODEL_REGISTRY: &str = "solarity.ui.character_customize_model";

pub(super) fn register_base_globals(
    lua: &Lua,
    environment: &UiScriptEnvironment,
    manifest_kind: UiManifestKind,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let (screen_width, screen_height) = environment.ui_extent();
    globals.raw_set(
        "GetScreenWidth",
        lua.create_function(move |_, ()| Ok(screen_width))?,
    )?;
    globals.raw_set(
        "GetScreenHeight",
        lua.create_function(move |_, ()| Ok(screen_height))?,
    )?;
    globals.raw_set(
        "IsWindowsClient",
        lua.create_function(|_, ()| Ok(cfg!(target_os = "windows").then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "IsMacClient",
        lua.create_function(|_, ()| Ok(cfg!(target_os = "macos").then_some(1_u32)))?,
    )?;
    globals.raw_set(
        "IsLinuxClient",
        lua.create_function(|_, ()| Ok(cfg!(target_os = "linux").then_some(1_u32)))?,
    )?;
    // Manifest-owned FrameXML and GlueXML execute in the stock secure context.
    // Add-on provenance will extend this state when untrusted manifests load.
    globals.raw_set("issecure", lua.create_function(|_, ()| Ok(true))?)?;
    globals.raw_set("securecall", create_secure_call(lua, "securecall")?)?;
    globals.raw_set(
        "securecallfunction",
        create_secure_call(lua, "securecallfunction")?,
    )?;
    if manifest_kind == UiManifestKind::Glue {
        register_glue_globals(lua, &globals, environment)?;
    }
    globals.raw_set(
        "seterrorhandler",
        lua.create_function(|lua, handler: Value| {
            let Value::Function(handler) = handler else {
                return Err(mlua::Error::runtime("Usage: seterrorhandler(errfunc)"));
            };
            lua.set_named_registry_value(ERROR_HANDLER_REGISTRY, handler)
        })?,
    )?;
    globals.raw_set(
        "geterrorhandler",
        lua.create_function(|lua, ()| {
            lua.named_registry_value::<Option<Function>>(ERROR_HANDLER_REGISTRY)
        })?,
    )?;
    register_table_wipe(lua)?;
    lua.load(COMPATIBILITY_SOURCE)
        .set_name("compat.lua")
        .exec()?;
    Ok(())
}

fn create_secure_call(lua: &Lua, name: &'static str) -> mlua::Result<Function> {
    lua.create_function(move |lua, (target, arguments): (Value, Variadic<Value>)| {
        let function = match target {
            Value::Function(function) => function,
            Value::String(global_name) => lua
                .globals()
                .raw_get::<Function>(global_name.to_str()?)
                .map_err(|_| mlua::Error::runtime(format!("Usage: {name}(function, ...)")))?,
            _ => {
                return Err(mlua::Error::runtime(format!(
                    "Usage: {name}(function, ...)"
                )));
            }
        };
        function.call::<MultiValue>(arguments)
    })
}

fn register_glue_globals(
    lua: &Lua,
    globals: &Table,
    environment: &UiScriptEnvironment,
) -> mlua::Result<()> {
    let character_count = environment.initial_character_count();
    globals.raw_set(
        "GetNumCharacters",
        lua.create_function(move |_, ()| Ok(character_count))?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetSavedAccountName",
        lua.create_function(move |_, ()| {
            cvars
                .get("accountName")
                .ok_or_else(|| mlua::Error::runtime("stock accountName CVar is not registered"))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "SetSavedAccountName",
        lua.create_function(move |lua, value: Value| {
            let Some(value) = lua.coerce_string(value)? else {
                return Err(mlua::Error::runtime(
                    "Usage: SetSavedAccountName(\"accountName\")",
                ));
            };
            set_cvar(&cvars, "accountName", value.to_string_lossy())
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetSavedAccountList",
        lua.create_function(move |_, ()| {
            cvars
                .get("accountList")
                .ok_or_else(|| mlua::Error::runtime("stock accountList CVar is not registered"))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVar",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVar")?;
            cvars
                .get(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("Couldn't find CVar named '{name}'")))
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "SetCVar",
        lua.create_function(move |lua, (name, value): (Value, Value)| {
            let name = cvar_name(lua, name, "SetCVar")?;
            let value = lua
                .coerce_string(value)?
                .map_or_else(|| "0".to_owned(), |value| value.to_string_lossy());
            set_cvar(&cvars, &name, value)
        })?,
    )?;
    let cvars = environment.cvars();
    globals.raw_set(
        "GetCVarDefault",
        lua.create_function(move |lua, value: Value| {
            let name = cvar_name(lua, value, "GetCVarDefault")?;
            cvars
                .default_value(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("Couldn't find CVar named '{name}'")))
        })?,
    )?;
    globals.raw_set(
        "GetBuildInfo",
        lua.create_function(|lua, ()| {
            let globals = lua.globals();
            Ok((
                globals.raw_get::<String>("VERSION")?,
                globals.raw_get::<String>("RELEASE_BUILD")?,
                "3.3.5",
                "12340",
                "Jun 24 2010",
            ))
        })?,
    )?;
    let streaming_trial = environment.streaming_trial();
    globals.raw_set(
        "IsStreamingTrial",
        lua.create_function(move |_, ()| Ok(streaming_trial.then_some(Value::Number(1.0))))?,
    )?;
    globals.raw_set(
        "GetClientExpansionLevel",
        lua.create_function(|_, ()| Ok(3_u32))?,
    )?;
    // The headless validation environment has no platform-enumerated display
    // modes. Build 12340 explicitly selects the first entry in that state.
    globals.raw_set(
        "GetCurrentResolution",
        lua.create_function(|_, ()| Ok(1_u32))?,
    )?;
    globals.raw_set("GetScreenResolutions", lua.create_function(|_, ()| Ok(()))?)?;
    globals.raw_set("GetRefreshRates", lua.create_function(|_, ()| Ok(()))?)?;
    globals.raw_set(
        "GetCurrentMultisampleFormat",
        lua.create_function(|_, ()| Ok(1_u32))?,
    )?;
    globals.raw_set(
        "GetMultisampleFormats",
        lua.create_function(|_, ()| Ok(()))?,
    )?;
    // No stereo-capable display surface is attached to the headless glue
    // validator, so build 12340 reports the feature as unavailable.
    globals.raw_set(
        "IsStereoVideoAvailable",
        lua.create_function(|_, ()| Ok(None::<u32>))?,
    )?;
    // The platform audio service has not attached devices to this headless
    // environment. Stock represents that state as an empty indexed list.
    globals.raw_set(
        "Sound_GameSystem_GetNumOutputDrivers",
        lua.create_function(|_, ()| Ok(0_u32))?,
    )?;
    globals.raw_set(
        "Sound_GameSystem_GetOutputDriverNameByIndex",
        lua.create_function(|_, _index: u32| Ok(None::<String>))?,
    )?;
    register_model_frame_selector(
        lua,
        globals,
        "SetCharSelectModelFrame",
        CHARACTER_SELECT_MODEL_REGISTRY,
    )?;
    register_model_frame_selector(
        lua,
        globals,
        "SetCharCustomizeFrame",
        CHARACTER_CUSTOMIZE_MODEL_REGISTRY,
    )
}

fn register_model_frame_selector(
    lua: &Lua,
    globals: &Table,
    function: &'static str,
    registry: &'static str,
) -> mlua::Result<()> {
    globals.raw_set(
        function,
        lua.create_function(move |lua, value: Value| {
            let Some(name) = lua.coerce_string(value)? else {
                return Err(mlua::Error::runtime(format!(
                    "Usage: {function}(\"frameName\")"
                )));
            };
            let globals = lua.globals();
            let frame = globals.raw_get::<Option<Table>>(name.to_str()?)?;
            let Some(frame) = frame else {
                return Ok(());
            };
            let kind = frame.raw_get::<String>(type_key())?;
            if matches!(kind.as_str(), "Model" | "ModelFFX") {
                lua.set_named_registry_value(registry, frame)?;
            }
            Ok(())
        })?,
    )
}

fn cvar_name(lua: &Lua, value: Value, function: &str) -> mlua::Result<String> {
    let Some(name) = lua.coerce_string(value)? else {
        return Err(mlua::Error::runtime(format!("Usage: {function}(\"cvar\")")));
    };
    Ok(name.to_string_lossy())
}

fn set_cvar(cvars: &super::cvars::UiCVarRegistry, name: &str, value: String) -> mlua::Result<()> {
    match cvars.set(name, value) {
        Ok(()) => Ok(()),
        Err(UiCVarSetError::Missing) => Err(mlua::Error::runtime(format!(
            "Couldn't find CVar named '{name}'"
        ))),
        Err(UiCVarSetError::ReadOnly) => {
            Err(mlua::Error::runtime(format!("\"{name}\" is read-only")))
        }
    }
}

fn register_table_wipe(lua: &Lua) -> mlua::Result<()> {
    let table: Table = lua.globals().raw_get("table")?;
    table.raw_set(
        "wipe",
        lua.create_function(|_, table: Table| {
            let keys = table
                .pairs::<Value, Value>()
                .map(|entry| entry.map(|(key, _)| key))
                .collect::<mlua::Result<Vec<_>>>()?;
            for key in keys {
                table.raw_set(key, Value::Nil)?;
            }
            Ok(table)
        })?,
    )?;
    Ok(())
}

const COMPATIBILITY_SOURCE: &str = r#"
local tab = table
foreach = tab.foreach
foreachi = tab.foreachi
getn = tab.getn
tinsert = tab.insert
tremove = tab.remove
sort = tab.sort
wipe = tab.wipe

local math = math
abs = math.abs
acos = function (x) return math.deg(math.acos(x)) end
asin = function (x) return math.deg(math.asin(x)) end
atan = function (x) return math.deg(math.atan(x)) end
atan2 = function (x,y) return math.deg(math.atan2(x,y)) end
ceil = math.ceil
cos = function (x) return math.cos(math.rad(x)) end
deg = math.deg
exp = math.exp
floor = math.floor
frexp = math.frexp
ldexp = math.ldexp
log = math.log
log10 = math.log10
max = math.max
min = math.min
mod = math.fmod
PI = math.pi
rad = math.rad
random = math.random
sin = function (x) return math.sin(math.rad(x)) end
sqrt = math.sqrt
tan = function (x) return math.tan(math.rad(x)) end

local str = string
strbyte = str.byte
strchar = str.char
strfind = str.find
format = str.format
gmatch = str.gmatch
gsub = str.gsub
strlen = str.len
strlower = str.lower
strmatch = str.match
strrep = str.rep
strrev = str.reverse
strsub = str.sub
strupper = str.upper
"#;

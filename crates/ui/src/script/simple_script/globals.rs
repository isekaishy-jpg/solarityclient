//! Process-independent globals installed before built-in UI execution.

use mlua::{Function, Lua, Table, Value};

use super::UiScriptEnvironment;

const ERROR_HANDLER_REGISTRY: &str = "solarity.ui.error_handler";

pub(super) fn register_base_globals(
    lua: &Lua,
    environment: UiScriptEnvironment,
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
    )
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

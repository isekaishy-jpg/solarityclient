//! World-map native helper objects and map presentation state.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;

use mlua::{Function, LightUserData, Lua, MultiValue, Table, Value};

// Distinct addresses provide private per-object fields without exposing
// implementation keys to addon code through ordinary string indexing.
static FILL_TEXTURE_TOKEN: u8 = 1;
static BORDER_TEXTURE_TOKEN: u8 = 2;
static FILL_ALPHA_TOKEN: u8 = 3;
static BORDER_ALPHA_TOKEN: u8 = 4;
static BORDER_SCALAR_TOKEN: u8 = 5;
static DRAWN_QUEST_TOKEN: u8 = 6;

/// Native world-map helper bindings established during FrameXML initialization.
#[derive(Clone, Debug, Default)]
pub struct UiWorldMapState {
    ping_extent: Rc<Cell<Option<(f64, f64)>>>,
    map_file: Rc<RefCell<Option<String>>>,
    texture_height: Rc<Cell<Option<u32>>>,
    current_continent: Rc<Cell<i32>>,
    current_zone: Rc<Cell<u32>>,
    current_area_id: Rc<Cell<u32>>,
    current_dungeon_level: Rc<Cell<u32>>,
    dungeon_level_count: Rc<Cell<u32>>,
    terrain_map: Rc<Cell<bool>>,
    zoom_out_available: Rc<Cell<bool>>,
    landmark_count: Rc<Cell<u32>>,
    overlay_count: Rc<Cell<u32>>,
    debug_zone_map_available: Rc<Cell<bool>>,
    debug_object_count: Rc<Cell<u32>>,
}

impl UiWorldMapState {
    /// Creates the uninitialized world-map helper state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the map-ping region extent captured by `InitWorldMapPing`.
    #[must_use]
    pub fn ping_extent(&self) -> Option<(f64, f64)> {
        self.ping_extent.get()
    }

    /// Replaces the selected map file, continent, and dungeon-floor projection.
    pub fn set_selection(
        &self,
        map_file: Option<String>,
        texture_height: Option<u32>,
        continent: i32,
        dungeon_level: u32,
        terrain_map: bool,
    ) {
        *self.map_file.borrow_mut() = map_file;
        self.texture_height.set(texture_height);
        self.current_continent.set(continent);
        self.current_dungeon_level.set(dungeon_level);
        self.terrain_map.set(terrain_map);
    }

    /// Replaces the selected zone, area, and available dungeon-floor count.
    pub fn set_location(&self, zone: u32, area_id: u32, dungeon_level_count: u32) {
        self.current_zone.set(zone);
        self.current_area_id.set(area_id);
        self.dungeon_level_count.set(dungeon_level_count);
    }

    /// Replaces whether the current map has a parent zoom level.
    pub fn set_zoom_out_available(&self, available: bool) {
        self.zoom_out_available.set(available);
    }
}

/// Registers native world-map object constructors used by FrameXML.
pub(crate) fn register_globals(
    lua: &Lua,
    globals: &Table,
    state: UiWorldMapState,
) -> mlua::Result<()> {
    globals.raw_set(
        "SetMapToCurrentZone",
        lua.create_function(|_, ()| {
            // Script_SetMapToCurrentZone is the 0x00547C10 thunk into the
            // native map selector. Runtime publication already keeps this
            // boundary on the current player zone until map navigation exists.
            Ok(())
        })?,
    )?;
    globals.raw_set(
        "CreateWorldMapArrowFrame",
        lua.create_function(move |lua, parent: Table| {
            let create_frame: Function = lua.globals().raw_get("CreateFrame")?;
            create_frame.call::<Table>((
                "Frame",
                Some("PlayerArrowEffectFrame"),
                Some(parent),
                Option::<String>::None,
            ))?;
            Ok(())
        })?,
    )?;
    let ping_state = state.clone();
    globals.raw_set(
        "InitWorldMapPing",
        lua.create_function(move |lua, _parent: Table| {
            let ping: Table = lua.globals().raw_get("WorldMapPing")?;
            let get_width: Function = ping.get("GetWidth")?;
            let get_height: Function = ping.get("GetHeight")?;
            ping_state.ping_extent.set(Some((
                get_width.call::<f64>(ping.clone())?,
                get_height.call::<f64>(ping)?,
            )));
            Ok(())
        })?,
    )?;
    let map_info = state.clone();
    globals.raw_set(
        "GetMapInfo",
        lua.create_function(move |lua, ()| {
            let file = map_info.map_file.borrow().clone();
            let Some(file) = file else {
                return Ok(MultiValue::new());
            };
            Ok(MultiValue::from_vec(vec![
                Value::String(lua.create_string(file)?),
                map_info
                    .texture_height
                    .get()
                    .map_or(Value::Nil, |height| Value::Integer(i64::from(height))),
            ]))
        })?,
    )?;
    let continent = state.clone();
    globals.raw_set(
        "GetCurrentMapContinent",
        lua.create_function(move |_, ()| Ok(continent.current_continent.get()))?,
    )?;
    let dungeon_level = state.clone();
    globals.raw_set(
        "GetCurrentMapDungeonLevel",
        lua.create_function(move |_, ()| Ok(dungeon_level.current_dungeon_level.get()))?,
    )?;
    let zone = state.clone();
    globals.raw_set(
        "GetCurrentMapZone",
        lua.create_function(move |_, ()| Ok(zone.current_zone.get()))?,
    )?;
    let area = state.clone();
    globals.raw_set(
        "GetCurrentMapAreaID",
        lua.create_function(move |_, ()| Ok(area.current_area_id.get()))?,
    )?;
    let level_count = state.clone();
    globals.raw_set(
        "GetNumDungeonMapLevels",
        lua.create_function(move |_, ()| Ok(level_count.dungeon_level_count.get()))?,
    )?;
    let terrain_map = state.clone();
    globals.raw_set(
        "DungeonUsesTerrainMap",
        lua.create_function(move |_, ()| Ok(terrain_map.terrain_map.get().then_some(1_u8)))?,
    )?;
    let zoom = state.clone();
    globals.raw_set(
        "IsZoomOutAvailable",
        lua.create_function(move |_, ()| Ok(zoom.zoom_out_available.get().then_some(1_u8)))?,
    )?;
    let landmarks = state.clone();
    globals.raw_set(
        "GetNumMapLandmarks",
        lua.create_function(move |_, ()| Ok(landmarks.landmark_count.get()))?,
    )?;
    let overlays = state.clone();
    globals.raw_set(
        "GetNumMapOverlays",
        lua.create_function(move |_, ()| Ok(overlays.overlay_count.get()))?,
    )?;
    // These enumerators intentionally return no values until the DBC map
    // catalog is loaded. Lua observes an empty vararg list, as it does when
    // the native client has no entries for the requested scope.
    globals.raw_set(
        "GetMapContinents",
        lua.create_function(move |_, ()| Ok(MultiValue::new()))?,
    )?;
    globals.raw_set(
        "GetMapZones",
        lua.create_function(move |_, _continent: Value| Ok(MultiValue::new()))?,
    )?;
    let debug_zone_map = state.clone();
    globals.raw_set(
        "HasDebugZoneMap",
        lua.create_function(move |_, ()| {
            Ok(debug_zone_map
                .debug_zone_map_available
                .get()
                .then_some(1_u8))
        })?,
    )?;
    globals.raw_set(
        "GetDebugZoneMap",
        lua.create_function(move |_, (_x, _y): (Value, Value)| Ok(MultiValue::new()))?,
    )?;
    let debug_objects = state.clone();
    globals.raw_set(
        "GetNumMapDebugObjects",
        lua.create_function(move |_, ()| Ok(debug_objects.debug_object_count.get()))?,
    )?;
    globals.raw_set(
        "GetMapDebugObjectInfo",
        lua.create_function(move |_, _index: Value| Ok(MultiValue::new()))?,
    )
}

/// Registers the native `QuestPOIFrame` method family used by the stock map.
pub(crate) fn register_quest_poi_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    register_value_setter(lua, methods, "SetFillTexture", fill_texture_key())?;
    register_value_setter(lua, methods, "SetBorderTexture", border_texture_key())?;
    register_value_setter(lua, methods, "SetFillAlpha", fill_alpha_key())?;
    register_value_setter(lua, methods, "SetBorderAlpha", border_alpha_key())?;
    register_value_setter(lua, methods, "SetBorderScalar", border_scalar_key())?;
    methods.raw_set(
        "DrawQuestBlob",
        lua.create_function(|_, (object, quest_id, draw): (Table, Value, Value)| {
            object.raw_set(
                drawn_quest_key(),
                if !matches!(draw, Value::Nil | Value::Boolean(false)) {
                    quest_id
                } else {
                    Value::Nil
                },
            )
        })?,
    )?;
    // Blob hit-test polygons are generated by the map renderer. With no
    // selected quest geography, the native projection has no tooltip hits.
    methods.raw_set(
        "GetNumTooltips",
        lua.create_function(|_, _object: Table| Ok(0_u8))?,
    )?;
    methods.raw_set(
        "GetTooltipIndex",
        lua.create_function(|_, (_object, _index): (Table, Value)| Ok(Value::Nil))?,
    )
}

fn register_value_setter(
    lua: &Lua,
    methods: &Table,
    name: &str,
    key: LightUserData,
) -> mlua::Result<()> {
    methods.raw_set(
        name,
        lua.create_function(move |_, (object, value): (Table, Value)| object.raw_set(key, value))?,
    )
}

fn hidden_key(token: &'static u8) -> LightUserData {
    LightUserData((token as *const u8).cast_mut().cast::<c_void>())
}

fn fill_texture_key() -> LightUserData {
    hidden_key(&FILL_TEXTURE_TOKEN)
}

fn border_texture_key() -> LightUserData {
    hidden_key(&BORDER_TEXTURE_TOKEN)
}

fn fill_alpha_key() -> LightUserData {
    hidden_key(&FILL_ALPHA_TOKEN)
}

fn border_alpha_key() -> LightUserData {
    hidden_key(&BORDER_ALPHA_TOKEN)
}

fn border_scalar_key() -> LightUserData {
    hidden_key(&BORDER_SCALAR_TOKEN)
}

fn drawn_quest_key() -> LightUserData {
    hidden_key(&DRAWN_QUEST_TOKEN)
}

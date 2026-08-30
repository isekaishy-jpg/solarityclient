//! Ordered Lua source execution and stock object identity methods.

mod globals;

use std::ffi::c_void;

use mlua::{LightUserData, Lua, RegistryKey, Table, Value, Variadic};

use crate::{
    UiAnchorTarget, UiBundle, UiLoadAction, UiManifestKind, UiObjectBatch, UiObjectKind,
    UiObjectTree, UiPoint, UiRegionStatePlan, UiResourceContent, UiScriptError, UiScriptHandler,
    UiScriptPlan, UiScriptTarget,
};

use self::globals::register_base_globals;
use super::script_events::glue_event;

const OBJECT_REGISTRY: &str = "solarity.ui.objects";
// Distinct values prevent identical-data folding from merging these private
// light-userdata keys in optimized builds.
static NAME_TOKEN: u8 = 1;
static TYPE_TOKEN: u8 = 2;
static PARENT_TOKEN: u8 = 3;
static EVENTS_TOKEN: u8 = 4;
static ALL_EVENTS_TOKEN: u8 = 5;
static WIDTH_TOKEN: u8 = 6;
static HEIGHT_TOKEN: u8 = 7;
static BACKDROP_COLOR_TOKEN: u8 = 8;
static BACKDROP_BORDER_COLOR_TOKEN: u8 = 9;
static INDEX_TOKEN: u8 = 10;
static ANCHORS_TOKEN: u8 = 11;
static ENABLED_TOKEN: u8 = 12;
static HORIZONTAL_SCROLL_TOKEN: u8 = 13;
static VERTICAL_SCROLL_TOKEN: u8 = 14;
static HORIZONTAL_SCROLL_RANGE_TOKEN: u8 = 15;
static VERTICAL_SCROLL_RANGE_TOKEN: u8 = 16;
static SLIDER_MIN_TOKEN: u8 = 17;
static SLIDER_MAX_TOKEN: u8 = 18;
static SLIDER_VALUE_TOKEN: u8 = 19;
static SLIDER_STEP_TOKEN: u8 = 20;
static SHOWN_TOKEN: u8 = 21;

const OBJECT_KINDS: [UiObjectKind; 20] = [
    UiObjectKind::Frame,
    UiObjectKind::Button,
    UiObjectKind::CheckButton,
    UiObjectKind::ColorSelect,
    UiObjectKind::Cooldown,
    UiObjectKind::EditBox,
    UiObjectKind::FontString,
    UiObjectKind::GameTooltip,
    UiObjectKind::MessageFrame,
    UiObjectKind::Minimap,
    UiObjectKind::Model,
    UiObjectKind::ModelFfx,
    UiObjectKind::MovieFrame,
    UiObjectKind::ScrollFrame,
    UiObjectKind::ScrollingMessageFrame,
    UiObjectKind::SimpleHtml,
    UiObjectKind::Slider,
    UiObjectKind::StatusBar,
    UiObjectKind::Texture,
    UiObjectKind::WorldFrame,
];

#[derive(Clone, Copy)]
struct InitialAnchor {
    point: UiPoint,
    target: Option<usize>,
    relative_point: UiPoint,
    offset: (f64, f64),
}

/// Incremental executor for one built-in UI bundle.
///
/// Object tables are registered at their exact XML action rather than before
/// execution begins. The initial identity methods are shared by one metatable;
/// concrete widget method tables extend this boundary in later stages.
pub struct UiScriptRuntime {
    next_action: usize,
    object_metatables: Vec<RegistryKey>,
    region_dimensions: Vec<(f64, f64)>,
    region_shown: Vec<bool>,
    region_anchors: Vec<Vec<InitialAnchor>>,
    registered_objects: usize,
    executed_chunks: usize,
    executed_load_handlers: usize,
}

/// Immutable process facts required by built-in Lua globals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiScriptEnvironment {
    logical_extent: (u32, u32),
    ui_extent: (f64, f64),
}

impl UiScriptEnvironment {
    /// Derives the stock 768-unit UI canvas from a logical window extent.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Plan`] when either logical dimension is zero.
    pub fn new(logical_width: u32, logical_height: u32) -> Result<Self, UiScriptError> {
        if logical_width == 0 || logical_height == 0 {
            return Err(UiScriptError::Plan {
                message: "UI script logical window extent must be nonzero".to_owned(),
            });
        }
        let ui_height = 768.0;
        let ui_width = f64::from(logical_width) / f64::from(logical_height) * ui_height;
        Ok(Self {
            logical_extent: (logical_width, logical_height),
            ui_extent: (ui_width, ui_height),
        })
    }

    /// Returns the SDL logical window extent used to derive UI coordinates.
    #[must_use]
    pub const fn logical_extent(self) -> (u32, u32) {
        self.logical_extent
    }

    /// Returns the stock aspect-compensated UI width and fixed 768-unit height.
    #[must_use]
    pub const fn ui_extent(self) -> (f64, f64) {
        self.ui_extent
    }
}

impl UiScriptRuntime {
    /// Creates an empty object registry and the stock identity method table.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Execution`] when Lua cannot allocate or retain
    /// the registry and metatable objects.
    pub fn new(
        bundle: &UiBundle,
        regions: &UiRegionStatePlan,
        environment: UiScriptEnvironment,
    ) -> Result<Self, UiScriptError> {
        let lua = bundle.lua();
        register_base_globals(lua, environment)
            .map_err(|error| execution_error("base globals", error))?;
        let objects = lua
            .create_table()
            .map_err(|error| execution_error("registry", error))?;
        lua.set_named_registry_value(OBJECT_REGISTRY, objects)
            .map_err(|error| execution_error("registry", error))?;
        let object_metatables = OBJECT_KINDS
            .into_iter()
            .map(|kind| {
                let metatable = create_object_metatable(lua, bundle.manifest().kind(), kind)
                    .map_err(|error| execution_error("object metatable", error))?;
                lua.create_registry_value(metatable)
                    .map_err(|error| execution_error("object metatable", error))
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_dimensions = (0..regions.state_count())
            .map(|index| {
                let state = regions.state(index).ok_or_else(|| UiScriptError::Plan {
                    message: format!("region state {index} is outside the arena"),
                })?;
                Ok((f64::from(state.width()), f64::from(state.height())))
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_shown = (0..regions.state_count())
            .map(|index| {
                regions
                    .state(index)
                    .map(|state| state.shown())
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_anchors = (0..regions.state_count())
            .map(|index| {
                let state = regions.state(index).ok_or_else(|| UiScriptError::Plan {
                    message: format!("region state {index} is outside the arena"),
                })?;
                Ok(regions
                    .anchors_for(state)
                    .iter()
                    .map(|anchor| InitialAnchor {
                        point: anchor.point(),
                        target: match anchor.target() {
                            UiAnchorTarget::Screen => None,
                            UiAnchorTarget::Object(target) => Some(target + 1),
                        },
                        relative_point: anchor.relative_point(),
                        offset: (f64::from(anchor.offset().0), f64::from(anchor.offset().1)),
                    })
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        Ok(Self {
            next_action: 0,
            object_metatables,
            region_dimensions,
            region_shown,
            region_anchors,
            registered_objects: 0,
            executed_chunks: 0,
            executed_load_handlers: 0,
        })
    }

    /// Executes one expanded bundle action and advances the stock load cursor.
    ///
    /// Returns `false` only when every action has already completed.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] for mismatched plans, object registration
    /// failures, missing named handlers, or a Lua runtime error. The cursor
    /// remains on the failing action for deterministic diagnostics.
    pub fn execute_next(
        &mut self,
        bundle: &UiBundle,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
    ) -> Result<bool, UiScriptError> {
        let Some(action) = bundle.actions().get(self.next_action) else {
            return Ok(false);
        };
        match action {
            UiLoadAction::XmlElement { .. } => {
                if let Some(batch) = tree.batch_for_action(self.next_action) {
                    self.execute_batch(bundle.lua(), tree, scripts, batch)?;
                }
            }
            UiLoadAction::LuaResource { resource_index } => {
                let resource =
                    bundle
                        .resource(*resource_index)
                        .ok_or_else(|| UiScriptError::Plan {
                            message: format!(
                                "Lua action {} has an out-of-range resource index {resource_index}",
                                self.next_action
                            ),
                        })?;
                let UiResourceContent::Lua(source) = resource.content() else {
                    return Err(UiScriptError::Plan {
                        message: format!(
                            "Lua action {} refers to a non-Lua resource",
                            self.next_action
                        ),
                    });
                };
                bundle
                    .lua()
                    .load(source.as_str())
                    .set_name(resource.path().as_str())
                    .exec()
                    .map_err(|error| execution_error(resource.path().as_str(), error))?;
                self.executed_chunks += 1;
            }
            UiLoadAction::InlineLua { path, source } => {
                let label = format!("{}:<inline>", path.as_str());
                bundle
                    .lua()
                    .load(source)
                    .set_name(&label)
                    .exec()
                    .map_err(|error| execution_error(&label, error))?;
                self.executed_chunks += 1;
            }
        }
        self.next_action += 1;
        Ok(true)
    }

    /// Executes the remaining expanded actions in order.
    ///
    /// # Errors
    ///
    /// Returns the first error from [`Self::execute_next`].
    pub fn execute_all(
        &mut self,
        bundle: &UiBundle,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
    ) -> Result<(), UiScriptError> {
        while self.execute_next(bundle, tree, scripts)? {}
        Ok(())
    }

    /// Returns the next unexecuted expanded action index.
    #[must_use]
    pub const fn next_action(&self) -> usize {
        self.next_action
    }

    /// Returns the number of object tables exposed so far.
    #[must_use]
    pub const fn registered_object_count(&self) -> usize {
        self.registered_objects
    }

    /// Returns the number of external and inline Lua chunks executed so far.
    #[must_use]
    pub const fn executed_chunk_count(&self) -> usize {
        self.executed_chunks
    }

    /// Returns the number of `OnLoad` callbacks executed so far.
    #[must_use]
    pub const fn executed_load_handler_count(&self) -> usize {
        self.executed_load_handlers
    }

    fn execute_batch(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
        batch: UiObjectBatch,
    ) -> Result<(), UiScriptError> {
        let mut visited = 0;
        self.execute_object(lua, tree, scripts, batch, batch.root(), &mut visited)?;
        if visited != batch.node_count() {
            return Err(UiScriptError::Plan {
                message: format!(
                    "action {} reached {visited} of {} construction nodes",
                    batch.action_index(),
                    batch.node_count()
                ),
            });
        }
        Ok(())
    }

    fn execute_object(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
        batch: UiObjectBatch,
        node_index: usize,
        visited: &mut usize,
    ) -> Result<(), UiScriptError> {
        if !batch.node_range().contains(&node_index) {
            return Err(UiScriptError::Plan {
                message: format!(
                    "object {node_index} escaped action {} construction range",
                    batch.action_index()
                ),
            });
        }
        self.register_object(lua, tree, node_index)?;
        *visited += 1;
        let children = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("construction node {node_index} is outside the object arena"),
            })?
            .construction_children();
        for child in children {
            self.execute_object(lua, tree, scripts, batch, *child, visited)?;
        }
        self.execute_load_handler(lua, tree, scripts, node_index)
    }

    fn register_object(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        node_index: usize,
    ) -> Result<(), UiScriptError> {
        let object = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("object index {node_index} is outside the arena"),
            })?;
        let dimensions =
            self.region_dimensions
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("object {node_index} has no resolved region state"),
                })?;
        let shown =
            self.region_shown
                .get(node_index)
                .copied()
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("object {node_index} has no resolved visibility state"),
                })?;
        let initial_anchors =
            self.region_anchors
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("object {node_index} has no resolved anchor state"),
                })?;
        let anchors = lua
            .create_table()
            .map_err(|error| execution_error("object registration", error))?;
        for anchor in initial_anchors {
            let record = create_anchor_record(
                lua,
                anchor.point,
                anchor.target,
                anchor.relative_point,
                anchor.offset,
            )
            .map_err(|error| execution_error("object registration", error))?;
            anchors
                .raw_set(point_index(anchor.point), record)
                .map_err(|error| execution_error("object registration", error))?;
        }
        let table = lua
            .create_table()
            .map_err(|error| execution_error("object registration", error))?;
        table
            .raw_set(name_key(), object.name())
            .and_then(|()| table.raw_set(type_key(), object_type_name(object.kind())))
            .and_then(|()| {
                table.raw_set(
                    parent_key(),
                    object.parent().map(|parent| parent.saturating_add(1)),
                )
            })
            .and_then(|()| table.raw_set(width_key(), dimensions.0))
            .and_then(|()| table.raw_set(height_key(), dimensions.1))
            .and_then(|()| table.raw_set(index_key(), node_index + 1))
            .and_then(|()| table.raw_set(anchors_key(), anchors))
            .and_then(|()| table.raw_set(shown_key(), shown))
            .map_err(|error| execution_error("object registration", error))?;
        if is_frame_object(object.kind()) {
            table
                .raw_set(
                    events_key(),
                    lua.create_table()
                        .map_err(|error| execution_error("object registration", error))?,
                )
                .and_then(|()| table.raw_set(all_events_key(), false))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if is_enabled_control(object.kind()) {
            table
                .raw_set(enabled_key(), true)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::ScrollFrame {
            table
                .raw_set(horizontal_scroll_key(), 0.0)
                .and_then(|()| table.raw_set(vertical_scroll_key(), 0.0))
                .and_then(|()| table.raw_set(horizontal_scroll_range_key(), 0.0))
                .and_then(|()| table.raw_set(vertical_scroll_range_key(), 0.0))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::Slider {
            table
                .raw_set(slider_min_key(), 0.0)
                .and_then(|()| table.raw_set(slider_max_key(), 0.0))
                .and_then(|()| table.raw_set(slider_value_key(), 0.0))
                .and_then(|()| table.raw_set(slider_step_key(), 0.0))
                .map_err(|error| execution_error("object registration", error))?;
        }
        let metatable_key = self
            .object_metatables
            .get(object_kind_index(object.kind()))
            .ok_or_else(|| UiScriptError::Plan {
                message: format!(
                    "{} has no script metatable",
                    object_type_name(object.kind())
                ),
            })?;
        let metatable: Table = lua
            .registry_value(metatable_key)
            .map_err(|error| execution_error("object registration", error))?;
        table
            .set_metatable(Some(metatable))
            .map_err(|error| execution_error("object registration", error))?;
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error("object registration", error))?;
        objects
            .raw_set(node_index + 1, table.clone())
            .map_err(|error| execution_error("object registration", error))?;

        if let Some(name) = object.name() {
            let globals = lua.globals();
            let existing: Value = globals
                .raw_get(name)
                .map_err(|error| execution_error("object registration", error))?;
            if matches!(existing, Value::Nil) {
                globals
                    .raw_set(name, table)
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }
        self.registered_objects += 1;
        Ok(())
    }

    fn execute_load_handler(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
        node_index: usize,
    ) -> Result<(), UiScriptError> {
        let Some(binding) = scripts.binding(node_index, UiScriptHandler::Load) else {
            return Ok(());
        };
        let object = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("OnLoad object index {node_index} is outside the arena"),
            })?;
        let label = format!(
            "{}<{}>:OnLoad",
            object.name().unwrap_or("<unnamed>"),
            object_type_name(object.kind())
        );
        let function = match binding.target() {
            UiScriptTarget::Compiled(index) => scripts
                .compiled_function(lua, *index)
                .map_err(|error| execution_error(&label, error))?,
            UiScriptTarget::Global(name) => {
                let value: Value = lua
                    .globals()
                    .raw_get(name.as_str())
                    .map_err(|error| execution_error(&label, error))?;
                let Value::Function(function) = value else {
                    return Err(UiScriptError::Execution {
                        label,
                        message: format!("global handler {name} is not a function"),
                    });
                };
                function
            }
        };
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let table: Table = objects
            .raw_get(node_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        function
            .call::<()>(table)
            .map_err(|error| execution_error(&label, error))?;
        self.executed_load_handlers += 1;
        Ok(())
    }
}

fn create_object_metatable(
    lua: &Lua,
    manifest_kind: UiManifestKind,
    kind: UiObjectKind,
) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    methods.raw_set(
        "GetName",
        lua.create_function(|_, object: Table| object.raw_get::<Option<String>>(name_key()))?,
    )?;
    methods.raw_set(
        "GetObjectType",
        lua.create_function(|_, object: Table| object.raw_get::<String>(type_key()))?,
    )?;
    methods.raw_set(
        "IsObjectType",
        lua.create_function(|_, (object, candidate): (Table, String)| {
            let exact = object.raw_get::<String>(type_key())?;
            Ok(is_object_type(&exact, &candidate).then_some(true))
        })?,
    )?;
    methods.raw_set(
        "GetParent",
        lua.create_function(|lua, object: Table| {
            let Some(parent) = object.raw_get::<Option<usize>>(parent_key())? else {
                return Ok(None::<Table>);
            };
            let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
            objects.raw_get::<Option<Table>>(parent)
        })?,
    )?;
    register_region_methods(lua, &methods)?;
    if is_frame_object(kind) {
        register_frame_event_methods(lua, &methods, manifest_kind)?;
        register_frame_backdrop_methods(lua, &methods)?;
        register_frame_visibility_methods(lua, &methods)?;
    }
    if is_enabled_control(kind) {
        register_enabled_methods(lua, &methods, kind)?;
    }
    if kind == UiObjectKind::ScrollFrame {
        register_scroll_frame_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::Slider {
        register_slider_methods(lua, &methods)?;
    }
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
}

fn register_frame_visibility_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "Show",
        lua.create_function(|_, object: Table| object.raw_set(shown_key(), true))?,
    )?;
    methods.raw_set(
        "Hide",
        lua.create_function(|_, object: Table| object.raw_set(shown_key(), false))?,
    )?;
    methods.raw_set(
        "IsShown",
        lua.create_function(|_, object: Table| {
            Ok(object.raw_get::<bool>(shown_key())?.then_some(true))
        })?,
    )?;
    methods.raw_set(
        "IsVisible",
        lua.create_function(|_, object: Table| {
            Ok(object.raw_get::<bool>(shown_key())?.then_some(true))
        })?,
    )?;
    Ok(())
}

fn register_scroll_frame_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetHorizontalScroll",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(horizontal_scroll_key()))?,
    )?;
    methods.raw_set(
        "GetVerticalScroll",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(vertical_scroll_key()))?,
    )?;
    methods.raw_set(
        "GetHorizontalScrollRange",
        lua.create_function(|_, object: Table| {
            object.raw_get::<f64>(horizontal_scroll_range_key())
        })?,
    )?;
    methods.raw_set(
        "GetVerticalScrollRange",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(vertical_scroll_range_key()))?,
    )?;
    methods.raw_set(
        "SetHorizontalScroll",
        lua.create_function(|_, (object, value): (Table, f64)| {
            object.raw_set(horizontal_scroll_key(), value)
        })?,
    )?;
    methods.raw_set(
        "SetVerticalScroll",
        lua.create_function(|_, (object, value): (Table, f64)| {
            object.raw_set(vertical_scroll_key(), value)
        })?,
    )?;
    Ok(())
}

fn register_slider_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetValue",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(slider_value_key()))?,
    )?;
    methods.raw_set(
        "SetValue",
        lua.create_function(|_, (object, value): (Table, f64)| {
            let minimum = object.raw_get::<f64>(slider_min_key())?;
            let maximum = object.raw_get::<f64>(slider_max_key())?;
            object.raw_set(slider_value_key(), value.clamp(minimum, maximum))
        })?,
    )?;
    methods.raw_set(
        "GetMinMaxValues",
        lua.create_function(|_, object: Table| {
            Ok((
                object.raw_get::<f64>(slider_min_key())?,
                object.raw_get::<f64>(slider_max_key())?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetMinMaxValues",
        lua.create_function(|_, (object, minimum, maximum): (Table, f64, f64)| {
            if minimum > maximum {
                return Err(mlua::Error::runtime(
                    "SetMinMaxValues minimum exceeds maximum",
                ));
            }
            object.raw_set(slider_min_key(), minimum)?;
            object.raw_set(slider_max_key(), maximum)?;
            let value = object.raw_get::<f64>(slider_value_key())?;
            object.raw_set(slider_value_key(), value.clamp(minimum, maximum))
        })?,
    )?;
    methods.raw_set(
        "GetValueStep",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(slider_step_key()))?,
    )?;
    methods.raw_set(
        "SetValueStep",
        lua.create_function(|_, (object, step): (Table, f64)| {
            object.raw_set(slider_step_key(), step)
        })?,
    )?;
    Ok(())
}

fn register_enabled_methods(lua: &Lua, methods: &Table, kind: UiObjectKind) -> mlua::Result<()> {
    methods.raw_set(
        "Enable",
        lua.create_function(|_, object: Table| object.raw_set(enabled_key(), true))?,
    )?;
    methods.raw_set(
        "Disable",
        lua.create_function(|_, object: Table| object.raw_set(enabled_key(), false))?,
    )?;
    methods.raw_set(
        "IsEnabled",
        lua.create_function(move |_, object: Table| {
            let enabled = object.raw_get::<bool>(enabled_key())?;
            if kind == UiObjectKind::Slider {
                Ok(if enabled {
                    Value::Number(1.0)
                } else {
                    Value::Nil
                })
            } else {
                Ok(Value::Number(if enabled { 1.0 } else { 0.0 }))
            }
        })?,
    )?;
    Ok(())
}

fn register_region_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetWidth",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(width_key()))?,
    )?;
    methods.raw_set(
        "GetHeight",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(height_key()))?,
    )?;
    methods.raw_set(
        "SetWidth",
        lua.create_function(|_, (object, width): (Table, f64)| object.raw_set(width_key(), width))?,
    )?;
    methods.raw_set(
        "SetHeight",
        lua.create_function(|_, (object, height): (Table, f64)| {
            object.raw_set(height_key(), height)
        })?,
    )?;
    methods.raw_set(
        "SetSize",
        lua.create_function(|_, (object, width, height): (Table, f64, f64)| {
            object.raw_set(width_key(), width)?;
            object.raw_set(height_key(), height)
        })?,
    )?;
    methods.raw_set(
        "SetPoint",
        lua.create_function(
            |lua, (object, point, arguments): (Table, String, Variadic<Value>)| {
                set_region_point(lua, &object, &point, arguments.as_slice())
            },
        )?,
    )?;
    Ok(())
}

fn set_region_point(
    lua: &Lua,
    object: &Table,
    point_name: &str,
    arguments: &[Value],
) -> mlua::Result<()> {
    let point = parse_point(point_name)
        .ok_or_else(|| mlua::Error::runtime("SetPoint(): Unknown region point"))?;
    let mut argument_index = 0;
    let mut target = object.raw_get::<Option<usize>>(parent_key())?;
    if let Some(argument) = arguments.first() {
        match argument {
            Value::String(name) => {
                let name = name.to_str()?;
                let relative: Value = lua.globals().raw_get(name.as_ref())?;
                let Value::Table(relative) = relative else {
                    return Err(mlua::Error::runtime(format!(
                        "SetPoint(): Couldn't find region named '{name}'"
                    )));
                };
                target = Some(relative.raw_get::<usize>(index_key())?);
                argument_index += 1;
            }
            Value::Table(relative) => {
                target = Some(relative.raw_get::<usize>(index_key())?);
                argument_index += 1;
            }
            Value::Nil => {
                target = None;
                argument_index += 1;
            }
            _ => {}
        }
    }
    let self_index = object.raw_get::<usize>(index_key())?;
    if target == Some(self_index) {
        return Err(mlua::Error::runtime(
            "SetPoint(): trying to anchor to itself",
        ));
    }
    let mut relative_point = point;
    if let Some(Value::String(name)) = arguments.get(argument_index) {
        relative_point = parse_point(name.to_str()?.as_ref())
            .ok_or_else(|| mlua::Error::runtime("SetPoint(): Unknown region point"))?;
        argument_index += 1;
    }
    let offset = match (
        arguments.get(argument_index).and_then(lua_number),
        arguments.get(argument_index + 1).and_then(lua_number),
    ) {
        (Some(x), Some(y)) => (x, y),
        _ => (0.0, 0.0),
    };
    let record = create_anchor_record(lua, point, target, relative_point, offset)?;
    let anchors: Table = object.raw_get(anchors_key())?;
    anchors.raw_set(point_index(point), record)
}

fn create_anchor_record(
    lua: &Lua,
    point: UiPoint,
    target: Option<usize>,
    relative_point: UiPoint,
    offset: (f64, f64),
) -> mlua::Result<Table> {
    let record = lua.create_table()?;
    record.raw_set(1, point_name(point))?;
    record.raw_set(2, target)?;
    record.raw_set(3, point_name(relative_point))?;
    record.raw_set(4, offset.0)?;
    record.raw_set(5, offset.1)?;
    Ok(record)
}

fn lua_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Number(value) => Some(*value),
        _ => None,
    }
}

fn parse_point(value: &str) -> Option<UiPoint> {
    if value.eq_ignore_ascii_case("TOPLEFT") {
        Some(UiPoint::TopLeft)
    } else if value.eq_ignore_ascii_case("TOP") {
        Some(UiPoint::Top)
    } else if value.eq_ignore_ascii_case("TOPRIGHT") {
        Some(UiPoint::TopRight)
    } else if value.eq_ignore_ascii_case("LEFT") {
        Some(UiPoint::Left)
    } else if value.eq_ignore_ascii_case("CENTER") {
        Some(UiPoint::Center)
    } else if value.eq_ignore_ascii_case("RIGHT") {
        Some(UiPoint::Right)
    } else if value.eq_ignore_ascii_case("BOTTOMLEFT") {
        Some(UiPoint::BottomLeft)
    } else if value.eq_ignore_ascii_case("BOTTOM") {
        Some(UiPoint::Bottom)
    } else if value.eq_ignore_ascii_case("BOTTOMRIGHT") {
        Some(UiPoint::BottomRight)
    } else {
        None
    }
}

const fn point_name(point: UiPoint) -> &'static str {
    match point {
        UiPoint::TopLeft => "TOPLEFT",
        UiPoint::Top => "TOP",
        UiPoint::TopRight => "TOPRIGHT",
        UiPoint::Left => "LEFT",
        UiPoint::Center => "CENTER",
        UiPoint::Right => "RIGHT",
        UiPoint::BottomLeft => "BOTTOMLEFT",
        UiPoint::Bottom => "BOTTOM",
        UiPoint::BottomRight => "BOTTOMRIGHT",
    }
}

const fn point_index(point: UiPoint) -> usize {
    match point {
        UiPoint::TopLeft => 1,
        UiPoint::Top => 2,
        UiPoint::TopRight => 3,
        UiPoint::Left => 4,
        UiPoint::Center => 5,
        UiPoint::Right => 6,
        UiPoint::BottomLeft => 7,
        UiPoint::Bottom => 8,
        UiPoint::BottomRight => 9,
    }
}

fn register_frame_event_methods(
    lua: &Lua,
    methods: &Table,
    manifest_kind: UiManifestKind,
) -> mlua::Result<()> {
    methods.raw_set(
        "RegisterEvent",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let canonical = registered_event(manifest_kind, &name)?;
            if let Some(canonical) = canonical {
                event_table(&object)?.raw_set(canonical, true)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "UnregisterEvent",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let canonical = registered_event(manifest_kind, &name)?;
            if let Some(canonical) = canonical {
                event_table(&object)?.raw_set(canonical, Value::Nil)?;
            }
            Ok(())
        })?,
    )?;
    methods.raw_set(
        "RegisterAllEvents",
        lua.create_function(|_, object: Table| object.raw_set(all_events_key(), true))?,
    )?;
    methods.raw_set(
        "UnregisterAllEvents",
        lua.create_function(|lua, object: Table| {
            object.raw_set(events_key(), lua.create_table()?)?;
            object.raw_set(all_events_key(), false)
        })?,
    )?;
    methods.raw_set(
        "IsEventRegistered",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let Some(canonical) = registered_event(manifest_kind, &name)? else {
                return Ok(None::<bool>);
            };
            if object.raw_get::<bool>(all_events_key())? {
                return Ok(Some(true));
            }
            event_table(&object)?.raw_get::<Option<bool>>(canonical)
        })?,
    )?;
    Ok(())
}

fn register_frame_backdrop_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetBackdropColor",
        lua.create_function(
            |lua, (object, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                let color = clamped_color(red, green, blue, alpha);
                object.raw_set(backdrop_color_key(), lua.create_sequence_from(color)?)
            },
        )?,
    )?;
    methods.raw_set(
        "SetBackdropBorderColor",
        lua.create_function(
            |lua, (object, red, green, blue, alpha): (Table, f64, f64, f64, Option<f64>)| {
                let color = clamped_color(red, green, blue, alpha);
                object.raw_set(
                    backdrop_border_color_key(),
                    lua.create_sequence_from(color)?,
                )
            },
        )?,
    )?;
    Ok(())
}

fn clamped_color(red: f64, green: f64, blue: f64, alpha: Option<f64>) -> [f64; 4] {
    [
        red.clamp(0.0, 1.0),
        green.clamp(0.0, 1.0),
        blue.clamp(0.0, 1.0),
        alpha.unwrap_or(1.0).clamp(0.0, 1.0),
    ]
}

fn registered_event(
    manifest_kind: UiManifestKind,
    name: &str,
) -> mlua::Result<Option<&'static str>> {
    match manifest_kind {
        UiManifestKind::Glue => Ok(glue_event(name)),
        UiManifestKind::Frame => Err(mlua::Error::runtime(
            "FrameXML event registry is not implemented",
        )),
    }
}

fn event_table(object: &Table) -> mlua::Result<Table> {
    object.raw_get(events_key())
}

fn object_type_name(kind: UiObjectKind) -> &'static str {
    match kind {
        UiObjectKind::Frame => "Frame",
        UiObjectKind::Button => "Button",
        UiObjectKind::CheckButton => "CheckButton",
        UiObjectKind::ColorSelect => "ColorSelect",
        UiObjectKind::Cooldown => "Cooldown",
        UiObjectKind::EditBox => "EditBox",
        UiObjectKind::FontString => "FontString",
        UiObjectKind::GameTooltip => "GameTooltip",
        UiObjectKind::MessageFrame => "MessageFrame",
        UiObjectKind::Minimap => "Minimap",
        UiObjectKind::Model => "Model",
        UiObjectKind::ModelFfx => "ModelFFX",
        UiObjectKind::MovieFrame => "MovieFrame",
        UiObjectKind::ScrollFrame => "ScrollFrame",
        UiObjectKind::ScrollingMessageFrame => "ScrollingMessageFrame",
        UiObjectKind::SimpleHtml => "SimpleHTML",
        UiObjectKind::Slider => "Slider",
        UiObjectKind::StatusBar => "StatusBar",
        UiObjectKind::Texture => "Texture",
        UiObjectKind::WorldFrame => "WorldFrame",
    }
}

fn is_object_type(exact: &str, candidate: &str) -> bool {
    if exact.eq_ignore_ascii_case(candidate) {
        return true;
    }
    if candidate.eq_ignore_ascii_case("Region") {
        return true;
    }
    if candidate.eq_ignore_ascii_case("Frame") {
        return !matches!(exact, "Texture" | "FontString");
    }
    (candidate.eq_ignore_ascii_case("Button") && exact == "CheckButton")
        || (candidate.eq_ignore_ascii_case("Model") && exact == "ModelFFX")
        || (candidate.eq_ignore_ascii_case("MessageFrame") && exact == "ScrollingMessageFrame")
}

fn is_frame_object(kind: UiObjectKind) -> bool {
    !matches!(kind, UiObjectKind::Texture | UiObjectKind::FontString)
}

fn is_enabled_control(kind: UiObjectKind) -> bool {
    matches!(
        kind,
        UiObjectKind::Button | UiObjectKind::CheckButton | UiObjectKind::Slider
    )
}

const fn object_kind_index(kind: UiObjectKind) -> usize {
    match kind {
        UiObjectKind::Frame => 0,
        UiObjectKind::Button => 1,
        UiObjectKind::CheckButton => 2,
        UiObjectKind::ColorSelect => 3,
        UiObjectKind::Cooldown => 4,
        UiObjectKind::EditBox => 5,
        UiObjectKind::FontString => 6,
        UiObjectKind::GameTooltip => 7,
        UiObjectKind::MessageFrame => 8,
        UiObjectKind::Minimap => 9,
        UiObjectKind::Model => 10,
        UiObjectKind::ModelFfx => 11,
        UiObjectKind::MovieFrame => 12,
        UiObjectKind::ScrollFrame => 13,
        UiObjectKind::ScrollingMessageFrame => 14,
        UiObjectKind::SimpleHtml => 15,
        UiObjectKind::Slider => 16,
        UiObjectKind::StatusBar => 17,
        UiObjectKind::Texture => 18,
        UiObjectKind::WorldFrame => 19,
    }
}

fn name_key() -> LightUserData {
    hidden_key(&NAME_TOKEN)
}

fn type_key() -> LightUserData {
    hidden_key(&TYPE_TOKEN)
}

fn parent_key() -> LightUserData {
    hidden_key(&PARENT_TOKEN)
}

fn events_key() -> LightUserData {
    hidden_key(&EVENTS_TOKEN)
}

fn all_events_key() -> LightUserData {
    hidden_key(&ALL_EVENTS_TOKEN)
}

fn width_key() -> LightUserData {
    hidden_key(&WIDTH_TOKEN)
}

fn height_key() -> LightUserData {
    hidden_key(&HEIGHT_TOKEN)
}

fn backdrop_color_key() -> LightUserData {
    hidden_key(&BACKDROP_COLOR_TOKEN)
}

fn backdrop_border_color_key() -> LightUserData {
    hidden_key(&BACKDROP_BORDER_COLOR_TOKEN)
}

fn index_key() -> LightUserData {
    hidden_key(&INDEX_TOKEN)
}

fn anchors_key() -> LightUserData {
    hidden_key(&ANCHORS_TOKEN)
}

fn enabled_key() -> LightUserData {
    hidden_key(&ENABLED_TOKEN)
}

fn horizontal_scroll_key() -> LightUserData {
    hidden_key(&HORIZONTAL_SCROLL_TOKEN)
}

fn vertical_scroll_key() -> LightUserData {
    hidden_key(&VERTICAL_SCROLL_TOKEN)
}

fn horizontal_scroll_range_key() -> LightUserData {
    hidden_key(&HORIZONTAL_SCROLL_RANGE_TOKEN)
}

fn vertical_scroll_range_key() -> LightUserData {
    hidden_key(&VERTICAL_SCROLL_RANGE_TOKEN)
}

fn slider_min_key() -> LightUserData {
    hidden_key(&SLIDER_MIN_TOKEN)
}

fn slider_max_key() -> LightUserData {
    hidden_key(&SLIDER_MAX_TOKEN)
}

fn slider_value_key() -> LightUserData {
    hidden_key(&SLIDER_VALUE_TOKEN)
}

fn slider_step_key() -> LightUserData {
    hidden_key(&SLIDER_STEP_TOKEN)
}

fn shown_key() -> LightUserData {
    hidden_key(&SHOWN_TOKEN)
}

fn hidden_key(token: &'static u8) -> LightUserData {
    LightUserData(std::ptr::from_ref(token).cast_mut().cast::<c_void>())
}

fn execution_error(label: impl Into<String>, error: mlua::Error) -> UiScriptError {
    UiScriptError::Execution {
        label: label.into(),
        message: error.to_string(),
    }
}

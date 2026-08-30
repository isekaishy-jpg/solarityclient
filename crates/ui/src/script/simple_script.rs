//! Ordered Lua source execution and stock object identity methods.

use std::ffi::c_void;

use mlua::{LightUserData, Lua, RegistryKey, Table, Value};

use crate::{
    UiBundle, UiLoadAction, UiObjectBatch, UiObjectKind, UiObjectTree, UiResourceContent,
    UiScriptError, UiScriptHandler, UiScriptPlan, UiScriptTarget,
};

const OBJECT_REGISTRY: &str = "solarity.ui.objects";
// Distinct values prevent identical-data folding from merging these private
// light-userdata keys in optimized builds.
static NAME_TOKEN: u8 = 1;
static TYPE_TOKEN: u8 = 2;
static PARENT_TOKEN: u8 = 3;

/// Incremental executor for one built-in UI bundle.
///
/// Object tables are registered at their exact XML action rather than before
/// execution begins. The initial identity methods are shared by one metatable;
/// concrete widget method tables extend this boundary in later stages.
pub struct UiScriptRuntime {
    next_action: usize,
    object_metatable: RegistryKey,
    registered_objects: usize,
    executed_chunks: usize,
    executed_load_handlers: usize,
}

impl UiScriptRuntime {
    /// Creates an empty object registry and the stock identity method table.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Execution`] when Lua cannot allocate or retain
    /// the registry and metatable objects.
    pub fn new(lua: &Lua) -> Result<Self, UiScriptError> {
        let objects = lua
            .create_table()
            .map_err(|error| execution_error("registry", error))?;
        lua.set_named_registry_value(OBJECT_REGISTRY, objects)
            .map_err(|error| execution_error("registry", error))?;
        let metatable = create_object_metatable(lua)
            .map_err(|error| execution_error("object metatable", error))?;
        let object_metatable = lua
            .create_registry_value(metatable)
            .map_err(|error| execution_error("object metatable", error))?;
        Ok(Self {
            next_action: 0,
            object_metatable,
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
            .map_err(|error| execution_error("object registration", error))?;
        let metatable: Table = lua
            .registry_value(&self.object_metatable)
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
        let label = format!("{}:OnLoad", object.name().unwrap_or("<unnamed>"));
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

fn create_object_metatable(lua: &Lua) -> mlua::Result<Table> {
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
            Ok(is_object_type(&exact, &candidate))
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
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
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

fn name_key() -> LightUserData {
    hidden_key(&NAME_TOKEN)
}

fn type_key() -> LightUserData {
    hidden_key(&TYPE_TOKEN)
}

fn parent_key() -> LightUserData {
    hidden_key(&PARENT_TOKEN)
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

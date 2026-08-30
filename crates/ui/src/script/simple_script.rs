//! Ordered Lua source execution and stock object identity methods.

mod cvars;
mod globals;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;

use mlua::{LightUserData, Lua, RegistryKey, Table, Value, Variadic};
use solarity_asset::{AssetPath, AssetStore};

use crate::{
    FontCatalog, FontDefinition, HorizontalJustification, UiAnchorTarget, UiBundle,
    UiFrameStatePlan, UiLoadAction, UiManifestKind, UiObjectBatch, UiObjectKind, UiObjectTree,
    UiPoint, UiRegionStatePlan, UiResourceContent, UiRuntimeTemplatePlan, UiScriptError,
    UiScriptHandler, UiScriptPlan, UiScriptTarget, UiTexturePlan, VerticalJustification,
};

use self::cvars::UiCVarRegistry;
use self::globals::register_base_globals;
use super::script_events::glue_event;
use super::templates::TEMPLATE_REGISTRY;

const OBJECT_REGISTRY: &str = "solarity.ui.objects";
const METATABLE_REGISTRY: &str = "solarity.ui.object_metatables";
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
static ID_TOKEN: u8 = 22;
static NORMAL_FONT_TOKEN: u8 = 23;
static DISABLED_FONT_TOKEN: u8 = 24;
static HIGHLIGHT_FONT_TOKEN: u8 = 25;
static TEXT_TOKEN: u8 = 26;
static HIGHLIGHT_LOCKED_TOKEN: u8 = 27;
static FONT_SET_TOKEN: u8 = 28;
static FONT_OBJECT_TOKEN: u8 = 29;
static TEX_COORD_TOKEN: u8 = 30;
static JUSTIFY_H_TOKEN: u8 = 31;
static JUSTIFY_V_TOKEN: u8 = 32;
static CHECKED_TOKEN: u8 = 33;
static MODEL_CAMERA_TOKEN: u8 = 34;
static MODEL_SEQUENCE_TOKEN: u8 = 35;
static MODEL_FILE_TOKEN: u8 = 36;
static CLICK_ACTION_TOKEN: u8 = 37;

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

#[derive(Clone, Debug)]
struct InitialFont {
    assigned: bool,
    object_name: Option<String>,
    justify_h: String,
    justify_v: String,
}

impl Default for InitialFont {
    fn default() -> Self {
        Self {
            assigned: false,
            object_name: None,
            justify_h: "CENTER".to_owned(),
            justify_v: "MIDDLE".to_owned(),
        }
    }
}

/// Incremental executor for one built-in UI bundle.
///
/// Object tables are registered at their exact XML action rather than before
/// execution begins. The initial identity methods are shared by one metatable;
/// concrete widget method tables extend this boundary in later stages.
pub struct UiScriptRuntime {
    next_action: usize,
    object_metatables: Vec<RegistryKey>,
    font_metatable: RegistryKey,
    font_actions: Vec<Option<FontDefinition>>,
    region_dimensions: Vec<(f64, f64)>,
    region_shown: Vec<bool>,
    region_anchors: Vec<Vec<InitialAnchor>>,
    font_strings: Vec<InitialFont>,
    texture_coords: Vec<[f64; 8]>,
    frame_ids: Vec<Option<i32>>,
    registered_objects: Rc<Cell<usize>>,
    executed_chunks: usize,
    executed_load_handlers: usize,
}

/// Resolved, mutually aligned plans consumed by ordered Lua construction.
pub struct UiScriptRuntimePlan<'plan, 'bundle> {
    tree: &'plan UiObjectTree<'bundle>,
    frames: &'plan UiFrameStatePlan,
    regions: &'plan UiRegionStatePlan,
    templates: &'plan UiRuntimeTemplatePlan,
    fonts: &'plan FontCatalog,
    textures: &'plan UiTexturePlan,
}

impl<'plan, 'bundle> UiScriptRuntimePlan<'plan, 'bundle> {
    /// Groups the typed plans that must describe the same UI object arena.
    #[must_use]
    pub const fn new(
        tree: &'plan UiObjectTree<'bundle>,
        frames: &'plan UiFrameStatePlan,
        regions: &'plan UiRegionStatePlan,
        templates: &'plan UiRuntimeTemplatePlan,
        fonts: &'plan FontCatalog,
        textures: &'plan UiTexturePlan,
    ) -> Self {
        Self {
            tree,
            frames,
            regions,
            templates,
            fonts,
            textures,
        }
    }
}

/// Immutable process facts required by built-in Lua globals.
#[derive(Clone)]
pub struct UiScriptEnvironment {
    logical_extent: (u32, u32),
    ui_extent: (f64, f64),
    initial_character_count: usize,
    streaming_trial: bool,
    cvars: UiCVarRegistry,
    assets: Option<Rc<RefCell<AssetStore>>>,
}

impl UiScriptEnvironment {
    /// Derives the stock 768-unit UI canvas from a logical window extent.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError::Plan`] when either logical dimension is zero.
    pub fn new(
        logical_width: u32,
        logical_height: u32,
        streaming_trial: bool,
    ) -> Result<Self, UiScriptError> {
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
            initial_character_count: 0,
            streaming_trial,
            cvars: UiCVarRegistry::stock_initial(),
            assets: None,
        })
    }

    /// Returns the SDL logical window extent used to derive UI coordinates.
    #[must_use]
    pub const fn logical_extent(&self) -> (u32, u32) {
        self.logical_extent
    }

    /// Returns the stock aspect-compensated UI width and fixed 768-unit height.
    #[must_use]
    pub const fn ui_extent(&self) -> (f64, f64) {
        self.ui_extent
    }

    /// Returns the character-list count present when GlueXML starts.
    #[must_use]
    pub const fn initial_character_count(&self) -> usize {
        self.initial_character_count
    }

    /// Returns whether assets come from the stock streaming-trial mode.
    #[must_use]
    pub const fn streaming_trial(&self) -> bool {
        self.streaming_trial
    }

    /// Attaches the mounted stock archive stack used by synchronous UI loads.
    #[must_use]
    pub fn with_asset_store(mut self, store: AssetStore) -> Self {
        self.assets = Some(Rc::new(RefCell::new(store)));
        self
    }

    fn cvars(&self) -> UiCVarRegistry {
        self.cvars.clone()
    }

    fn assets(&self) -> Option<Rc<RefCell<AssetStore>>> {
        self.assets.clone()
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
        plan: &UiScriptRuntimePlan<'_, '_>,
        environment: UiScriptEnvironment,
    ) -> Result<Self, UiScriptError> {
        let lua = bundle.lua();
        plan.templates
            .install(lua)
            .map_err(|error| execution_error("runtime templates", error))?;
        register_base_globals(lua, &environment, bundle.manifest().kind())
            .map_err(|error| execution_error("base globals", error))?;
        let objects = lua
            .create_table()
            .map_err(|error| execution_error("registry", error))?;
        lua.set_named_registry_value(OBJECT_REGISTRY, objects)
            .map_err(|error| execution_error("registry", error))?;
        let metatables = lua
            .create_table()
            .map_err(|error| execution_error("object metatables", error))?;
        let object_metatables = OBJECT_KINDS
            .into_iter()
            .map(|kind| {
                let metatable = create_object_metatable(
                    lua,
                    bundle.manifest().kind(),
                    kind,
                    environment.assets(),
                )
                .map_err(|error| execution_error("object metatable", error))?;
                metatables
                    .raw_set(object_type_name(kind), metatable.clone())
                    .map_err(|error| execution_error("object metatable", error))?;
                lua.create_registry_value(metatable)
                    .map_err(|error| execution_error("object metatable", error))
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        lua.set_named_registry_value(METATABLE_REGISTRY, metatables)
            .map_err(|error| execution_error("object metatables", error))?;
        let font_metatable = create_font_metatable(lua)
            .and_then(|metatable| lua.create_registry_value(metatable))
            .map_err(|error| execution_error("font metatable", error))?;
        let mut font_actions = vec![None; bundle.actions().len()];
        for definition in plan.fonts.definitions() {
            let action_index = definition.action_index();
            let Some(slot) = font_actions.get_mut(action_index) else {
                return Err(UiScriptError::Plan {
                    message: format!(
                        "font {} has out-of-range construction action {action_index}",
                        definition.name()
                    ),
                });
            };
            *slot = Some(definition.clone());
        }
        let region_dimensions = (0..plan.regions.state_count())
            .map(|index| {
                let state = plan
                    .regions
                    .state(index)
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })?;
                Ok((f64::from(state.width()), f64::from(state.height())))
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_shown = (0..plan.regions.state_count())
            .map(|index| {
                plan.regions
                    .state(index)
                    .map(|state| state.shown())
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_anchors = (0..plan.regions.state_count())
            .map(|index| {
                let state = plan
                    .regions
                    .state(index)
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })?;
                Ok(plan
                    .regions
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
        let frame_ids = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.id()))
            .collect();
        let font_strings = tree_font_strings(plan.tree, plan.fonts);
        let texture_coords = tree_texture_coords(plan.tree, plan.textures)?;
        let registered_objects = Rc::new(Cell::new(0));
        register_create_frame(lua, plan.regions.state_count(), registered_objects.clone())
            .map_err(|error| execution_error("CreateFrame", error))?;
        Ok(Self {
            next_action: 0,
            object_metatables,
            font_metatable,
            font_actions,
            region_dimensions,
            region_shown,
            region_anchors,
            font_strings,
            texture_coords,
            frame_ids,
            registered_objects,
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
                if let Some(definition) = self
                    .font_actions
                    .get(self.next_action)
                    .and_then(Option::as_ref)
                {
                    register_font(bundle.lua(), &self.font_metatable, definition)?;
                }
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
    pub fn registered_object_count(&self) -> usize {
        self.registered_objects.get()
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
        let frame_id = if is_frame_object(object.kind()) {
            Some(
                self.frame_ids
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("frame object {node_index} has no resolved frame state"),
                    })?,
            )
        } else {
            None
        };
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
                .and_then(|()| table.raw_set(id_key(), frame_id))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if is_enabled_control(object.kind()) {
            table
                .raw_set(enabled_key(), true)
                .map_err(|error| execution_error("object registration", error))?;
        }
        if matches!(
            object.kind(),
            UiObjectKind::Button | UiObjectKind::CheckButton
        ) {
            table
                .raw_set(highlight_locked_key(), false)
                .and_then(|()| table.raw_set(click_action_key(), 0_u64))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::CheckButton {
            table
                .raw_set(checked_key(), false)
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
        if matches!(object.kind(), UiObjectKind::Model | UiObjectKind::ModelFfx) {
            table
                .raw_set(model_camera_key(), 0)
                .and_then(|()| table.raw_set(model_sequence_key(), 0_u32))
                .map_err(|error| execution_error("object registration", error))?;
        }
        if object.kind() == UiObjectKind::FontString {
            let font = self
                .font_strings
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("font string {node_index} has no initial font state"),
                })?;
            table
                .raw_set(font_set_key(), font.assigned)
                .and_then(|()| table.raw_set(justify_h_key(), font.justify_h.as_str()))
                .and_then(|()| table.raw_set(justify_v_key(), font.justify_v.as_str()))
                .map_err(|error| execution_error("object registration", error))?;
            if let Some(name) = &font.object_name {
                let global: Table = lua.globals().raw_get(name.as_str()).map_err(|error| {
                    execution_error(
                        "object registration",
                        mlua::Error::runtime(format!(
                            "font string {} refers to unavailable font {name}: {error}",
                            object.name().unwrap_or("<unnamed>")
                        )),
                    )
                })?;
                table
                    .raw_set(font_object_key(), global)
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }
        if object.kind() == UiObjectKind::Texture {
            let coords =
                self.texture_coords
                    .get(node_index)
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("texture {node_index} has no initial coordinate state"),
                    })?;
            table
                .raw_set(
                    tex_coord_key(),
                    lua.create_sequence_from(*coords)
                        .map_err(|error| execution_error("object registration", error))?,
                )
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
        self.registered_objects
            .set(self.registered_objects.get() + 1);
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

fn register_create_frame(
    lua: &Lua,
    static_object_count: usize,
    registered_objects: Rc<Cell<usize>>,
) -> mlua::Result<()> {
    lua.globals().raw_set(
        "CreateFrame",
        lua.create_function(
            move |lua,
                  (kind, name, parent, template): (
                String,
                Option<String>,
                Option<Table>,
                Option<String>,
            )| {
                let templates: Table = lua.named_registry_value(TEMPLATE_REGISTRY)?;
                let descriptor = if let Some(template) = template {
                    if template.contains(',') {
                        return Err(mlua::Error::runtime(
                            "CreateFrame multiple-template inheritance is not implemented",
                        ));
                    }
                    Some(templates.raw_get::<Table>(template.as_str()).map_err(|_| {
                        mlua::Error::runtime(format!(
                            "CreateFrame template {template} is unavailable"
                        ))
                    })?)
                } else {
                    None
                };
                create_dynamic_frame(
                    lua,
                    &kind,
                    name.as_deref(),
                    parent,
                    descriptor,
                    static_object_count,
                    &registered_objects,
                )
            },
        )?,
    )
}

fn create_dynamic_frame(
    lua: &Lua,
    requested_kind: &str,
    requested_name: Option<&str>,
    requested_parent: Option<Table>,
    descriptor: Option<Table>,
    static_object_count: usize,
    registered_objects: &Cell<usize>,
) -> mlua::Result<Table> {
    let records = if let Some(descriptor) = &descriptor {
        descriptor.raw_get::<Table>("nodes")?
    } else {
        let records = lua.create_table()?;
        let record = lua.create_table()?;
        record.raw_set("kind", requested_kind)?;
        record.raw_set("root_name", true)?;
        record.raw_set("children", lua.create_table()?)?;
        record.raw_set("width", 0.0)?;
        record.raw_set("height", 0.0)?;
        record.raw_set("shown", true)?;
        record.raw_set("font_assigned", false)?;
        record.raw_set("justify_h", "CENTER")?;
        record.raw_set("justify_v", "MIDDLE")?;
        record.raw_set(
            "texture_coords",
            lua.create_sequence_from([0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0])?,
        )?;
        records.raw_set(1, record)?;
        records
    };
    let root_local = descriptor
        .as_ref()
        .map_or(Ok(1), |descriptor| descriptor.raw_get::<usize>("root"))?;
    let root_record: Table = records.raw_get(root_local)?;
    let template_kind = root_record.raw_get::<String>("kind")?;
    if !template_kind.eq_ignore_ascii_case(requested_kind) {
        return Err(mlua::Error::runtime(format!(
            "CreateFrame type {requested_kind} does not match template type {template_kind}"
        )));
    }
    let base_name = requested_name.map(str::to_owned).or_else(|| {
        requested_parent
            .as_ref()
            .and_then(|parent| parent.raw_get(name_key()).ok())
    });
    let count = records.raw_len();
    let mut objects = Vec::with_capacity(count);
    for local_index in 1..=count {
        let record: Table = records.raw_get(local_index)?;
        let kind = record.raw_get::<String>("kind")?;
        let name = dynamic_name(&record, requested_name, base_name.as_deref())?;
        let parent = if local_index == root_local {
            requested_parent.clone()
        } else if let Some(parent_index) = record.raw_get::<Option<usize>>("parent")? {
            objects.get(parent_index - 1).cloned()
        } else if let Some(parent_name) = record.raw_get::<Option<String>>("external_parent")? {
            lua.globals().raw_get::<Option<Table>>(parent_name)?
        } else {
            None
        };
        let index = static_object_count + registered_objects.get() + 1;
        let object =
            create_dynamic_object(lua, &kind, name.as_deref(), parent.as_ref(), index, &record)?;
        registered_objects.set(registered_objects.get() + 1);
        objects.push(object);
    }
    run_dynamic_load(lua, &records, &objects, root_local)?;
    objects
        .get(root_local - 1)
        .cloned()
        .ok_or_else(|| mlua::Error::runtime("CreateFrame root is outside its template"))
}

fn dynamic_name(
    record: &Table,
    requested_name: Option<&str>,
    base_name: Option<&str>,
) -> mlua::Result<Option<String>> {
    if record.raw_get::<Option<bool>>("root_name")?.is_some() {
        Ok(requested_name.map(str::to_owned))
    } else if let Some(suffix) = record.raw_get::<Option<String>>("root_suffix")? {
        Ok(Some(format!("{}{suffix}", base_name.unwrap_or(""))))
    } else {
        record.raw_get("absolute_name")
    }
}

fn create_dynamic_object(
    lua: &Lua,
    kind: &str,
    name: Option<&str>,
    parent: Option<&Table>,
    index: usize,
    record: &Table,
) -> mlua::Result<Table> {
    let object = lua.create_table()?;
    object.raw_set(name_key(), name)?;
    object.raw_set(type_key(), kind)?;
    object.raw_set(
        parent_key(),
        parent
            .map(|parent| parent.raw_get::<usize>(index_key()))
            .transpose()?,
    )?;
    object.raw_set(index_key(), index)?;
    object.raw_set(width_key(), record.raw_get::<f64>("width")?)?;
    object.raw_set(height_key(), record.raw_get::<f64>("height")?)?;
    object.raw_set(shown_key(), record.raw_get::<bool>("shown")?)?;
    object.raw_set(anchors_key(), lua.create_table()?)?;
    if !matches!(kind, "Texture" | "FontString") {
        object.raw_set(events_key(), lua.create_table()?)?;
        object.raw_set(all_events_key(), false)?;
        object.raw_set(id_key(), 0)?;
    }
    if matches!(kind, "Button" | "CheckButton" | "Slider") {
        object.raw_set(enabled_key(), true)?;
    }
    if matches!(kind, "Button" | "CheckButton") {
        object.raw_set(highlight_locked_key(), false)?;
        object.raw_set(click_action_key(), 0_u64)?;
    }
    if kind == "CheckButton" {
        object.raw_set(checked_key(), false)?;
    }
    if kind == "ScrollFrame" {
        object.raw_set(horizontal_scroll_key(), 0.0)?;
        object.raw_set(vertical_scroll_key(), 0.0)?;
        object.raw_set(horizontal_scroll_range_key(), 0.0)?;
        object.raw_set(vertical_scroll_range_key(), 0.0)?;
    }
    if kind == "Slider" {
        object.raw_set(slider_min_key(), 0.0)?;
        object.raw_set(slider_max_key(), 0.0)?;
        object.raw_set(slider_value_key(), 0.0)?;
        object.raw_set(slider_step_key(), 0.0)?;
    }
    if matches!(kind, "Model" | "ModelFFX") {
        object.raw_set(model_camera_key(), 0)?;
        object.raw_set(model_sequence_key(), 0_u32)?;
    }
    if kind == "FontString" {
        object.raw_set(font_set_key(), record.raw_get::<bool>("font_assigned")?)?;
        object.raw_set(justify_h_key(), record.raw_get::<String>("justify_h")?)?;
        object.raw_set(justify_v_key(), record.raw_get::<String>("justify_v")?)?;
        if let Some(name) = record.raw_get::<Option<String>>("font_object_name")? {
            let font: Table = lua.globals().raw_get(name.as_str())?;
            object.raw_set(font_object_key(), font)?;
        }
    }
    if kind == "Texture" {
        object.raw_set(tex_coord_key(), record.raw_get::<Table>("texture_coords")?)?;
    }
    let metatables: Table = lua.named_registry_value(METATABLE_REGISTRY)?;
    object.set_metatable(Some(metatables.raw_get(kind)?))?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    objects.raw_set(index, object.clone())?;
    if let Some(name) = name
        && matches!(lua.globals().raw_get::<Value>(name)?, Value::Nil)
    {
        lua.globals().raw_set(name, object.clone())?;
    }
    Ok(object)
}

fn run_dynamic_load(
    lua: &Lua,
    records: &Table,
    objects: &[Table],
    local: usize,
) -> mlua::Result<()> {
    let record: Table = records.raw_get(local)?;
    let children: Table = record.raw_get("children")?;
    for child in children.sequence_values::<usize>() {
        run_dynamic_load(lua, records, objects, child?)?;
    }
    let function = if let Some(function) = record.raw_get::<Option<mlua::Function>>("on_load")? {
        Some(function)
    } else if let Some(name) = record.raw_get::<Option<String>>("on_load_global")? {
        Some(lua.globals().raw_get::<mlua::Function>(name)?)
    } else {
        None
    };
    if let Some(function) = function {
        function.call::<()>(objects.get(local - 1).cloned().ok_or_else(|| {
            mlua::Error::runtime("dynamic OnLoad object is outside its template")
        })?)?;
    }
    Ok(())
}

fn register_font(
    lua: &Lua,
    metatable_key: &RegistryKey,
    definition: &FontDefinition,
) -> Result<(), UiScriptError> {
    let table = lua
        .create_table()
        .map_err(|error| execution_error("font registration", error))?;
    table
        .raw_set(name_key(), definition.name())
        .and_then(|()| table.raw_set(type_key(), "Font"))
        .map_err(|error| execution_error("font registration", error))?;
    let metatable: Table = lua
        .registry_value(metatable_key)
        .map_err(|error| execution_error("font registration", error))?;
    table
        .set_metatable(Some(metatable))
        .map_err(|error| execution_error("font registration", error))?;

    // FrameScript_Object registers a named object only when that global is
    // currently nil. This first-wins behavior is observable by stock Lua.
    let globals = lua.globals();
    let existing: Value = globals
        .raw_get(definition.name())
        .map_err(|error| execution_error("font registration", error))?;
    if matches!(existing, Value::Nil) {
        globals
            .raw_set(definition.name(), table)
            .map_err(|error| execution_error("font registration", error))?;
    }
    Ok(())
}

fn create_font_metatable(lua: &Lua) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    methods.raw_set(
        "GetName",
        lua.create_function(|_, object: Table| object.raw_get::<String>(name_key()))?,
    )?;
    methods.raw_set(
        "GetObjectType",
        lua.create_function(|_, _: Table| Ok("Font"))?,
    )?;
    methods.raw_set(
        "IsObjectType",
        lua.create_function(|_, (_, candidate): (Table, String)| {
            Ok(candidate.eq_ignore_ascii_case("Font").then_some(true))
        })?,
    )?;
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", methods)?;
    Ok(metatable)
}

fn create_object_metatable(
    lua: &Lua,
    manifest_kind: UiManifestKind,
    kind: UiObjectKind,
    assets: Option<Rc<RefCell<AssetStore>>>,
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
    if matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton) {
        register_button_font_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::CheckButton {
        register_check_button_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::FontString {
        register_font_string_methods(lua, &methods)?;
    }
    if kind == UiObjectKind::Texture {
        register_texture_methods(lua, &methods)?;
    }
    if matches!(kind, UiObjectKind::Model | UiObjectKind::ModelFfx) {
        register_model_methods(lua, &methods, assets)?;
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

fn register_font_string_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetFontObject",
        lua.create_function(|lua, (font_string, value): (Table, Value)| {
            let font = resolve_font_object(lua, value).ok_or_else(|| {
                mlua::Error::runtime("Usage: FontString:SetFontObject(fontObject)")
            })?;
            font_string.raw_set(font_object_key(), font)?;
            font_string.raw_set(font_set_key(), true)
        })?,
    )?;
    methods.raw_set(
        "GetFontObject",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<Option<Table>>(font_object_key())
        })?,
    )?;
    methods.raw_set(
        "SetText",
        lua.create_function(|lua, (font_string, value): (Table, Value)| {
            require_font_string_font(&font_string, "SetText")?;
            font_string.raw_set(text_key(), lua_text(lua, value)?)
        })?,
    )?;
    methods.raw_set(
        "SetFormattedText",
        lua.create_function(|lua, (font_string, arguments): (Table, Variadic<Value>)| {
            require_font_string_font(&font_string, "SetFormattedText")?;
            let library: Table = lua.globals().raw_get("string")?;
            let format: mlua::Function = library.raw_get("format")?;
            font_string.raw_set(text_key(), format.call::<String>(arguments)?)
        })?,
    )?;
    methods.raw_set(
        "GetText",
        lua.create_function(|_, font_string: Table| {
            let text = font_string.raw_get::<Option<String>>(text_key())?;
            Ok(text.filter(|text| !text.is_empty()))
        })?,
    )?;
    register_font_string_justification_methods(lua, methods)
}

fn register_font_string_justification_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetJustifyH",
        lua.create_function(|_, (font_string, value): (Table, String)| {
            let value = stock_justify(&value).ok_or_else(|| {
                mlua::Error::runtime("Usage: FontString:SetJustifyH(\"justify\")")
            })?;
            font_string.raw_set(justify_h_key(), value)
        })?,
    )?;
    methods.raw_set(
        "GetJustifyH",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<String>(justify_h_key())
        })?,
    )?;
    methods.raw_set(
        "SetJustifyV",
        lua.create_function(|_, (font_string, value): (Table, String)| {
            let value = stock_justify(&value).ok_or_else(|| {
                mlua::Error::runtime("Usage: FontString:SetJustifyV(\"justify\")")
            })?;
            font_string.raw_set(justify_v_key(), value)
        })?,
    )?;
    methods.raw_set(
        "GetJustifyV",
        lua.create_function(|_, font_string: Table| {
            font_string.raw_get::<String>(justify_v_key())
        })?,
    )
}

fn stock_justify(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("LEFT") {
        Some("LEFT")
    } else if value.eq_ignore_ascii_case("CENTER") {
        Some("CENTER")
    } else if value.eq_ignore_ascii_case("RIGHT") {
        Some("RIGHT")
    } else if value.eq_ignore_ascii_case("TOP") {
        Some("TOP")
    } else if value.eq_ignore_ascii_case("MIDDLE") {
        Some("MIDDLE")
    } else if value.eq_ignore_ascii_case("BOTTOM") {
        Some("BOTTOM")
    } else {
        None
    }
}

fn require_font_string_font(font_string: &Table, method: &str) -> mlua::Result<()> {
    if font_string.raw_get::<bool>(font_set_key())? {
        return Ok(());
    }
    let name = font_string
        .raw_get::<Option<String>>(name_key())?
        .unwrap_or_else(|| "<unnamed>".to_owned());
    Err(mlua::Error::runtime(format!(
        "{name}:{method}(): Font not set"
    )))
}

fn register_texture_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetTexCoord",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            let values = arguments
                .iter()
                .map(|value| lua_number(value).unwrap_or(0.0))
                .collect::<Vec<_>>();
            let coords = match values.as_slice() {
                [left, right, top, bottom] => [
                    *left, *top, *left, *bottom, *right, *top, *right, *bottom,
                ],
                [ul_x, ul_y, ll_x, ll_y, ur_x, ur_y, lr_x, lr_y] => {
                    [*ul_x, *ul_y, *ll_x, *ll_y, *ur_x, *ur_y, *lr_x, *lr_y]
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "Usage: Texture:SetTexCoord(minX, maxX, minY, maxY) or SetTexCoord(ULx, ULy, LLx, LLy, URx, URy, LRx, LRy)",
                    ));
                }
            };
            if coords
                .iter()
                .any(|value| *value < -10_000.0 || *value > 10_000.0)
            {
                return Err(mlua::Error::runtime("TexCoord out of range"));
            }
            texture.raw_set(tex_coord_key(), lua.create_sequence_from(coords)?)
        })?,
    )?;
    methods.raw_set(
        "GetTexCoord",
        lua.create_function(|_, texture: Table| {
            let coords: Table = texture.raw_get(tex_coord_key())?;
            coords
                .sequence_values::<f64>()
                .map(|value| value.map(Value::Number))
                .collect::<mlua::Result<Variadic<Value>>>()
        })?,
    )
}

fn register_model_methods(
    lua: &Lua,
    methods: &Table,
    assets: Option<Rc<RefCell<AssetStore>>>,
) -> mlua::Result<()> {
    methods.raw_set(
        "SetModel",
        lua.create_function(move |lua, (model, value): (Table, Value)| {
            let Some(value) = lua.coerce_string(value)? else {
                return Err(mlua::Error::runtime("Usage: Model:SetModel(\"file\")"));
            };
            let display = value.to_string_lossy();
            let path = AssetPath::new(&display)
                .map_err(|error| mlua::Error::runtime(format!("Invalid model file: {error}")))?;
            let Some(assets) = &assets else {
                return Err(mlua::Error::runtime(
                    "Model:SetModel requires a mounted asset store",
                ));
            };
            assets
                .borrow_mut()
                .read(&path)
                .map_err(|_| mlua::Error::runtime(format!("Invalid model file: {display}")))?;
            model.raw_set(model_file_key(), path.as_str())
        })?,
    )?;
    methods.raw_set(
        "GetModel",
        lua.create_function(|_, model: Table| model.raw_get::<Option<String>>(model_file_key()))?,
    )?;
    methods.raw_set(
        "SetCamera",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Model:SetCamera(index)"))?;
            model.raw_set(model_camera_key(), value as i32)
        })?,
    )?;
    methods.raw_set(
        "SetSequence",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Model:SetSequence(sequence)"))?;
            if !(0.0..506.0).contains(&value) {
                return Err(mlua::Error::runtime(
                    "SetSequence(sequence) exceeds valid range of 0 - 506",
                ));
            }
            model.raw_set(model_sequence_key(), value as u32)
        })?,
    )
}

fn register_button_font_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    register_button_font_pair(
        lua,
        methods,
        "SetNormalFontObject",
        "GetNormalFontObject",
        normal_font_key(),
    )?;
    register_button_font_pair(
        lua,
        methods,
        "SetDisabledFontObject",
        "GetDisabledFontObject",
        disabled_font_key(),
    )?;
    register_button_font_pair(
        lua,
        methods,
        "SetHighlightFontObject",
        "GetHighlightFontObject",
        highlight_font_key(),
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
    )
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

fn register_check_button_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
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

fn lua_bool(value: &Value, default: bool) -> bool {
    match value {
        Value::Nil => false,
        Value::Boolean(value) => *value,
        Value::Integer(value) => *value != 0,
        Value::Number(value) => *value != 0.0,
        Value::String(value) => {
            let value = value.to_string_lossy();
            let Some(first) = value.as_bytes().first().copied() else {
                return default;
            };
            match first {
                b'0' | b'F' | b'N' | b'f' | b'n' => false,
                b'1'..=b'9' | b'T' | b'Y' | b't' | b'y' => true,
                _ if value.eq_ignore_ascii_case("off")
                    || value.eq_ignore_ascii_case("disabled") =>
                {
                    false
                }
                _ if value.eq_ignore_ascii_case("on") || value.eq_ignore_ascii_case("enabled") => {
                    true
                }
                _ => default,
            }
        }
        _ => default,
    }
}

fn register_button_font_pair(
    lua: &Lua,
    methods: &Table,
    setter: &'static str,
    getter: &'static str,
    key: LightUserData,
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
            button.raw_set(key, font)
        })?,
    )?;
    methods.raw_set(
        getter,
        lua.create_function(move |_, button: Table| button.raw_get::<Option<Table>>(key))?,
    )
}

fn resolve_font_object(lua: &Lua, value: Value) -> Option<Table> {
    let table = match value {
        Value::String(name) => lua.globals().raw_get::<Table>(name.to_str().ok()?).ok()?,
        Value::Table(table) => table,
        _ => return None,
    };
    let kind = table.raw_get::<String>(type_key()).ok()?;
    kind.eq_ignore_ascii_case("Font").then_some(table)
}

fn lua_text(lua: &Lua, value: Value) -> mlua::Result<Option<String>> {
    lua.coerce_string(value)
        .map(|value| value.map(|value| value.to_string_lossy()))
}

fn register_frame_visibility_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetID",
        lua.create_function(|_, object: Table| object.raw_get::<i32>(id_key()))?,
    )?;
    methods.raw_set(
        "SetID",
        lua.create_function(|_, (object, id): (Table, i32)| object.raw_set(id_key(), id))?,
    )
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
        "GetSize",
        lua.create_function(|_, object: Table| {
            Ok((
                object.raw_get::<f64>(width_key())?,
                object.raw_get::<f64>(height_key())?,
            ))
        })?,
    )?;
    methods.raw_set(
        "GetNumPoints",
        lua.create_function(|_, object: Table| {
            let anchors: Table = object.raw_get(anchors_key())?;
            let mut count = 0;
            for entry in anchors.pairs::<Value, Value>() {
                let _ = entry?;
                count += 1;
            }
            Ok(count)
        })?,
    )?;
    methods.raw_set(
        "GetPoint",
        lua.create_function(|lua, (object, index): (Table, Option<usize>)| {
            get_region_point(lua, &object, index.unwrap_or(1))
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
    methods.raw_set(
        "SetAllPoints",
        lua.create_function(|lua, (object, relative): (Table, Option<Value>)| {
            set_all_region_points(lua, &object, relative)
        })?,
    )?;
    methods.raw_set(
        "ClearAllPoints",
        lua.create_function(|lua, object: Table| {
            object.raw_set(anchors_key(), lua.create_table()?)
        })?,
    )?;
    register_region_visibility_methods(lua, methods)?;
    Ok(())
}

fn register_region_visibility_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
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
            Ok(object
                .raw_get::<bool>(shown_key())?
                .then_some(Value::Number(1.0)))
        })?,
    )?;
    methods.raw_set(
        "IsVisible",
        lua.create_function(|lua, object: Table| {
            Ok(object_is_visible(lua, object)?.then_some(Value::Number(1.0)))
        })?,
    )
}

fn object_is_visible(lua: &Lua, mut object: Table) -> mlua::Result<bool> {
    loop {
        if !object.raw_get::<bool>(shown_key())? {
            return Ok(false);
        }
        let Some(parent_index) = object.raw_get::<Option<usize>>(parent_key())? else {
            return Ok(true);
        };
        let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
        let Some(parent) = objects.raw_get::<Option<Table>>(parent_index)? else {
            return Ok(false);
        };
        object = parent;
    }
}

fn get_region_point(
    lua: &Lua,
    object: &Table,
    index: usize,
) -> mlua::Result<(String, Option<Table>, String, f64, f64)> {
    let anchors: Table = object.raw_get(anchors_key())?;
    let mut records = Vec::new();
    for point in 1..=9 {
        if let Some(record) = anchors.raw_get::<Option<Table>>(point)? {
            records.push(record);
        }
    }
    let record = records
        .get(index.saturating_sub(1))
        .ok_or_else(|| mlua::Error::runtime("GetPoint(): anchor index is out of range"))?;
    let relative = if let Some(target) = record.raw_get::<Option<usize>>(2)? {
        let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
        objects.raw_get::<Option<Table>>(target)?
    } else {
        None
    };
    Ok((
        record.raw_get(1)?,
        relative,
        record.raw_get(3)?,
        record.raw_get(4)?,
        record.raw_get(5)?,
    ))
}

fn set_all_region_points(lua: &Lua, object: &Table, relative: Option<Value>) -> mlua::Result<()> {
    let target = match relative {
        None => object.raw_get::<Option<usize>>(parent_key())?,
        Some(Value::Nil) => None,
        Some(Value::String(name)) => {
            let name = name.to_str()?;
            let table = lua.globals().raw_get::<Table>(name.as_ref()).map_err(|_| {
                mlua::Error::runtime(format!(
                    "SetAllPoints(): Couldn't find region named '{name}'"
                ))
            })?;
            Some(table.raw_get::<usize>(index_key())?)
        }
        Some(Value::Table(table)) => Some(table.raw_get::<usize>(index_key())?),
        Some(_) => object.raw_get::<Option<usize>>(parent_key())?,
    };
    if target == Some(object.raw_get::<usize>(index_key())?) {
        return Err(mlua::Error::runtime(
            "SetAllPoints(): trying to anchor to itself",
        ));
    }
    let anchors = lua.create_table()?;
    let top_left =
        create_anchor_record(lua, UiPoint::TopLeft, target, UiPoint::TopLeft, (0.0, 0.0))?;
    let bottom_right = create_anchor_record(
        lua,
        UiPoint::BottomRight,
        target,
        UiPoint::BottomRight,
        (0.0, 0.0),
    )?;
    anchors.raw_set(point_index(UiPoint::TopLeft), top_left)?;
    anchors.raw_set(point_index(UiPoint::BottomRight), bottom_right)?;
    object.raw_set(anchors_key(), anchors)
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

fn tree_font_strings(tree: &UiObjectTree<'_>, fonts: &FontCatalog) -> Vec<InitialFont> {
    tree.nodes()
        .iter()
        .map(|node| {
            if node.kind() != UiObjectKind::FontString {
                return InitialFont::default();
            }
            let mut initial = InitialFont::default();
            for layer in node.layers() {
                if let Some(inherits) = xml_attribute(layer.element(), "inherits") {
                    for name in inherits.split(',').map(str::trim) {
                        if let Some(definition) = fonts.definition(name) {
                            initial.assigned = true;
                            initial.object_name = Some(name.to_owned());
                            apply_font_justification(&mut initial, definition);
                        }
                    }
                }
                if let Some(font) = xml_attribute(layer.element(), "font") {
                    initial.assigned = !font.is_empty();
                    let definition = fonts.definition(font);
                    initial.object_name = definition.map(|definition| definition.name().to_owned());
                    if let Some(definition) = definition {
                        apply_font_justification(&mut initial, definition);
                    }
                }
                if let Some(value) = xml_attribute(layer.element(), "justifyH")
                    && stock_justify(value).is_some()
                {
                    initial.justify_h = value.to_ascii_uppercase();
                }
                if let Some(value) = xml_attribute(layer.element(), "justifyV")
                    && stock_justify(value).is_some()
                {
                    initial.justify_v = value.to_ascii_uppercase();
                }
            }
            initial
        })
        .collect()
}

fn apply_font_justification(initial: &mut InitialFont, definition: &FontDefinition) {
    if let Some(value) = definition.horizontal_justification() {
        initial.justify_h = match value {
            HorizontalJustification::Left => "LEFT",
            HorizontalJustification::Center => "CENTER",
            HorizontalJustification::Right => "RIGHT",
        }
        .to_owned();
    }
    if let Some(value) = definition.vertical_justification() {
        initial.justify_v = match value {
            VerticalJustification::Top => "TOP",
            VerticalJustification::Middle => "MIDDLE",
            VerticalJustification::Bottom => "BOTTOM",
        }
        .to_owned();
    }
}

fn tree_texture_coords(
    tree: &UiObjectTree<'_>,
    textures: &UiTexturePlan,
) -> Result<Vec<[f64; 8]>, UiScriptError> {
    tree.nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let mut coords = [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
            if node.kind() != UiObjectKind::Texture {
                return Ok(coords);
            }
            let texture = textures.node(index).ok_or_else(|| UiScriptError::Plan {
                message: format!("texture {index} is outside the texture plan"),
            })?;
            for layer in textures.layers_for(texture) {
                let Some(value) = layer.tex_coords() else {
                    continue;
                };
                let left = f64::from(value.left().unwrap_or(coords[0] as f32));
                let right = f64::from(value.right().unwrap_or(coords[4] as f32));
                let top = f64::from(value.top().unwrap_or(coords[1] as f32));
                let bottom = f64::from(value.bottom().unwrap_or(coords[3] as f32));
                coords = [left, top, left, bottom, right, top, right, bottom];
            }
            Ok(coords)
        })
        .collect()
}

fn xml_attribute<'a>(element: &'a crate::XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
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

fn id_key() -> LightUserData {
    hidden_key(&ID_TOKEN)
}

fn normal_font_key() -> LightUserData {
    hidden_key(&NORMAL_FONT_TOKEN)
}

fn disabled_font_key() -> LightUserData {
    hidden_key(&DISABLED_FONT_TOKEN)
}

fn highlight_font_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_FONT_TOKEN)
}

fn text_key() -> LightUserData {
    hidden_key(&TEXT_TOKEN)
}

fn highlight_locked_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_LOCKED_TOKEN)
}

fn font_set_key() -> LightUserData {
    hidden_key(&FONT_SET_TOKEN)
}

fn font_object_key() -> LightUserData {
    hidden_key(&FONT_OBJECT_TOKEN)
}

fn tex_coord_key() -> LightUserData {
    hidden_key(&TEX_COORD_TOKEN)
}

fn justify_h_key() -> LightUserData {
    hidden_key(&JUSTIFY_H_TOKEN)
}

fn justify_v_key() -> LightUserData {
    hidden_key(&JUSTIFY_V_TOKEN)
}

fn checked_key() -> LightUserData {
    hidden_key(&CHECKED_TOKEN)
}

fn model_camera_key() -> LightUserData {
    hidden_key(&MODEL_CAMERA_TOKEN)
}

fn model_sequence_key() -> LightUserData {
    hidden_key(&MODEL_SEQUENCE_TOKEN)
}

fn model_file_key() -> LightUserData {
    hidden_key(&MODEL_FILE_TOKEN)
}

fn click_action_key() -> LightUserData {
    hidden_key(&CLICK_ACTION_TOKEN)
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

//! Ordered Lua source execution and stock object identity methods.

mod buttons;
mod cvars;
mod globals;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;

use mlua::{LightUserData, Lua, MultiValue, RegistryKey, Table, Value, Variadic};
use solarity_asset::{AssetPath, AssetStore, AssetStoreHandle};

use crate::event::{UiEventArgument, UiEventPayload, canonical_glue_event};
use crate::script::UiGlueNetworkBridge;
use crate::{
    FontCatalog, FontDefinition, HorizontalJustification, UiAnchorTarget, UiBlendMode, UiBundle,
    UiDrawLayer, UiFrameStatePlan, UiFrameStrata, UiLoadAction, UiManifestKind, UiObjectBatch,
    UiObjectKind, UiObjectRole, UiObjectTree, UiPoint, UiRegionStatePlan, UiResourceContent,
    UiRuntimeTemplatePlan, UiScriptError, UiScriptHandler, UiScriptPlan, UiScriptTarget,
    UiTextureFile, UiTextureStatePlan, VerticalJustification, XmlContent,
};

use self::cvars::UiCVarRegistry;
use self::globals::register_base_globals;
use super::handlers::handler_for;
use super::templates::TEMPLATE_REGISTRY;

pub(super) const OBJECT_REGISTRY: &str = "solarity.ui.objects";
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
static MODEL_SEQUENCE_TIME_SEQUENCE_TOKEN: u8 = 38;
static MODEL_SEQUENCE_TIME_TOKEN: u8 = 39;
static FRAME_LEVEL_TOKEN: u8 = 40;
static MODEL_SCALE_TOKEN: u8 = 41;
static KEYBOARD_ENABLED_TOKEN: u8 = 42;
static TEXTURE_COLOR_TOKEN: u8 = 43;
static NORMAL_TEXTURE_TOKEN: u8 = 44;
static PUSHED_TEXTURE_TOKEN: u8 = 45;
static DISABLED_TEXTURE_TOKEN: u8 = 46;
static HIGHLIGHT_TEXTURE_TOKEN: u8 = 47;
static SCRIPT_HANDLERS_TOKEN: u8 = 48;
static BUTTON_TEXT_TOKEN: u8 = 49;
static ROLE_TOKEN: u8 = 50;
static ALPHA_TOKEN: u8 = 51;
static SCALE_TOKEN: u8 = 52;
static TEXTURE_FILE_TOKEN: u8 = 53;
static TEXTURE_SOLID_COLOR_TOKEN: u8 = 54;
static TEXTURE_BLEND_MODE_TOKEN: u8 = 55;
static HORIZONTAL_TILING_TOKEN: u8 = 56;
static VERTICAL_TILING_TOKEN: u8 = 57;
static NON_BLOCKING_TOKEN: u8 = 58;
static DRAW_LAYER_TOKEN: u8 = 59;
static DRAW_SUB_LEVEL_TOKEN: u8 = 60;
static FRAME_STRATA_TOKEN: u8 = 61;

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

#[derive(Clone, Debug, Default)]
struct InitialButton {
    text_reference: Option<String>,
    normal_font: Option<String>,
    disabled_font: Option<String>,
    highlight_font: Option<String>,
}

#[derive(Clone, Debug)]
struct InitialTexture {
    file: Option<String>,
    coords: [f64; 8],
    colors: [[f64; 4]; 4],
    blend_mode: &'static str,
    horizontal_tiling: bool,
    vertical_tiling: bool,
    non_blocking: bool,
    draw_layer: &'static str,
    draw_sub_level: i16,
}

impl Default for InitialTexture {
    fn default() -> Self {
        Self {
            file: None,
            coords: [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0],
            colors: [[1.0, 1.0, 1.0, 1.0]; 4],
            blend_mode: "BLEND",
            horizontal_tiling: false,
            vertical_tiling: false,
            non_blocking: false,
            draw_layer: "ARTWORK",
            draw_sub_level: 0,
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
    region_alpha: Vec<f64>,
    region_scale: Vec<f64>,
    region_anchors: Vec<Vec<InitialAnchor>>,
    font_strings: Vec<InitialFont>,
    buttons: Vec<InitialButton>,
    textures: Vec<InitialTexture>,
    frame_ids: Vec<Option<i32>>,
    frame_levels: Vec<Option<i32>>,
    frame_strata: Vec<Option<&'static str>>,
    frame_keyboard_enabled: Vec<Option<bool>>,
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
    texture_states: &'plan UiTextureStatePlan,
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
        texture_states: &'plan UiTextureStatePlan,
    ) -> Self {
        Self {
            tree,
            frames,
            regions,
            templates,
            fonts,
            texture_states,
        }
    }
}

/// Retained stock audio requests awaiting the media backend.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiGlueMediaIntent {
    pub(crate) music: Option<String>,
    pub(crate) ambience: Option<String>,
}

impl UiGlueMediaIntent {
    /// Returns the most recently requested Glue music resource.
    #[must_use]
    pub fn music(&self) -> Option<&str> {
        self.music.as_deref()
    }

    /// Returns the most recently requested Glue ambience resource.
    #[must_use]
    pub fn ambience(&self) -> Option<&str> {
        self.ambience.as_deref()
    }
}

/// Immutable process facts required by built-in Lua globals.
#[derive(Clone)]
pub struct UiScriptEnvironment {
    logical_extent: (u32, u32),
    ui_extent: (f64, f64),
    streaming_trial: bool,
    cvars: UiCVarRegistry,
    assets: Option<AssetStoreHandle>,
    media_intent: Rc<RefCell<UiGlueMediaIntent>>,
    network: Rc<RefCell<UiGlueNetworkBridge>>,
    current_screen: Rc<RefCell<String>>,
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
            streaming_trial,
            cvars: UiCVarRegistry::stock_initial(),
            assets: None,
            media_intent: Rc::new(RefCell::new(UiGlueMediaIntent::default())),
            network: Rc::new(RefCell::new(UiGlueNetworkBridge::default())),
            current_screen: Rc::new(RefCell::new(String::new())),
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

    /// Returns whether assets come from the stock streaming-trial mode.
    #[must_use]
    pub const fn streaming_trial(&self) -> bool {
        self.streaming_trial
    }

    /// Returns one registered console variable's current script-visible text.
    #[must_use]
    pub fn cvar_value(&self, name: &str) -> Option<String> {
        self.cvars.get(name)
    }

    /// Attaches the mounted stock archive stack used by synchronous UI loads.
    #[must_use]
    pub fn with_asset_store(mut self, store: AssetStore) -> Self {
        self.assets = Some(AssetStoreHandle::new(store));
        self
    }

    /// Attaches a main-thread archive stack already owned by a UI manager.
    #[must_use]
    pub(crate) fn with_shared_asset_store(mut self, store: AssetStoreHandle) -> Self {
        self.assets = Some(store);
        self
    }

    fn cvars(&self) -> UiCVarRegistry {
        self.cvars.clone()
    }

    fn assets(&self) -> Option<AssetStoreHandle> {
        self.assets.clone()
    }

    pub(crate) fn media_intent(&self) -> Rc<RefCell<UiGlueMediaIntent>> {
        self.media_intent.clone()
    }

    pub(crate) fn network(&self) -> Rc<RefCell<UiGlueNetworkBridge>> {
        self.network.clone()
    }

    pub(crate) fn current_screen(&self) -> Rc<RefCell<String>> {
        self.current_screen.clone()
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
        let font_definitions = Rc::new(
            plan.fonts
                .definitions()
                .iter()
                .cloned()
                .map(|definition| (definition.name().to_owned(), definition))
                .collect::<HashMap<_, _>>(),
        );
        let button_measurement = environment
            .assets()
            .map(|assets| {
                buttons::ButtonTextMeasurement::new(
                    assets,
                    font_definitions.clone(),
                    environment.logical_extent().1,
                )
            })
            .transpose()
            .map_err(|error| {
                execution_error(
                    "button text measurement",
                    mlua::Error::runtime(error.to_string()),
                )
            })?;
        let object_metatables = OBJECT_KINDS
            .into_iter()
            .map(|kind| {
                let metatable = create_object_metatable(
                    lua,
                    bundle.manifest().kind(),
                    kind,
                    environment.assets(),
                    button_measurement.clone(),
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
        let region_alpha = (0..plan.regions.state_count())
            .map(|index| {
                plan.regions
                    .state(index)
                    .map(|state| f64::from(state.alpha()))
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("region state {index} is outside the arena"),
                    })
            })
            .collect::<Result<Vec<_>, UiScriptError>>()?;
        let region_scale = (0..plan.regions.state_count())
            .map(|index| {
                plan.regions
                    .state(index)
                    .map(|state| f64::from(state.scale()))
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
        let frame_levels = (0..plan.regions.state_count())
            .map(|index| plan.frames.state(index).map(|state| state.level()))
            .collect();
        let frame_strata = (0..plan.regions.state_count())
            .map(|index| {
                plan.frames
                    .state(index)
                    .map(|state| frame_strata_name(state.strata()))
            })
            .collect();
        let frame_keyboard_enabled = (0..plan.regions.state_count())
            .map(|index| {
                plan.frames
                    .state(index)
                    .map(|state| state.keyboard_enabled())
            })
            .collect();
        let font_strings = tree_font_strings(plan.tree, plan.fonts);
        let buttons = tree_buttons(plan.tree);
        let textures = tree_textures(plan.tree, plan.texture_states)?;
        let registered_objects = Rc::new(Cell::new(0));
        let dynamic_objects = Rc::new(Cell::new(0));
        register_create_frame(
            lua,
            plan.regions.state_count(),
            registered_objects.clone(),
            dynamic_objects,
        )
        .map_err(|error| execution_error("CreateFrame", error))?;
        Ok(Self {
            next_action: 0,
            object_metatables,
            font_metatable,
            font_actions,
            region_dimensions,
            region_shown,
            region_alpha,
            region_scale,
            region_anchors,
            font_strings,
            buttons,
            textures,
            frame_ids,
            frame_levels,
            frame_strata,
            frame_keyboard_enabled,
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

    /// Copies the authoritative post-script region state out of Lua.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] when a live object table is incomplete or
    /// contains an invalid arena reference or non-finite layout value.
    pub(crate) fn snapshot_objects(
        &self,
        bundle: &UiBundle,
    ) -> Result<super::runtime_state::UiRuntimeObjectPlan, UiScriptError> {
        super::runtime_state::snapshot_runtime_objects(bundle.lua(), self.registered_object_count())
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

    /// Delivers one canonical Glue event to subscribers in creation order.
    ///
    /// # Errors
    ///
    /// Returns [`UiScriptError`] when Lua registry access or a subscribed
    /// `OnEvent` callback fails. Dispatch stops at the first failed handler.
    pub(crate) fn dispatch_glue_event(
        &mut self,
        bundle: &UiBundle,
        event: &'static str,
        payload: &UiEventPayload,
    ) -> Result<usize, UiScriptError> {
        let lua = bundle.lua();
        let globals = lua.globals();
        let previous_event = globals
            .raw_get::<Value>("event")
            .map_err(|error| execution_error(event, error))?;
        let mut previous_arguments = Vec::with_capacity(crate::event::MAX_EVENT_ARGUMENTS);
        for index in 1..=crate::event::MAX_EVENT_ARGUMENTS {
            previous_arguments.push(
                globals
                    .raw_get::<Value>(format!("arg{index}"))
                    .map_err(|error| execution_error(event, error))?,
            );
        }
        let arguments = payload
            .arguments()
            .iter()
            .map(|argument| event_argument(lua, argument))
            .collect::<mlua::Result<Vec<_>>>()
            .map_err(|error| execution_error(event, error))?;
        globals
            .raw_set("event", event)
            .map_err(|error| execution_error(event, error))?;
        for index in 1..=crate::event::MAX_EVENT_ARGUMENTS {
            globals
                .raw_set(
                    format!("arg{index}"),
                    arguments.get(index - 1).cloned().unwrap_or(Value::Nil),
                )
                .map_err(|error| execution_error(event, error))?;
        }

        let dispatch = dispatch_subscribers(lua, self.registered_object_count(), event, &arguments);
        let restore = restore_event_globals(lua, previous_event, previous_arguments);
        match (dispatch, restore) {
            (Ok(count), Ok(())) => Ok(count),
            (Err(error), _) => Err(execution_error(event, error)),
            (Ok(_), Err(error)) => Err(execution_error(event, error)),
        }
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
        self.register_object(lua, tree, scripts, node_index)?;
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
        self.execute_load_handler(lua, tree, node_index)
    }

    fn register_object(
        &mut self,
        lua: &Lua,
        tree: &UiObjectTree<'_>,
        scripts: &UiScriptPlan,
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
        let frame_level = if is_frame_object(object.kind()) {
            Some(
                self.frame_levels
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("frame object {node_index} has no resolved frame level"),
                    })?,
            )
        } else {
            None
        };
        let keyboard_enabled = if is_frame_object(object.kind()) {
            Some(
                self.frame_keyboard_enabled
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!(
                            "frame object {node_index} has no resolved keyboard-input state"
                        ),
                    })?,
            )
        } else {
            None
        };
        let frame_strata = if is_frame_object(object.kind()) {
            Some(
                self.frame_strata
                    .get(node_index)
                    .copied()
                    .flatten()
                    .ok_or_else(|| UiScriptError::Plan {
                        message: format!("frame object {node_index} has no resolved frame strata"),
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
            .and_then(|()| table.raw_set(role_key(), object_role_name(object.role())))
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
            .and_then(|()| {
                table.raw_set(
                    alpha_key(),
                    self.region_alpha.get(node_index).copied().ok_or_else(|| {
                        mlua::Error::runtime("object has no resolved alpha state")
                    })?,
                )
            })
            .and_then(|()| {
                table.raw_set(
                    scale_key(),
                    self.region_scale.get(node_index).copied().ok_or_else(|| {
                        mlua::Error::runtime("object has no resolved scale state")
                    })?,
                )
            })
            .and_then(|()| table.raw_set(draw_layer_key(), "ARTWORK"))
            .and_then(|()| table.raw_set(draw_sub_level_key(), 0_i16))
            .and_then(|()| {
                table.raw_set(
                    texture_color_key(),
                    lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
                )
            })
            .map_err(|error| execution_error("object registration", error))?;
        if is_frame_object(object.kind()) {
            let script_handlers = lua
                .create_table()
                .map_err(|error| execution_error("object registration", error))?;
            let script_node = scripts
                .node(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("frame object {node_index} has no script state"),
                })?;
            for binding in scripts.bindings_for(script_node) {
                let value = match binding.target() {
                    UiScriptTarget::Compiled(index) => Value::Function(
                        scripts
                            .compiled_function(lua, *index)
                            .map_err(|error| execution_error("object registration", error))?,
                    ),
                    UiScriptTarget::Global(name) => Value::String(
                        lua.create_string(name)
                            .map_err(|error| execution_error("object registration", error))?,
                    ),
                };
                script_handlers
                    .raw_set(binding.handler().name(), value)
                    .map_err(|error| execution_error("object registration", error))?;
            }
            table
                .raw_set(
                    events_key(),
                    lua.create_table()
                        .map_err(|error| execution_error("object registration", error))?,
                )
                .and_then(|()| table.raw_set(all_events_key(), false))
                .and_then(|()| table.raw_set(id_key(), frame_id))
                .and_then(|()| table.raw_set(frame_level_key(), frame_level))
                .and_then(|()| table.raw_set(frame_strata_key(), frame_strata))
                .and_then(|()| table.raw_set(keyboard_enabled_key(), keyboard_enabled))
                .and_then(|()| table.raw_set(script_handlers_key(), script_handlers))
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
            let button = self
                .buttons
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("button {node_index} has no initial button state"),
                })?;
            table
                .raw_set(highlight_locked_key(), false)
                .and_then(|()| table.raw_set(click_action_key(), 0_u64))
                .map_err(|error| execution_error("object registration", error))?;
            set_initial_font(
                lua,
                &table,
                normal_font_key(),
                button.normal_font.as_deref(),
            )
            .and_then(|()| {
                set_initial_font(
                    lua,
                    &table,
                    disabled_font_key(),
                    button.disabled_font.as_deref(),
                )
            })
            .and_then(|()| {
                set_initial_font(
                    lua,
                    &table,
                    highlight_font_key(),
                    button.highlight_font.as_deref(),
                )
            })
            .map_err(|error| execution_error("object registration", error))?;
            if let Some(reference) = &button.text_reference {
                let text = stock_text(lua, reference)
                    .map_err(|error| execution_error("object registration", error))?;
                table
                    .raw_set(text_key(), text)
                    .map_err(|error| execution_error("object registration", error))?;
            }
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
                .and_then(|()| table.raw_set(model_sequence_time_sequence_key(), 0_u32))
                .and_then(|()| table.raw_set(model_sequence_time_key(), 0_i32))
                .and_then(|()| table.raw_set(model_scale_key(), 1.0))
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
            let texture = self
                .textures
                .get(node_index)
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("texture {node_index} has no initial state"),
                })?;
            table
                .raw_set(
                    tex_coord_key(),
                    lua.create_sequence_from(texture.coords)
                        .map_err(|error| execution_error("object registration", error))?,
                )
                .and_then(|()| {
                    table.raw_set(
                        texture_color_key(),
                        lua.create_sequence_from(texture.colors.iter().flatten().copied())?,
                    )
                })
                .and_then(|()| table.raw_set(texture_file_key(), texture.file.as_deref()))
                .and_then(|()| table.raw_set(texture_solid_color_key(), Option::<Table>::None))
                .and_then(|()| table.raw_set(texture_blend_mode_key(), texture.blend_mode))
                .and_then(|()| table.raw_set(horizontal_tiling_key(), texture.horizontal_tiling))
                .and_then(|()| table.raw_set(vertical_tiling_key(), texture.vertical_tiling))
                .and_then(|()| table.raw_set(non_blocking_key(), texture.non_blocking))
                .and_then(|()| table.raw_set(draw_layer_key(), texture.draw_layer))
                .and_then(|()| table.raw_set(draw_sub_level_key(), texture.draw_sub_level))
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
        if let Some(key) = widget_region_key(object.role())
            && let Some(parent) = object.parent()
        {
            let owner: Table = objects
                .raw_get(parent + 1)
                .map_err(|error| execution_error("object registration", error))?;
            owner
                .raw_set(key, table.clone())
                .map_err(|error| execution_error("object registration", error))?;
            if object.role() == UiObjectRole::ButtonText
                && let Some(font) = owner
                    .raw_get::<Option<Table>>(normal_font_key())
                    .map_err(|error| execution_error("object registration", error))?
            {
                table
                    .raw_set(font_object_key(), font)
                    .and_then(|()| table.raw_set(font_set_key(), true))
                    .map_err(|error| execution_error("object registration", error))?;
            }
        }

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
        node_index: usize,
    ) -> Result<(), UiScriptError> {
        let object = tree
            .nodes()
            .get(node_index)
            .ok_or_else(|| UiScriptError::Plan {
                message: format!("OnLoad object index {node_index} is outside the arena"),
            })?;
        if !is_frame_object(object.kind()) {
            return Ok(());
        }
        let label = format!(
            "{}<{}>:OnLoad",
            object.name().unwrap_or("<unnamed>"),
            object_type_name(object.kind())
        );
        let objects: Table = lua
            .named_registry_value(OBJECT_REGISTRY)
            .map_err(|error| execution_error(&label, error))?;
        let table: Table = objects
            .raw_get(node_index + 1)
            .map_err(|error| execution_error(&label, error))?;
        let Some(function) = object_script_function(lua, &table, UiScriptHandler::Load)
            .map_err(|error| execution_error(&label, error))?
        else {
            return Ok(());
        };
        call_object_handler(lua, &function, table)
            .map_err(|error| execution_error(&label, error))?;
        self.executed_load_handlers += 1;
        Ok(())
    }
}

fn register_create_frame(
    lua: &Lua,
    static_object_count: usize,
    registered_objects: Rc<Cell<usize>>,
    dynamic_objects: Rc<Cell<usize>>,
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
                    &DynamicArenaCounters {
                        static_object_count,
                        registered_objects: &registered_objects,
                        dynamic_objects: &dynamic_objects,
                    },
                )
            },
        )?,
    )
}

struct DynamicArenaCounters<'state> {
    static_object_count: usize,
    registered_objects: &'state Cell<usize>,
    dynamic_objects: &'state Cell<usize>,
}

fn create_dynamic_frame(
    lua: &Lua,
    requested_kind: &str,
    requested_name: Option<&str>,
    requested_parent: Option<Table>,
    descriptor: Option<Table>,
    counters: &DynamicArenaCounters<'_>,
) -> mlua::Result<Table> {
    let records = if let Some(descriptor) = &descriptor {
        descriptor.raw_get::<Table>("nodes")?
    } else {
        let records = lua.create_table()?;
        let record = lua.create_table()?;
        record.raw_set("kind", requested_kind)?;
        record.raw_set("role", "object")?;
        record.raw_set("root_name", true)?;
        record.raw_set("children", lua.create_table()?)?;
        record.raw_set("width", 0.0)?;
        record.raw_set("height", 0.0)?;
        record.raw_set("shown", true)?;
        record.raw_set("alpha", 1.0)?;
        record.raw_set("scale", 1.0)?;
        record.raw_set("anchors", lua.create_table()?)?;
        record.raw_set("font_assigned", false)?;
        record.raw_set("justify_h", "CENTER")?;
        record.raw_set("justify_v", "MIDDLE")?;
        record.raw_set(
            "texture_coords",
            lua.create_sequence_from([0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0])?,
        )?;
        record.raw_set("texture_color", lua.create_sequence_from([1.0; 16])?)?;
        record.raw_set("texture_file", Option::<String>::None)?;
        record.raw_set("texture_blend_mode", "BLEND")?;
        record.raw_set("horizontal_tiling", false)?;
        record.raw_set("vertical_tiling", false)?;
        record.raw_set("non_blocking", false)?;
        record.raw_set("draw_layer", "ARTWORK")?;
        record.raw_set("draw_sub_level", 0_i16)?;
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
    let requested_name = requested_name
        .map(|name| expand_dynamic_parent_name(lua, name, requested_parent.as_ref()))
        .transpose()?;
    let base_name = requested_name.clone().or_else(|| {
        requested_parent
            .as_ref()
            .and_then(|parent| parent.raw_get(name_key()).ok())
    });
    let count = records.raw_len();
    let mut objects = Vec::with_capacity(count);
    for local_index in 1..=count {
        let record: Table = records.raw_get(local_index)?;
        let kind = record.raw_get::<String>("kind")?;
        let name = dynamic_name(&record, requested_name.as_deref(), base_name.as_deref())?;
        let parent = if local_index == root_local {
            requested_parent.clone()
        } else if let Some(parent_index) = record.raw_get::<Option<usize>>("parent")? {
            objects.get(parent_index - 1).cloned()
        } else if let Some(parent_name) = record.raw_get::<Option<String>>("external_parent")? {
            lua.globals().raw_get::<Option<Table>>(parent_name)?
        } else {
            None
        };
        let index = counters.static_object_count + counters.dynamic_objects.get() + 1;
        let object =
            create_dynamic_object(lua, &kind, name.as_deref(), parent.as_ref(), index, &record)?;
        counters
            .dynamic_objects
            .set(counters.dynamic_objects.get() + 1);
        counters
            .registered_objects
            .set(counters.registered_objects.get() + 1);
        objects.push(object);
    }
    apply_dynamic_anchors(lua, &records, &objects)?;
    run_dynamic_load(lua, &records, &objects, root_local)?;
    objects
        .get(root_local - 1)
        .cloned()
        .ok_or_else(|| mlua::Error::runtime("CreateFrame root is outside its template"))
}

fn expand_dynamic_parent_name(
    lua: &Lua,
    source: &str,
    parent: Option<&Table>,
) -> mlua::Result<String> {
    let Some(prefix) = source.get(..7) else {
        return Ok(source.to_owned());
    };
    if !prefix.eq_ignore_ascii_case("$parent") {
        return Ok(source.to_owned());
    }

    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut cursor = parent.cloned();
    while let Some(candidate) = cursor {
        if let Some(name) = candidate.raw_get::<Option<String>>(name_key())?
            && !name.is_empty()
        {
            return Ok(format!("{name}{}", &source[7..]));
        }
        cursor = candidate
            .raw_get::<Option<usize>>(parent_key())?
            .map(|index| objects.raw_get::<Table>(index))
            .transpose()?;
    }
    Ok(source[7..].to_owned())
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
    object.raw_set(role_key(), record.raw_get::<String>("role")?)?;
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
    object.raw_set(alpha_key(), record.raw_get::<f64>("alpha")?)?;
    object.raw_set(scale_key(), record.raw_get::<f64>("scale")?)?;
    object.raw_set(anchors_key(), lua.create_table()?)?;
    object.raw_set(draw_layer_key(), record.raw_get::<String>("draw_layer")?)?;
    object.raw_set(
        draw_sub_level_key(),
        record.raw_get::<i16>("draw_sub_level")?,
    )?;
    object.raw_set(
        texture_color_key(),
        lua.create_sequence_from([1.0, 1.0, 1.0, 1.0])?,
    )?;
    if !matches!(kind, "Texture" | "FontString") {
        object.raw_set(events_key(), lua.create_table()?)?;
        object.raw_set(all_events_key(), false)?;
        object.raw_set(id_key(), 0)?;
        let level = parent
            .map(|parent| parent.raw_get::<i32>(frame_level_key()))
            .transpose()?
            .map_or(0, |level| level.saturating_add(1));
        object.raw_set(frame_level_key(), level)?;
        let strata = parent
            .map(|parent| parent.raw_get::<String>(frame_strata_key()))
            .transpose()?
            .unwrap_or_else(|| "MEDIUM".to_owned());
        object.raw_set(frame_strata_key(), strata)?;
        object.raw_set(keyboard_enabled_key(), false)?;
        let handlers = lua.create_table()?;
        if let Some(initial) = record.raw_get::<Option<Table>>("scripts")? {
            for pair in initial.pairs::<String, Value>() {
                let (name, value) = pair?;
                handlers.raw_set(name, value)?;
            }
        }
        object.raw_set(script_handlers_key(), handlers)?;
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
        object.raw_set(model_sequence_time_sequence_key(), 0_u32)?;
        object.raw_set(model_sequence_time_key(), 0_i32)?;
        object.raw_set(model_scale_key(), 1.0)?;
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
        object.raw_set(
            texture_color_key(),
            record.raw_get::<Table>("texture_color")?,
        )?;
        object.raw_set(
            texture_file_key(),
            record.raw_get::<Option<String>>("texture_file")?,
        )?;
        object.raw_set(texture_solid_color_key(), Option::<Table>::None)?;
        object.raw_set(
            texture_blend_mode_key(),
            record.raw_get::<String>("texture_blend_mode")?,
        )?;
        object.raw_set(
            horizontal_tiling_key(),
            record.raw_get::<bool>("horizontal_tiling")?,
        )?;
        object.raw_set(
            vertical_tiling_key(),
            record.raw_get::<bool>("vertical_tiling")?,
        )?;
        object.raw_set(non_blocking_key(), record.raw_get::<bool>("non_blocking")?)?;
        object.raw_set(draw_layer_key(), record.raw_get::<String>("draw_layer")?)?;
        object.raw_set(
            draw_sub_level_key(),
            record.raw_get::<i16>("draw_sub_level")?,
        )?;
    }
    let metatables: Table = lua.named_registry_value(METATABLE_REGISTRY)?;
    object.set_metatable(Some(metatables.raw_get(kind)?))?;
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    objects.raw_set(index, object.clone())?;
    if let Some(parent) = parent
        && let Some(key) = dynamic_widget_region_key(record.raw_get::<String>("role")?.as_str())
    {
        parent.raw_set(key, object.clone())?;
        if key == button_text_key()
            && let Some(font) = parent.raw_get::<Option<Table>>(normal_font_key())?
        {
            object.raw_set(font_object_key(), font)?;
            object.raw_set(font_set_key(), true)?;
        }
    }
    if let Some(name) = name
        && matches!(lua.globals().raw_get::<Value>(name)?, Value::Nil)
    {
        lua.globals().raw_set(name, object.clone())?;
    }
    Ok(object)
}

fn apply_dynamic_anchors(lua: &Lua, records: &Table, objects: &[Table]) -> mlua::Result<()> {
    for (local_index, object) in objects.iter().enumerate() {
        let record: Table = records.raw_get(local_index + 1)?;
        let prototypes: Table = record.raw_get("anchors")?;
        let anchors = lua.create_table()?;
        for point_index in 1..=9 {
            let Some(prototype) = prototypes.raw_get::<Option<Table>>(point_index)? else {
                continue;
            };
            let point_name = prototype.raw_get::<String>("point")?;
            let point = parse_point(&point_name)
                .ok_or_else(|| mlua::Error::runtime("runtime template has unknown anchor point"))?;
            let relative_name = prototype.raw_get::<String>("relative_point")?;
            let relative_point = parse_point(&relative_name).ok_or_else(|| {
                mlua::Error::runtime("runtime template has unknown relative anchor point")
            })?;
            let target_kind = prototype.raw_get::<String>("target_kind")?;
            let target = match target_kind.as_str() {
                "parent" => object.raw_get::<Option<usize>>(parent_key())?,
                "local" => {
                    let target = prototype.raw_get::<usize>("target")?;
                    Some(
                        objects
                            .get(target.saturating_sub(1))
                            .ok_or_else(|| {
                                mlua::Error::runtime(
                                    "runtime template anchor target is outside its instance",
                                )
                            })?
                            .raw_get::<usize>(index_key())?,
                    )
                }
                "global" => {
                    let name = prototype.raw_get::<String>("target")?;
                    let target: Table = lua.globals().raw_get(name.as_str()).map_err(|_| {
                        mlua::Error::runtime(format!(
                            "runtime template anchor target {name} is unavailable"
                        ))
                    })?;
                    Some(target.raw_get::<usize>(index_key())?)
                }
                _ => {
                    return Err(mlua::Error::runtime(
                        "runtime template has unknown anchor target kind",
                    ));
                }
            };
            let anchor = create_anchor_record(
                lua,
                point,
                target,
                relative_point,
                (
                    prototype.raw_get::<f64>("x")?,
                    prototype.raw_get::<f64>("y")?,
                ),
            )?;
            anchors.raw_set(point_index, anchor)?;
        }
        object.raw_set(anchors_key(), anchors)?;
    }
    Ok(())
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
    let object = objects
        .get(local - 1)
        .cloned()
        .ok_or_else(|| mlua::Error::runtime("dynamic OnLoad object is outside its template"))?;
    if let Some(function) = object_script_function(lua, &object, UiScriptHandler::Load)? {
        call_object_handler(lua, &function, object)?;
    }
    Ok(())
}

/// Installs and restores build 12340's legacy global `this` around callbacks.
fn call_object_handler(lua: &Lua, function: &mlua::Function, object: Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous = globals.raw_get::<Value>("this")?;
    globals.raw_set("this", object.clone())?;
    let result = function.call::<()>(object);
    let restore = globals.raw_set("this", previous);
    match result {
        Ok(()) => restore,
        Err(error) => {
            let _ = restore;
            Err(error)
        }
    }
}

fn dispatch_subscribers(
    lua: &Lua,
    object_count: usize,
    event: &str,
    payload: &[Value],
) -> mlua::Result<usize> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let mut dispatched = 0;
    for index in 1..=object_count {
        let object: Table = objects.raw_get(index)?;
        let all_events = object
            .raw_get::<Option<bool>>(all_events_key())?
            .unwrap_or(false);
        let subscribed = if all_events {
            true
        } else if let Some(events) = object.raw_get::<Option<Table>>(events_key())? {
            events.raw_get::<Option<bool>>(event)? == Some(true)
        } else {
            false
        };
        if !subscribed {
            continue;
        }
        let Some(function) = object_script_function(lua, &object, UiScriptHandler::Event)? else {
            continue;
        };
        call_event_handler(lua, &function, object, event, payload)?;
        dispatched += 1;
    }
    Ok(dispatched)
}

fn call_event_handler(
    lua: &Lua,
    function: &mlua::Function,
    object: Table,
    event: &str,
    payload: &[Value],
) -> mlua::Result<()> {
    let globals = lua.globals();
    let previous_this = globals.raw_get::<Value>("this")?;
    globals.raw_set("this", object.clone())?;
    let mut arguments = Vec::with_capacity(payload.len() + 2);
    arguments.push(Value::Table(object));
    arguments.push(Value::String(lua.create_string(event)?));
    arguments.extend(payload.iter().cloned());
    let result = function.call::<()>(MultiValue::from_vec(arguments));
    let restore = globals.raw_set("this", previous_this);
    match result {
        Ok(()) => restore,
        Err(error) => {
            let _ = restore;
            Err(error)
        }
    }
}

fn event_argument(lua: &Lua, argument: &UiEventArgument) -> mlua::Result<Value> {
    match argument {
        UiEventArgument::Nil => Ok(Value::Nil),
        UiEventArgument::Boolean(value) => Ok(Value::Boolean(*value)),
        UiEventArgument::Integer(value) => Ok(Value::Integer(*value)),
        UiEventArgument::Number(value) => Ok(Value::Number(*value)),
        UiEventArgument::String(value) => lua.create_string(value).map(Value::String),
    }
}

fn restore_event_globals(lua: &Lua, event: Value, arguments: Vec<Value>) -> mlua::Result<()> {
    let globals = lua.globals();
    globals.raw_set("event", event)?;
    for (index, value) in arguments.into_iter().enumerate() {
        globals.raw_set(format!("arg{}", index + 1), value)?;
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
    assets: Option<AssetStoreHandle>,
    button_measurement: Option<buttons::ButtonTextMeasurement>,
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
        register_frame_script_methods(lua, &methods, kind)?;
    }
    if is_enabled_control(kind) {
        register_enabled_methods(lua, &methods, kind)?;
    }
    if matches!(kind, UiObjectKind::Button | UiObjectKind::CheckButton) {
        buttons::register_button_methods(lua, &methods, button_measurement)?;
    }
    if kind == UiObjectKind::CheckButton {
        buttons::register_check_button_methods(lua, &methods)?;
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

fn register_frame_script_methods(
    lua: &Lua,
    methods: &Table,
    kind: UiObjectKind,
) -> mlua::Result<()> {
    methods.raw_set(
        "SetScript",
        lua.create_function(move |_, (object, name, value): (Table, String, Value)| {
            let handler = handler_for(kind, &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Unknown script handler: {name}")))?;
            if !matches!(value, Value::Nil | Value::Function(_)) {
                return Err(mlua::Error::runtime(format!(
                    "Usage: {}:SetScript(\"scriptType\", function)",
                    object_type_name(kind)
                )));
            }
            let handlers: Table = object.raw_get(script_handlers_key())?;
            handlers.raw_set(handler.name(), value)
        })?,
    )?;
    methods.raw_set(
        "GetScript",
        lua.create_function(move |lua, (object, name): (Table, String)| {
            let handler = handler_for(kind, &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Unknown script handler: {name}")))?;
            object_script_function(lua, &object, handler)
        })?,
    )?;
    methods.raw_set(
        "HasScript",
        lua.create_function(move |_, (object, name): (Table, String)| {
            let handler = handler_for(kind, &name)
                .ok_or_else(|| mlua::Error::runtime(format!("Unknown script handler: {name}")))?;
            let handlers: Table = object.raw_get(script_handlers_key())?;
            Ok(!matches!(
                handlers.raw_get::<Value>(handler.name())?,
                Value::Nil
            ))
        })?,
    )
}

fn object_script_function(
    lua: &Lua,
    object: &Table,
    handler: UiScriptHandler,
) -> mlua::Result<Option<mlua::Function>> {
    let Some(handlers) = object.raw_get::<Option<Table>>(script_handlers_key())? else {
        return Ok(None);
    };
    match handlers.raw_get::<Value>(handler.name())? {
        Value::Nil => Ok(None),
        Value::Function(function) => Ok(Some(function)),
        Value::String(name) => {
            let name = name.to_str()?;
            match lua.globals().raw_get::<Value>(&name)? {
                Value::Nil => Ok(None),
                Value::Function(function) => Ok(Some(function)),
                _ => Err(mlua::Error::runtime(format!(
                    "global handler {name} is not a function"
                ))),
            }
        }
        _ => Err(mlua::Error::runtime("script handler slot is invalid")),
    }
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
        "SetTexture",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            let first = arguments.first().cloned().unwrap_or(Value::Nil);
            if let Some(red) = lua.coerce_number(first.clone())? {
                let mut color = [red, 1.0, 1.0, 1.0];
                for (index, component) in color.iter_mut().enumerate().skip(1) {
                    if let Some(value) = arguments
                        .get(index)
                        .map(|value| lua.coerce_number(value.clone()))
                        .transpose()?
                        .flatten()
                    {
                        *component = value;
                    }
                }
                if color.iter().any(|value| !value.is_finite()) {
                    return Err(mlua::Error::runtime("SetTexture(): invalid color"));
                }
                texture.raw_set(texture_file_key(), Option::<String>::None)?;
                texture.raw_set(texture_solid_color_key(), lua.create_sequence_from(color)?)?;
                return Ok(());
            }
            let value = lua
                .coerce_string(first)?
                .map(|value| value.to_string_lossy())
                .unwrap_or_default();
            let file = (!value.is_empty()).then_some(value);
            texture.raw_set(texture_file_key(), file)?;
            texture.raw_set(texture_solid_color_key(), Option::<Table>::None)
        })?,
    )?;
    methods.raw_set(
        "GetTexture",
        lua.create_function(|_, texture: Table| {
            texture.raw_get::<Option<String>>(texture_file_key())
        })?,
    )?;
    methods.raw_set(
        "SetBlendMode",
        lua.create_function(|_, (texture, requested): (Table, String)| {
            let mode = if requested.eq_ignore_ascii_case("BLEND") {
                "BLEND"
            } else if requested.eq_ignore_ascii_case("ADD") {
                "ADD"
            } else {
                return Err(mlua::Error::runtime("invalid texture blend mode"));
            };
            texture.raw_set(texture_blend_mode_key(), mode)
        })?,
    )?;
    methods.raw_set(
        "GetBlendMode",
        lua.create_function(|_, texture: Table| {
            texture.raw_get::<String>(texture_blend_mode_key())
        })?,
    )?;
    register_texture_flag_methods(lua, methods)?;
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
    )?;
    methods.raw_set(
        "SetVertexColor",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            let mut color = [0.0, 0.0, 0.0, 1.0];
            for (index, component) in color.iter_mut().take(3).enumerate() {
                *component = arguments
                    .get(index)
                    .map(|value| lua.coerce_number(value.clone()))
                    .transpose()?
                    .flatten()
                    .unwrap_or(0.0);
            }
            color[3] = arguments
                .get(3)
                .map(|value| lua.coerce_number(value.clone()))
                .transpose()?
                .flatten()
                .unwrap_or(1.0);
            if color.iter().any(|value| !value.is_finite()) {
                return Err(mlua::Error::runtime("invalid vertex color"));
            }
            texture.raw_set(
                texture_color_key(),
                lua.create_sequence_from(color.into_iter().cycle().take(16))?,
            )
        })?,
    )?;
    methods.raw_set(
        "GetVertexColor",
        lua.create_function(|_, texture: Table| {
            let color: Table = texture.raw_get(texture_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )?;
    methods.raw_set(
        "SetGradient",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            set_texture_gradient(lua, &texture, arguments.as_slice(), false)
        })?,
    )?;
    methods.raw_set(
        "SetGradientAlpha",
        lua.create_function(|lua, (texture, arguments): (Table, Variadic<Value>)| {
            set_texture_gradient(lua, &texture, arguments.as_slice(), true)
        })?,
    )
}

fn register_texture_flag_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    for (set_name, get_name, key) in [
        ("SetHorizTile", "GetHorizTile", horizontal_tiling_key()),
        ("SetVertTile", "GetVertTile", vertical_tiling_key()),
        ("SetNonBlocking", "GetNonBlocking", non_blocking_key()),
    ] {
        methods.raw_set(
            set_name,
            lua.create_function(move |_, (texture, enabled): (Table, Option<bool>)| {
                texture.raw_set(key, enabled.unwrap_or(true))
            })?,
        )?;
        methods.raw_set(
            get_name,
            lua.create_function(move |_, texture: Table| {
                Ok(texture.raw_get::<bool>(key)?.then_some(Value::Number(1.0)))
            })?,
        )?;
    }
    Ok(())
}

fn set_texture_gradient(
    lua: &Lua,
    texture: &Table,
    arguments: &[Value],
    includes_alpha: bool,
) -> mlua::Result<()> {
    let orientation = arguments
        .first()
        .and_then(|value| value.as_string())
        .map(|value| value.to_string_lossy())
        .ok_or_else(|| mlua::Error::runtime("invalid Texture gradient"))?;
    let stride = if includes_alpha { 4 } else { 3 };
    if arguments.len() != 1 + stride * 2 {
        return Err(mlua::Error::runtime("invalid Texture gradient"));
    }
    let mut minimum = [0.0, 0.0, 0.0, 1.0];
    let mut maximum = [0.0, 0.0, 0.0, 1.0];
    for channel in 0..stride {
        minimum[channel] = lua
            .coerce_number(arguments[1 + channel].clone())?
            .ok_or_else(|| mlua::Error::runtime("invalid Texture gradient"))?
            .clamp(0.0, 1.0);
        maximum[channel] = lua
            .coerce_number(arguments[1 + stride + channel].clone())?
            .ok_or_else(|| mlua::Error::runtime("invalid Texture gradient"))?
            .clamp(0.0, 1.0);
    }
    let colors = if orientation.eq_ignore_ascii_case("VERTICAL") {
        [maximum, minimum, maximum, minimum]
    } else if orientation.eq_ignore_ascii_case("HORIZONTAL") {
        [minimum, minimum, maximum, maximum]
    } else {
        return Err(mlua::Error::runtime("invalid Texture gradient"));
    };
    texture.raw_set(
        texture_color_key(),
        lua.create_sequence_from(colors.into_iter().flatten())?,
    )
}

fn register_model_methods(
    lua: &Lua,
    methods: &Table,
    assets: Option<AssetStoreHandle>,
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
    )?;
    methods.raw_set(
        "SetSequenceTime",
        lua.create_function(|lua, (model, sequence, time): (Table, Value, Value)| {
            let sequence = lua.coerce_number(sequence)?.ok_or_else(|| {
                mlua::Error::runtime("Usage: Model:SetSequenceTime(sequence, time)")
            })?;
            let time = lua.coerce_number(time)?.ok_or_else(|| {
                mlua::Error::runtime("Usage: Model:SetSequenceTime(sequence, time)")
            })?;
            model.raw_set(model_sequence_time_sequence_key(), sequence as u32)?;
            model.raw_set(model_sequence_time_key(), time as i32)
        })?,
    )?;
    methods.raw_set(
        "SetModelScale",
        lua.create_function(|lua, (model, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Model:SetModelScale(scale)"))?;
            model.raw_set(model_scale_key(), value)
        })?,
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

fn register_frame_visibility_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "GetID",
        lua.create_function(|_, object: Table| object.raw_get::<i32>(id_key()))?,
    )?;
    methods.raw_set(
        "SetID",
        lua.create_function(|_, (object, id): (Table, i32)| object.raw_set(id_key(), id))?,
    )?;
    methods.raw_set(
        "GetFrameLevel",
        lua.create_function(|_, object: Table| object.raw_get::<i32>(frame_level_key()))?,
    )?;
    methods.raw_set(
        "SetFrameLevel",
        lua.create_function(|lua, (object, value): (Table, Value)| {
            let value = lua
                .coerce_number(value)?
                .ok_or_else(|| mlua::Error::runtime("Usage: Frame:SetFrameLevel(level)"))?
                as i32;
            if value < 0 {
                return Err(mlua::Error::runtime(format!(
                    "Frame:SetFrameLevel(): Passed negative frame level: {value}"
                )));
            }
            object.raw_set(frame_level_key(), value)
        })?,
    )?;
    methods.raw_set(
        "GetFrameStrata",
        lua.create_function(|_, object: Table| object.raw_get::<String>(frame_strata_key()))?,
    )?;
    methods.raw_set(
        "SetFrameStrata",
        lua.create_function(|_, (object, requested): (Table, String)| {
            let strata = parse_frame_strata_name(&requested)
                .ok_or_else(|| mlua::Error::runtime("invalid frame strata"))?;
            object.raw_set(frame_strata_key(), strata)
        })?,
    )?;
    methods.raw_set(
        "EnableKeyboard",
        lua.create_function(|_, (object, arguments): (Table, Variadic<Value>)| {
            let enabled = arguments.first().is_none_or(|value| lua_bool(value, true));
            object.raw_set(keyboard_enabled_key(), enabled)
        })?,
    )?;
    methods.raw_set(
        "IsKeyboardEnabled",
        lua.create_function(|_, object: Table| {
            Ok(object
                .raw_get::<bool>(keyboard_enabled_key())?
                .then_some(Value::Number(1.0)))
        })?,
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
        "GetAlpha",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(alpha_key()))?,
    )?;
    methods.raw_set(
        "SetAlpha",
        lua.create_function(|_, (object, alpha): (Table, f64)| object.raw_set(alpha_key(), alpha))?,
    )?;
    methods.raw_set(
        "GetScale",
        lua.create_function(|_, object: Table| object.raw_get::<f64>(scale_key()))?,
    )?;
    methods.raw_set(
        "SetDrawLayer",
        lua.create_function(
            |_, (object, requested, sub_level): (Table, String, Option<i32>)| {
                let layer = parse_draw_layer_name(&requested)
                    .ok_or_else(|| mlua::Error::runtime("invalid draw layer"))?;
                let sub_level = i16::try_from(sub_level.unwrap_or(0))
                    .map_err(|_| mlua::Error::runtime("invalid draw layer"))?;
                object.raw_set(draw_layer_key(), layer)?;
                object.raw_set(draw_sub_level_key(), sub_level)
            },
        )?,
    )?;
    methods.raw_set(
        "GetDrawLayer",
        lua.create_function(|_, object: Table| object.raw_get::<String>(draw_layer_key()))?,
    )?;
    methods.raw_set(
        "SetScale",
        lua.create_function(|_, (object, scale): (Table, f64)| {
            if scale <= 0.0 {
                return Err(mlua::Error::runtime("SetScale(): scale must be positive"));
            }
            object.raw_set(scale_key(), scale)
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
    register_region_vertex_color_methods(lua, methods)?;
    register_region_visibility_methods(lua, methods)?;
    Ok(())
}

fn register_region_vertex_color_methods(lua: &Lua, methods: &Table) -> mlua::Result<()> {
    methods.raw_set(
        "SetVertexColor",
        lua.create_function(|lua, (object, arguments): (Table, Variadic<Value>)| {
            let mut color = [0.0, 0.0, 0.0, 1.0];
            for (index, component) in color.iter_mut().take(3).enumerate() {
                *component = arguments
                    .get(index)
                    .map(|value| lua.coerce_number(value.clone()))
                    .transpose()?
                    .flatten()
                    .unwrap_or(0.0);
            }
            color[3] = arguments
                .get(3)
                .map(|value| lua.coerce_number(value.clone()))
                .transpose()?
                .flatten()
                .unwrap_or(1.0);
            if color.iter().any(|value| !value.is_finite()) {
                return Err(mlua::Error::runtime("invalid vertex color"));
            }
            let kind = object.raw_get::<String>(type_key())?;
            let component_count = if kind == "Texture" { 16 } else { 4 };
            object.raw_set(
                texture_color_key(),
                lua.create_sequence_from(color.into_iter().cycle().take(component_count))?,
            )
        })?,
    )?;
    methods.raw_set(
        "GetVertexColor",
        lua.create_function(|_, object: Table| {
            let color: Table = object.raw_get(texture_color_key())?;
            Ok((
                color.raw_get::<f64>(1)?,
                color.raw_get::<f64>(2)?,
                color.raw_get::<f64>(3)?,
                color.raw_get::<f64>(4)?,
            ))
        })?,
    )
}

fn parse_draw_layer_name(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("BACKGROUND") {
        Some("BACKGROUND")
    } else if value.eq_ignore_ascii_case("BORDER") {
        Some("BORDER")
    } else if value.eq_ignore_ascii_case("ARTWORK") {
        Some("ARTWORK")
    } else if value.eq_ignore_ascii_case("OVERLAY") {
        Some("OVERLAY")
    } else if value.eq_ignore_ascii_case("HIGHLIGHT") {
        Some("HIGHLIGHT")
    } else {
        None
    }
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
            let name = resolve_region_name(lua, object, name.to_str()?.as_ref())?;
            let table = lua.globals().raw_get::<Table>(name.as_str()).map_err(|_| {
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
                let name = resolve_region_name(lua, object, name.to_str()?.as_ref())?;
                let relative: Value = lua.globals().raw_get(name.as_str())?;
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

fn resolve_region_name(lua: &Lua, object: &Table, source: &str) -> mlua::Result<String> {
    let objects: Table = lua.named_registry_value(OBJECT_REGISTRY)?;
    let parent = object
        .raw_get::<Option<usize>>(parent_key())?
        .map(|index| objects.raw_get::<Table>(index))
        .transpose()?;
    expand_dynamic_parent_name(lua, source, parent.as_ref())
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

pub(super) fn parse_point(value: &str) -> Option<UiPoint> {
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
        UiManifestKind::Glue => Ok(canonical_glue_event(name)),
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

fn tree_buttons(tree: &UiObjectTree<'_>) -> Vec<InitialButton> {
    tree.nodes()
        .iter()
        .map(|node| {
            if !matches!(
                node.kind(),
                UiObjectKind::Button | UiObjectKind::CheckButton
            ) {
                return InitialButton::default();
            }
            let mut initial = InitialButton::default();
            for layer in node.layers() {
                if let Some(reference) = xml_attribute(layer.element(), "text") {
                    initial.text_reference = (!reference.is_empty()).then(|| reference.to_owned());
                }
                for content in layer.element().content() {
                    let XmlContent::Element(index) = content else {
                        continue;
                    };
                    let Some(child) = layer.document().element(*index) else {
                        continue;
                    };
                    let target = match child.name() {
                        "NormalFont" => &mut initial.normal_font,
                        "DisabledFont" => &mut initial.disabled_font,
                        "HighlightFont" => &mut initial.highlight_font,
                        _ => continue,
                    };
                    if let Some(style) = xml_attribute(child, "style") {
                        *target = (!style.is_empty()).then(|| style.to_owned());
                    }
                }
            }
            initial
        })
        .collect()
}

fn set_initial_font(
    lua: &Lua,
    button: &Table,
    key: LightUserData,
    name: Option<&str>,
) -> mlua::Result<()> {
    let Some(name) = name else {
        return Ok(());
    };
    let font = lua.globals().raw_get::<Option<Table>>(name)?;
    if let Some(font) = font {
        button.raw_set(key, font)?;
    }
    Ok(())
}

fn stock_text(lua: &Lua, reference: &str) -> mlua::Result<String> {
    let localized = lua.globals().raw_get::<Option<String>>(reference)?;
    Ok(localized
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| reference.to_owned()))
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

fn tree_textures(
    tree: &UiObjectTree<'_>,
    textures: &UiTextureStatePlan,
) -> Result<Vec<InitialTexture>, UiScriptError> {
    tree.nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let initial = InitialTexture::default();
            if node.kind() != UiObjectKind::Texture {
                return Ok(initial);
            }
            textures
                .state(index)
                .map(|state| InitialTexture {
                    file: match state.file() {
                        Some(UiTextureFile::Asset(path)) => Some(path.as_str().to_owned()),
                        Some(UiTextureFile::Dynamic) | None => None,
                    },
                    coords: state.tex_coords().map(f64::from),
                    colors: state.vertex_colors().map(|color| color.map(f64::from)),
                    blend_mode: blend_mode_name(state.blend_mode()),
                    horizontal_tiling: state.horizontal_tiling(),
                    vertical_tiling: state.vertical_tiling(),
                    non_blocking: state.non_blocking(),
                    draw_layer: draw_layer_name(state.draw_layer()),
                    draw_sub_level: state.draw_sub_level(),
                })
                .ok_or_else(|| UiScriptError::Plan {
                    message: format!("texture {index} is outside the texture state plan"),
                })
        })
        .collect()
}

fn blend_mode_name(value: UiBlendMode) -> &'static str {
    match value {
        UiBlendMode::Blend => "BLEND",
        UiBlendMode::Add => "ADD",
    }
}

fn draw_layer_name(value: UiDrawLayer) -> &'static str {
    match value {
        UiDrawLayer::Background => "BACKGROUND",
        UiDrawLayer::Border => "BORDER",
        UiDrawLayer::Artwork => "ARTWORK",
        UiDrawLayer::Overlay => "OVERLAY",
        UiDrawLayer::Highlight => "HIGHLIGHT",
    }
}

fn frame_strata_name(value: UiFrameStrata) -> &'static str {
    match value {
        UiFrameStrata::Background => "BACKGROUND",
        UiFrameStrata::Low => "LOW",
        UiFrameStrata::Medium => "MEDIUM",
        UiFrameStrata::High => "HIGH",
        UiFrameStrata::Dialog => "DIALOG",
        UiFrameStrata::Fullscreen => "FULLSCREEN",
        UiFrameStrata::FullscreenDialog => "FULLSCREEN_DIALOG",
        UiFrameStrata::Tooltip => "TOOLTIP",
    }
}

fn parse_frame_strata_name(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("BACKGROUND") {
        Some("BACKGROUND")
    } else if value.eq_ignore_ascii_case("LOW") {
        Some("LOW")
    } else if value.eq_ignore_ascii_case("MEDIUM") {
        Some("MEDIUM")
    } else if value.eq_ignore_ascii_case("HIGH") {
        Some("HIGH")
    } else if value.eq_ignore_ascii_case("DIALOG") {
        Some("DIALOG")
    } else if value.eq_ignore_ascii_case("FULLSCREEN") {
        Some("FULLSCREEN")
    } else if value.eq_ignore_ascii_case("FULLSCREEN_DIALOG") {
        Some("FULLSCREEN_DIALOG")
    } else if value.eq_ignore_ascii_case("TOOLTIP") {
        Some("TOOLTIP")
    } else {
        None
    }
}

fn xml_attribute<'a>(element: &'a crate::XmlElement, name: &str) -> Option<&'a str> {
    element
        .attributes()
        .iter()
        .find(|attribute| attribute.name() == name)
        .map(|attribute| attribute.value())
}

fn widget_region_key(role: UiObjectRole) -> Option<LightUserData> {
    match role {
        UiObjectRole::ButtonText => Some(button_text_key()),
        UiObjectRole::NormalTexture => Some(normal_texture_key()),
        UiObjectRole::PushedTexture => Some(pushed_texture_key()),
        UiObjectRole::DisabledTexture => Some(disabled_texture_key()),
        UiObjectRole::HighlightTexture => Some(highlight_texture_key()),
        _ => None,
    }
}

fn dynamic_widget_region_key(role: &str) -> Option<LightUserData> {
    match role {
        "button_text" => Some(button_text_key()),
        "normal_texture" => Some(normal_texture_key()),
        "pushed_texture" => Some(pushed_texture_key()),
        "disabled_texture" => Some(disabled_texture_key()),
        "highlight_texture" => Some(highlight_texture_key()),
        _ => None,
    }
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

fn object_role_name(role: UiObjectRole) -> &'static str {
    match role {
        UiObjectRole::Object => "object",
        UiObjectRole::ButtonText => "button_text",
        UiObjectRole::NormalTexture => "normal_texture",
        UiObjectRole::PushedTexture => "pushed_texture",
        UiObjectRole::DisabledTexture => "disabled_texture",
        UiObjectRole::HighlightTexture => "highlight_texture",
        UiObjectRole::CheckedTexture => "checked_texture",
        UiObjectRole::DisabledCheckedTexture => "disabled_checked_texture",
        UiObjectRole::ThumbTexture => "thumb_texture",
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

pub(super) fn name_key() -> LightUserData {
    hidden_key(&NAME_TOKEN)
}

pub(super) fn type_key() -> LightUserData {
    hidden_key(&TYPE_TOKEN)
}

pub(super) fn role_key() -> LightUserData {
    hidden_key(&ROLE_TOKEN)
}

pub(super) fn parent_key() -> LightUserData {
    hidden_key(&PARENT_TOKEN)
}

fn events_key() -> LightUserData {
    hidden_key(&EVENTS_TOKEN)
}

fn all_events_key() -> LightUserData {
    hidden_key(&ALL_EVENTS_TOKEN)
}

pub(super) fn width_key() -> LightUserData {
    hidden_key(&WIDTH_TOKEN)
}

pub(super) fn height_key() -> LightUserData {
    hidden_key(&HEIGHT_TOKEN)
}

fn backdrop_color_key() -> LightUserData {
    hidden_key(&BACKDROP_COLOR_TOKEN)
}

fn backdrop_border_color_key() -> LightUserData {
    hidden_key(&BACKDROP_BORDER_COLOR_TOKEN)
}

pub(super) fn index_key() -> LightUserData {
    hidden_key(&INDEX_TOKEN)
}

pub(super) fn anchors_key() -> LightUserData {
    hidden_key(&ANCHORS_TOKEN)
}

pub(super) fn enabled_key() -> LightUserData {
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

pub(super) fn shown_key() -> LightUserData {
    hidden_key(&SHOWN_TOKEN)
}

pub(super) fn alpha_key() -> LightUserData {
    hidden_key(&ALPHA_TOKEN)
}

pub(super) fn scale_key() -> LightUserData {
    hidden_key(&SCALE_TOKEN)
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

pub(super) fn highlight_locked_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_LOCKED_TOKEN)
}

fn font_set_key() -> LightUserData {
    hidden_key(&FONT_SET_TOKEN)
}

fn font_object_key() -> LightUserData {
    hidden_key(&FONT_OBJECT_TOKEN)
}

pub(super) fn tex_coord_key() -> LightUserData {
    hidden_key(&TEX_COORD_TOKEN)
}

fn justify_h_key() -> LightUserData {
    hidden_key(&JUSTIFY_H_TOKEN)
}

fn justify_v_key() -> LightUserData {
    hidden_key(&JUSTIFY_V_TOKEN)
}

pub(super) fn checked_key() -> LightUserData {
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

pub(super) fn click_action_key() -> LightUserData {
    hidden_key(&CLICK_ACTION_TOKEN)
}

fn model_sequence_time_sequence_key() -> LightUserData {
    hidden_key(&MODEL_SEQUENCE_TIME_SEQUENCE_TOKEN)
}

fn model_sequence_time_key() -> LightUserData {
    hidden_key(&MODEL_SEQUENCE_TIME_TOKEN)
}

pub(super) fn frame_level_key() -> LightUserData {
    hidden_key(&FRAME_LEVEL_TOKEN)
}

fn model_scale_key() -> LightUserData {
    hidden_key(&MODEL_SCALE_TOKEN)
}

fn keyboard_enabled_key() -> LightUserData {
    hidden_key(&KEYBOARD_ENABLED_TOKEN)
}

pub(super) fn texture_color_key() -> LightUserData {
    hidden_key(&TEXTURE_COLOR_TOKEN)
}

pub(super) fn texture_file_key() -> LightUserData {
    hidden_key(&TEXTURE_FILE_TOKEN)
}

pub(super) fn texture_solid_color_key() -> LightUserData {
    hidden_key(&TEXTURE_SOLID_COLOR_TOKEN)
}

pub(super) fn texture_blend_mode_key() -> LightUserData {
    hidden_key(&TEXTURE_BLEND_MODE_TOKEN)
}

pub(super) fn horizontal_tiling_key() -> LightUserData {
    hidden_key(&HORIZONTAL_TILING_TOKEN)
}

pub(super) fn vertical_tiling_key() -> LightUserData {
    hidden_key(&VERTICAL_TILING_TOKEN)
}

pub(super) fn non_blocking_key() -> LightUserData {
    hidden_key(&NON_BLOCKING_TOKEN)
}

pub(super) fn draw_layer_key() -> LightUserData {
    hidden_key(&DRAW_LAYER_TOKEN)
}

pub(super) fn draw_sub_level_key() -> LightUserData {
    hidden_key(&DRAW_SUB_LEVEL_TOKEN)
}

pub(super) fn frame_strata_key() -> LightUserData {
    hidden_key(&FRAME_STRATA_TOKEN)
}

fn normal_texture_key() -> LightUserData {
    hidden_key(&NORMAL_TEXTURE_TOKEN)
}

fn pushed_texture_key() -> LightUserData {
    hidden_key(&PUSHED_TEXTURE_TOKEN)
}

fn disabled_texture_key() -> LightUserData {
    hidden_key(&DISABLED_TEXTURE_TOKEN)
}

fn highlight_texture_key() -> LightUserData {
    hidden_key(&HIGHLIGHT_TEXTURE_TOKEN)
}

fn script_handlers_key() -> LightUserData {
    hidden_key(&SCRIPT_HANDLERS_TOKEN)
}

fn button_text_key() -> LightUserData {
    hidden_key(&BUTTON_TEXT_TOKEN)
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

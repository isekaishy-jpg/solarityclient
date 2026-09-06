//! Synchronous, reentrant AddOn source loading into the existing Lua/object owner.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use mlua::{Lua, Table, Value};
use solarity_asset::AssetStoreHandle;

use crate::xml::UiSourceImage;
use crate::{
    AddonDefinition, FontCatalog, FontDefinition, UiAddonLoadState, UiBundle, UiLoadAction,
    UiObjectCatalog, UiResourceContent, UiRuntimeTemplatePlan,
};

use super::super::templates::TEMPLATE_REGISTRY;
use super::cvars::UiCVarRegistry;
use super::{
    DynamicArenaState, UiScriptEnvironment, create_dynamic_frame, dispatch_subscribers,
    object_type_name, register_font, restore_event_globals,
};

pub(super) fn install(
    lua: &Lua,
    sources: Rc<UiSourceImage>,
    environment: &UiScriptEnvironment,
    arena: DynamicArenaState,
    fonts: Rc<RefCell<HashMap<String, FontDefinition>>>,
    font_metatable: Table,
) -> mlua::Result<()> {
    let loader = Loader {
        sources: RefCell::new(sources),
        assets: environment.assets(),
        state: environment.addon_load_state(),
        cvars: environment.cvars(),
        world: environment.world.clone(),
        saved_variables: environment.saved_variable_state(),
        arena,
        fonts,
        font_metatable,
    };
    lua.globals().raw_set(
        "LoadAddOn",
        lua.create_function(move |lua, identifier: Value| {
            let Some(index) = super::globals::addon_index_from_value(
                &loader.state,
                &identifier,
                "Usage: LoadAddOn(index or \"name\")",
            )?
            else {
                return Ok((None::<u8>, Some("MISSING".to_owned())));
            };
            let addon = loader
                .state
                .definition_by_index(index)
                .ok_or_else(|| mlua::Error::runtime("AddOn catalog changed during lookup"))?;
            let reason = loader.load(lua, &addon)?;
            Ok((reason.is_none().then_some(1_u8), reason))
        })?,
    )
}

struct Loader {
    sources: RefCell<Rc<UiSourceImage>>,
    assets: Option<AssetStoreHandle>,
    state: UiAddonLoadState,
    cvars: UiCVarRegistry,
    world: crate::UiWorldState,
    saved_variables: crate::UiSavedVariableState,
    arena: DynamicArenaState,
    fonts: Rc<RefCell<HashMap<String, FontDefinition>>>,
    font_metatable: Table,
}

impl Loader {
    /// Native 0x005F7E90 permits dependency cycles while checking the rest of
    /// each dependency graph; disabled and incompatible dependencies still fail.
    fn failure(&self, name: &str, checking: &mut HashSet<String>) -> Option<String> {
        if self
            .state
            .status_by_name(name)
            .is_some_and(|(loaded, _)| loaded)
        {
            return None;
        }
        let Some(index) = self.state.index_by_name(name) else {
            return Some("MISSING".to_owned());
        };
        if !checking.insert(name.to_ascii_lowercase()) {
            return None;
        }
        let result = (|| {
            let addon = self.state.definition_by_index(index)?;
            let character = self.world.player_identity();
            if self
                .state
                .enable_state(character.as_ref().map(|player| player.name()), index)
                == Some(0)
            {
                return Some("DISABLED".to_owned());
            }
            if addon.interface().unwrap_or(0) < 20_000 {
                return Some("INCOMPATIBLE".to_owned());
            }
            if addon.interface() != Some(crate::STOCK_INTERFACE_VERSION)
                && self
                    .cvars
                    .get("checkAddonVersion")
                    .is_some_and(|value| value != "0")
            {
                return Some("INTERFACE_VERSION".to_owned());
            }
            for dependency in addon.dependencies() {
                if let Some(reason) = self.failure(dependency, checking) {
                    return Some(format!(
                        "DEP_{}",
                        reason.strip_prefix("DEP_").unwrap_or(&reason)
                    ));
                }
            }
            None
        })();
        checking.remove(&name.to_ascii_lowercase());
        result
    }

    /// 0x005F80B0 marks loading before optional/required dependencies and marks
    /// completion before ADDON_LOADED. Recursive calls return success once
    /// loading begins, and source callbacks never hold the loader's RefCell.
    fn load(&self, lua: &Lua, addon: &AddonDefinition) -> mlua::Result<Option<String>> {
        if self
            .state
            .status_by_name(addon.name())
            .is_some_and(|(loaded, _)| loaded)
        {
            return Ok(None);
        }
        if let Some(reason) = self.failure(addon.name(), &mut HashSet::new()) {
            return Ok(Some(reason));
        }
        self.state.set_status(addon.name(), true, false);
        for dependency in addon.optional_dependencies() {
            if let Some(definition) = self.state.definition_by_name(dependency) {
                let _reason = self.load(lua, &definition)?;
            }
        }
        for dependency in addon.dependencies() {
            let Some(definition) = self.state.definition_by_name(dependency) else {
                self.state.set_status(addon.name(), false, false);
                return Ok(Some("DEP_MISSING".to_owned()));
            };
            if let Some(reason) = self.load(lua, &definition)? {
                self.state.set_status(addon.name(), false, false);
                return Ok(Some(format!(
                    "DEP_{}",
                    reason.strip_prefix("DEP_").unwrap_or(&reason)
                )));
            }
        }
        self.execute(lua, addon)?;
        for name in addon.saved_variables() {
            self.saved_variables.register_account(name);
        }
        for name in addon.saved_variables_per_character() {
            self.saved_variables.register_character(name);
        }
        self.state.set_status(addon.name(), true, true);
        self.loaded_event(lua, addon.name())?;
        Ok(None)
    }

    fn execute(&self, lua: &Lua, addon: &AddonDefinition) -> mlua::Result<()> {
        let assets = self.assets.as_ref().ok_or_else(|| {
            mlua::Error::runtime("LoadAddOn requires the mounted client asset store")
        })?;
        let sources = self.sources.borrow().clone();
        let module = sources
            .load_addon(&mut assets.borrow_mut(), lua, addon)
            .map_err(script_error)?;
        let module = UiBundle::from_source_image(lua, Rc::new(module));
        let private = lua.create_table()?;
        for action in module.actions() {
            match action {
                UiLoadAction::LuaResource { resource_index } => {
                    let resource = module
                        .resource(*resource_index)
                        .ok_or_else(|| mlua::Error::runtime("AddOn Lua resource is unavailable"))?;
                    let UiResourceContent::Lua(source) = resource.content() else {
                        return Err(mlua::Error::runtime("AddOn Lua action refers to XML"));
                    };
                    lua.load(source.as_str())
                        .set_name(resource.path().as_str())
                        .call::<()>((addon.name(), private.clone()))?;
                }
                UiLoadAction::InlineLua { path, source } => {
                    lua.load(source).set_name(path.as_str()).exec()?;
                }
                UiLoadAction::XmlElement {
                    resource_index,
                    element_index,
                } => {
                    let resource = module
                        .resource(*resource_index)
                        .ok_or_else(|| mlua::Error::runtime("AddOn XML resource is unavailable"))?;
                    let next = Rc::new(
                        self.sources
                            .borrow()
                            .append_declaration(resource, *element_index),
                    );
                    let declarations = UiBundle::from_source_image(lua, next.clone());
                    let action_index = declarations.actions().len() - 1;
                    let fonts = FontCatalog::from_bundle(&declarations).map_err(script_error)?;
                    let catalog = UiObjectCatalog::from_bundle(&declarations, &fonts)
                        .map_err(script_error)?;
                    *self.sources.borrow_mut() = next;
                    if let Some(font) = fonts.definition_for_action(action_index) {
                        self.fonts
                            .borrow_mut()
                            .insert(font.name().to_owned(), font.clone());
                        register_font(lua, &self.font_metatable, font).map_err(script_error)?;
                    } else if let Some(definition) = catalog
                        .definitions()
                        .iter()
                        .find(|definition| definition.action_index() == action_index)
                    {
                        let plan =
                            UiRuntimeTemplatePlan::for_action(&catalog, &fonts, lua, action_index)
                                .map_err(script_error)?;
                        let descriptors = plan.descriptors(lua)?;
                        let descriptor: Table = descriptors.raw_get(definition.name())?;
                        if definition.virtual_object() {
                            let registry: Table = lua.named_registry_value(TEMPLATE_REGISTRY)?;
                            registry.raw_set(definition.name(), descriptor)?;
                        } else {
                            let parent = definition
                                .parent_name()
                                .map(|name| lua.globals().raw_get::<Table>(name))
                                .transpose()?;
                            create_dynamic_frame(
                                lua,
                                object_type_name(definition.kind()),
                                Some(definition.name()),
                                parent,
                                Some(descriptor),
                                &self.arena.counters(),
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn loaded_event(&self, lua: &Lua, name: &str) -> mlua::Result<()> {
        let globals = lua.globals();
        let previous_event = globals.raw_get::<Value>("event")?;
        let mut previous = Vec::with_capacity(crate::event::LEGACY_EVENT_ARGUMENT_GLOBALS);
        for index in 1..=crate::event::LEGACY_EVENT_ARGUMENT_GLOBALS {
            previous.push(globals.raw_get::<Value>(format!("arg{index}"))?);
        }
        globals.raw_set("event", "ADDON_LOADED")?;
        let argument = Value::String(lua.create_string(name)?);
        for index in 1..=crate::event::LEGACY_EVENT_ARGUMENT_GLOBALS {
            globals.raw_set(
                format!("arg{index}"),
                if index == 1 {
                    argument.clone()
                } else {
                    Value::Nil
                },
            )?;
        }
        let result = dispatch_subscribers(
            lua,
            self.arena.static_object_count + self.arena.dynamic_objects.get(),
            "ADDON_LOADED",
            &[argument],
        );
        let restore = restore_event_globals(lua, previous_event, previous);
        result.map(|_| ()).and(restore)
    }
}

fn script_error(error: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(error.to_string())
}
